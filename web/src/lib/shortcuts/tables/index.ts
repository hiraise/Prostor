// The binding tables (step 2.2 §3–§4): one module per context, static data. A module that loads
// later (boards in 2.7, …) adds its table — that registration plus the overlay's rendering of it
// is the whole of UI-024's mechanism.
import type { ContextId, Tables } from '../kinds.ts';
import { dialogTable } from './dialog.ts';
import { editorTable } from './editor.ts';
import { globalTable } from './global.ts';
import { issueTable } from './issue.ts';
import { listTable } from './list.ts';
import { queryTable } from './query.ts';

export { dialogTable } from './dialog.ts';
export { editorTable } from './editor.ts';
export { globalTable } from './global.ts';
export { issueTable } from './issue.ts';
export { listTable } from './list.ts';
export { queryTable } from './query.ts';

export const contextIds: readonly ContextId[] = ['list', 'issue', 'editor', 'dialog', 'query'];

export const tables: Tables = {
  global: globalTable,
  list: listTable,
  issue: issueTable,
  editor: editorTable,
  dialog: dialogTable,
  query: queryTable,
};
