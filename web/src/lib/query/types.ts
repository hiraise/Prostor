// What the browser's copy of the query language answers (D-038). Classes and codes are the ones
// the server's `GET /queries/check` uses; offsets are UTF-16 code units, as strings index.

import type { QueryProblemCode, TokenClass } from '#lib/api/schema.gen.ts';

export type { QueryProblemCode, TokenClass };

export interface QueryToken {
  from: number;
  to: number;
  class: TokenClass;
}

export interface SyntaxProblem {
  from: number;
  to: number;
  code: QueryProblemCode;
  severity: 'error' | 'note';
}

/** The syntax half of a check: no names are resolved (that needs the server). */
export interface Analysis {
  tokens: QueryToken[];
  problems: SyntaxProblem[];
  /** The query written canonically. */
  canonical: string;
  /** Where the sort clause is, to replace it (a header click); `null` without one. */
  sort: { from: number; to: number } | null;
}

export interface QueryEngine {
  analyze(text: string): Analysis;
}
