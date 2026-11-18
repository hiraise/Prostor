// The bindings of the issue list (step 2.2 §4): the list holds focus, on top of global. The list
// context lands with part 2.2a-2; the keys are fixed here (the table is the single source).
import type { Binding } from '../kinds.ts';

export const listTable: readonly Binding[] = [
  { id: 'list.up', contexts: ['list'], combo: { code: 'ArrowUp' } },
  { id: 'list.down', contexts: ['list'], combo: { code: 'ArrowDown' } },
  { id: 'list.home', contexts: ['list'], combo: { code: 'Home' } },
  { id: 'list.end', contexts: ['list'], combo: { code: 'End' } },
  { id: 'list.page-prev', contexts: ['list'], combo: { code: 'Comma' } },
  { id: 'list.page-next', contexts: ['list'], combo: { code: 'Period' } },
  { id: 'list.open', contexts: ['list'], combo: { code: 'Enter' } },
  { id: 'list.open.key', contexts: ['list'], combo: { code: 'KeyO' } },
  { id: 'list.open.new', contexts: ['list'], combo: { code: 'Enter', modifiers: ['primary'] } },
  { id: 'list.edit', contexts: ['list'], combo: { code: 'KeyE' } },
  { id: 'list.select', contexts: ['list'], combo: { code: 'KeyX' } },
  { id: 'list.select.up', contexts: ['list'], combo: { code: 'ArrowUp', shift: true } },
  { id: 'list.select.down', contexts: ['list'], combo: { code: 'ArrowDown', shift: true } },
  { id: 'list.select.page', contexts: ['list'], combo: { code: 'KeyA', modifiers: ['primary'] } },
  { id: 'list.assign-me', contexts: ['list'], combo: { code: 'KeyM' } },
  { id: 'list.star', contexts: ['list'], combo: { code: 'KeyS' } },
  { id: 'list.sidebar', contexts: ['list'], combo: { code: 'KeyB' } },
];
