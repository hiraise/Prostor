// The query language in the browser (D-038): a WebAssembly module built from the Rust crate the
// server runs (`make wasm`), loaded on first use. It answers instantly while a query is typed;
// names (fields, people, values) still need the server's check.
//
// The module has no bindings: `alloc(len)` reserves bytes for the query, `analyze(ptr, len)`
// returns the answer's address and length packed into one u64 (a BigInt here), `free(ptr, len)`
// returns either.

import type { Analysis, QueryEngine } from './types.ts';

interface Exports {
  memory: WebAssembly.Memory;
  alloc(len: number): number;
  free(ptr: number, len: number): void;
  analyze(ptr: number, len: number): bigint;
}

const encoder = new TextEncoder();
const decoder = new TextDecoder();

/** Wraps an instantiated module. */
export function engineFrom(instance: WebAssembly.Instance): QueryEngine {
  const wasm = instance.exports as unknown as Exports;
  return {
    analyze(text: string): Analysis {
      const input = encoder.encode(text);
      const at = input.length > 0 ? wasm.alloc(input.length) : 0;
      if (input.length > 0) {
        if (at === 0) throw new Error('query engine: out of memory');
        new Uint8Array(wasm.memory.buffer, at, input.length).set(input);
      }
      const packed = wasm.analyze(at, input.length);
      if (at !== 0) wasm.free(at, input.length);
      const out = Number(packed >> 32n);
      const length = Number(packed & 0xffff_ffffn);
      if (out === 0) throw new Error('query engine: out of memory');
      // Read before freeing; the memory may grow (and move) on the next call.
      const json = decoder.decode(new Uint8Array(wasm.memory.buffer, out, length));
      wasm.free(out, length);
      return JSON.parse(json) as Analysis;
    },
  };
}

let loading: Promise<QueryEngine> | null = null;

async function instantiate(): Promise<QueryEngine> {
  const { default: url } = await import('./query.wasm?url');
  const { instance } = await WebAssembly.instantiateStreaming(fetch(url));
  return engineFrom(instance);
}

/** The engine, downloaded once; a failed download is tried again on the next call. */
export function loadQueryEngine(): Promise<QueryEngine> {
  loading ??= instantiate().catch((error: unknown) => {
    loading = null;
    throw error;
  });
  return loading;
}
