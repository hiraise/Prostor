// The word diff of the conflict view (ISS-051): what changed between the current text and the
// person's. Tokens are words and runs of whitespace; the shortest edit script by Myers' O(ND)
// algorithm. Very long or very different texts are shown as one removal and one addition instead.

export interface DiffPart {
  op: 'same' | 'added' | 'removed';
  text: string;
}

/** Above this many tokens on either side the diff is not computed. */
export const MAX_TOKENS = 20_000;
/** Texts further apart than this many token edits are shown whole. */
const MAX_EDITS = 2_000;

function tokens(text: string): string[] {
  return text.match(/\s+|[^\s]+/g) ?? [];
}

/** Adds a part, merging it with the previous one of the same kind. */
function push(parts: DiffPart[], op: DiffPart['op'], text: string): void {
  const last = parts.at(-1);
  if (last?.op === op) last.text += text;
  else parts.push({ op, text });
}

function whole(before: string, after: string): DiffPart[] {
  const parts: DiffPart[] = [];
  if (before) parts.push({ op: 'removed', text: before });
  if (after) parts.push({ op: 'added', text: after });
  return parts;
}

export function diffWords(before: string, after: string): DiffPart[] {
  if (before === after) return before ? [{ op: 'same', text: before }] : [];
  const a = tokens(before);
  const b = tokens(after);
  if (a.length > MAX_TOKENS || b.length > MAX_TOKENS) return whole(before, after);

  // The common head and tail are kept out of the search.
  let head = 0;
  while (head < a.length && head < b.length && a[head] === b[head]) head += 1;
  let tail = 0;
  while (
    tail < a.length - head &&
    tail < b.length - head &&
    a[a.length - 1 - tail] === b[b.length - 1 - tail]
  ) {
    tail += 1;
  }
  const x = a.slice(head, a.length - tail);
  const y = b.slice(head, b.length - tail);

  const steps = middle(x, y);
  if (!steps) return whole(before, after);
  const parts: DiffPart[] = [];
  if (head > 0) push(parts, 'same', a.slice(0, head).join(''));
  for (const part of tidy(steps)) push(parts, part.op, part.text);
  if (tail > 0) push(parts, 'same', a.slice(a.length - tail).join(''));
  return parts;
}

/** Whitespace kept between two changes joins them, and each change reads as all that was removed,
 * then all that was added: «-b c +x b» rather than «-b +x · -c +b». */
function tidy(steps: [DiffPart['op'], string][]): DiffPart[] {
  const parts: DiffPart[] = [];
  let removed = '';
  let added = '';
  const flush = (): void => {
    if (removed) parts.push({ op: 'removed', text: removed });
    if (added) parts.push({ op: 'added', text: added });
    removed = '';
    added = '';
  };
  steps.forEach(([op, text], index) => {
    const inside =
      op === 'same' &&
      /^\s+$/.test(text) &&
      (removed || added) !== '' &&
      (steps[index + 1]?.[0] ?? 'same') !== 'same';
    if (op === 'removed' || inside) removed += text;
    if (op === 'added' || inside) added += text;
    if (op === 'same' && !inside) {
      flush();
      parts.push({ op, text });
    }
  });
  flush();
  return parts;
}

/** Myers' greedy forward search; each round keeps its furthest points (k from -d to d) for the
 * way back. `null` past `MAX_EDITS`, so memory stays small (about MAX_EDITS² numbers). */
function middle(a: string[], b: string[]): [DiffPart['op'], string][] | null {
  const n = a.length;
  const m = b.length;
  if (n === 0) return b.map((text) => ['added', text]);
  if (m === 0) return a.map((text) => ['removed', text]);
  const rounds: Int32Array[] = [];
  let previous = new Int32Array(0);
  const at = (round: Int32Array, d: number, k: number): number => round[k + d] ?? 0;
  for (let d = 0; d <= Math.min(n + m, MAX_EDITS); d += 1) {
    const current = new Int32Array(2 * d + 1);
    for (let k = -d; k <= d; k += 2) {
      let i: number;
      if (d === 0) i = 0;
      else if (k === -d || (k !== d && at(previous, d - 1, k - 1) < at(previous, d - 1, k + 1))) {
        i = at(previous, d - 1, k + 1);
      } else {
        i = at(previous, d - 1, k - 1) + 1;
      }
      let j = i - k;
      while (i < n && j < m && a[i] === b[j]) {
        i += 1;
        j += 1;
      }
      current[k + d] = i;
      if (i >= n && j >= m) {
        rounds.push(current);
        return back(a, b, rounds);
      }
    }
    rounds.push(current);
    previous = current;
  }
  return null;
}

/** The path from the end back to the start: each round is one removal or addition after a run
 * of equal tokens. */
function back(a: string[], b: string[], rounds: Int32Array[]): [DiffPart['op'], string][] {
  const steps: [DiffPart['op'], string][] = [];
  let i = a.length;
  let j = b.length;
  for (let d = rounds.length - 1; d > 0; d -= 1) {
    const previous = rounds[d - 1] ?? new Int32Array(0);
    const at = (k: number): number => previous[k + d - 1] ?? 0;
    const k = i - j;
    const down = k === -d || (k !== d && at(k - 1) < at(k + 1));
    const previousK = down ? k + 1 : k - 1;
    const previousI = at(previousK);
    const previousJ = previousI - previousK;
    while (i > previousI && j > previousJ) {
      i -= 1;
      j -= 1;
      steps.push(['same', a[i] ?? '']);
    }
    if (down) {
      j -= 1;
      steps.push(['added', b[j] ?? '']);
    } else {
      i -= 1;
      steps.push(['removed', a[i] ?? '']);
    }
  }
  while (i > 0 && j > 0) {
    i -= 1;
    j -= 1;
    steps.push(['same', a[i] ?? '']);
  }
  return steps.reverse();
}
