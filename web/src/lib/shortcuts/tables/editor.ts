// The bindings of the Markdown editor (step 2.2 §4): `content`'s MarkdownEditor focused. While
// it is, letters go to the text — the editor table holds only modifier combos and Escape (§3).
// The editor context lands with part 2.2a-3; the keys are fixed here.
import type { Binding } from '../kinds.ts';

export const editorTable: readonly Binding[] = [
  { id: 'editor.bold', contexts: ['editor'], combo: { code: 'KeyB', modifiers: ['primary'] } },
  { id: 'editor.italic', contexts: ['editor'], combo: { code: 'KeyI', modifiers: ['primary'] } },
  {
    id: 'editor.strike',
    contexts: ['editor'],
    combo: { code: 'KeyS', modifiers: ['primary'], shift: true },
  },
  { id: 'editor.code', contexts: ['editor'], combo: { code: 'KeyE', modifiers: ['primary'] } },
  { id: 'editor.submit', contexts: ['editor'], combo: { code: 'Enter', modifiers: ['primary'] } },
  { id: 'editor.cancel', contexts: ['editor'], combo: { code: 'Escape' } },
];
