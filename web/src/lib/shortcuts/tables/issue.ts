// The bindings of the issue view (step 2.2 §4): the page holds focus, not while editing. The
// issue context lands with part 2.2a-3; the keys are fixed here.
import type { Binding } from '../kinds.ts';

export const issueTable: readonly Binding[] = [
  { id: 'issue.edit', contexts: ['issue'], combo: { code: 'KeyE' } },
  { id: 'issue.comment', contexts: ['issue'], combo: { code: 'KeyR' } },
  { id: 'issue.history', contexts: ['issue'], combo: { code: 'KeyH' } },
  { id: 'issue.comments', contexts: ['issue'], combo: { code: 'KeyU' } },
  { id: 'issue.assign-me', contexts: ['issue'], combo: { code: 'KeyM' } },
  { id: 'issue.star', contexts: ['issue'], combo: { code: 'KeyS' } },
  { id: 'issue.attach', contexts: ['issue'], combo: { code: 'KeyF' } },
  { id: 'issue.tags', contexts: ['issue'], combo: { code: 'KeyT' } },
];
