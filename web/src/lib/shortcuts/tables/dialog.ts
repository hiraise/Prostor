// The bindings of the command dialog (step 2.2 §4): already shipped in 2.1, listed here so the
// overlay shows them. Part 2.2a-3 registers the dialog's context; the keys are fixed here.
import type { Binding } from '../kinds.ts';

export const dialogTable: readonly Binding[] = [
  { id: 'dialog.apply', contexts: ['dialog'], combo: { code: 'Enter' } },
  {
    id: 'dialog.apply.primary',
    contexts: ['dialog'],
    combo: { code: 'Enter', modifiers: ['primary'] },
  },
  {
    id: 'dialog.apply.silent',
    contexts: ['dialog'],
    combo: { code: 'Enter', modifiers: ['primary'], shift: true },
  },
  { id: 'dialog.cancel', contexts: ['dialog'], combo: { code: 'Escape' } },
];
