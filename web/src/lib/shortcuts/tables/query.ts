// The bindings of the query bar (step 2.2 §4): already shipped in 2.1, listed here so the
// overlay shows them. Part 2.2a-3 registers the bar's context; the keys are fixed here.
import type { Binding } from '../kinds.ts';

export const queryTable: readonly Binding[] = [
  { id: 'query.suggest', contexts: ['query'], combo: { code: 'Space', modifiers: ['primary'] } },
  { id: 'query.run', contexts: ['query'], combo: { code: 'Enter' } },
  { id: 'query.leave', contexts: ['query'], combo: { code: 'Escape' } },
];
