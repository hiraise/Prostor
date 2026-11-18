// The global bindings (step 2.2 §4): active everywhere, the letters suppressed while typing.
// One binding scheme, no scheme setting (D-063); the 2.1 combos are adopted unchanged.
import { canAdminister, canAnywhere } from '#lib/session/index.ts';

import type { Binding } from '../kinds.ts';

export const globalTable: readonly Binding[] = [
  { id: 'overlay', contexts: ['global'], combo: { code: 'Slash', shift: true }, display: '?' },
  { id: 'focus-query', contexts: ['global'], combo: { code: 'Slash' } },
  {
    id: 'create',
    contexts: ['global'],
    combo: { code: 'KeyC' },
    if: () => canAnywhere('issue.create'),
  },
  { id: 'copy.id', contexts: ['global'], combo: { code: 'KeyI' } },
  { id: 'copy.link', contexts: ['global'], combo: { code: 'KeyY' } },
  { id: 'copy.full', contexts: ['global'], combo: { code: 'KeyY', shift: true } },
  { id: 'command', contexts: ['global'], combo: { code: 'KeyC', modifiers: ['primary', 'alt'] } },
  {
    id: 'command.repeat',
    contexts: ['global'],
    combo: { code: 'KeyC', modifiers: ['primary', 'alt'], shift: true },
  },
  { id: 'rail', contexts: ['global'], combo: { code: 'KeyS', modifiers: ['primary', 'alt'] } },
  { id: 'go.issues', contexts: ['global'], combo: { first: 'KeyG', second: 'KeyI' } },
  { id: 'go.boards', contexts: ['global'], combo: { first: 'KeyG', second: 'KeyB' } },
  { id: 'go.projects', contexts: ['global'], combo: { first: 'KeyG', second: 'KeyP' } },
  {
    id: 'go.admin',
    contexts: ['global'],
    combo: { first: 'KeyG', second: 'KeyA' },
    if: canAdminister,
  },
];
