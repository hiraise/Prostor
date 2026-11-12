// Issues in the data store (architecture.md §15) and the calls of the issue page (ISS-001…ISS-003,
// UI-006). The short form is what lists and live refetches need (`POST /issues/batch`, one request
// for many); the full form adds what one page needs: references of the texts and the parent. The
// page opens with one request (`GET /issues/{id}/page`): the issue, its comments and files go into
// the store, the rest stays the page's own data.
import { resolve } from '$app/paths';

import { ApiError, api } from '#lib/api/client.ts';
import type {
  ActivityPage,
  AttachmentPage,
  Issue,
  IssueBoard,
  IssueBrief,
  IssueField,
  IssueLinks,
  IssuePage,
} from '#lib/api/schema.gen.ts';
import { preferences } from '#lib/session/index.ts';
import { store, type EntityHandle } from '#lib/store/index.ts';

import { firstPageQuery } from './activity/activity.svelte.ts';
import './content.ts';

declare module '#lib/store/index.ts' {
  interface EntityKinds {
    issue: Issue;
  }
}

/** The most IDs one batch request takes. */
const BATCH = 100;
/** Issues kept in `recent_issues` (LNK-005). */
const RECENT = 20;
/** How long an opened page's answer serves another load of the same address (a hover preload). */
const PAGE_FRESH_MS = 10_000;

store.defineKind('issue', {
  async load(ids, form) {
    if (form === 'full') {
      const answers = await Promise.all(
        ids.map((id) =>
          api
            .get('/issues/{id}', { path: { id: String(id) }, accept: [404] })
            .then((answer) => ('id' in answer ? answer : null)),
        ),
      );
      return answers.filter((issue): issue is Issue => issue !== null);
    }
    const chunks: number[][] = [];
    for (let start = 0; start < ids.length; start += BATCH) {
      chunks.push(ids.slice(start, start + BATCH));
    }
    const pages = await Promise.all(
      chunks.map((chunk) => api.post('/issues/batch', { body: { ids: chunk } })),
    );
    return pages.flatMap((page) => page.items);
  },
});

/** What the issue page shows besides the issue; the comments and files in it are also in the store. */
export interface IssuePageData {
  activity: ActivityPage;
  links: IssueLinks;
  attachments: AttachmentPage;
  /** The boards and sprints holding the issue (ISS-020). */
  boards?: IssueBoard[];
  /** The comment of the deep link the stream was opened around. */
  focusComment?: number;
}

/** What the address of an issue leads to. */
export type OpenedIssue =
  | { kind: 'found'; issue: EntityHandle<Issue>; page: IssuePageData }
  /** The address used a former ID or the internal one: the page shows the current ID (ISS-003). */
  | { kind: 'moved'; issue: EntityHandle<Issue>; page: IssuePageData; readableId: string }
  | { kind: 'missing' };

/** Answers of recent page loads by address, so a preload on hover serves the navigation. */
const pages = new Map<string, { at: number; answer: Promise<IssuePage | null> }>();

function fetchPage(segment: string, aroundComment?: number): Promise<IssuePage | null> {
  const now = Date.now();
  // The stream's first page follows the person's filters and order (activity.svelte.ts).
  const query = firstPageQuery(aroundComment);
  const key = `${segment} ${JSON.stringify(query)}`;
  const cached = pages.get(key);
  if (cached && now - cached.at < PAGE_FRESH_MS) return cached.answer;
  const answer = api
    .get('/issues/{id}/page', { path: { id: segment }, query, accept: [404] })
    .then((found) => ('issue' in found ? found : null));
  pages.set(key, { at: now, answer });
  answer.catch(() => pages.delete(key));
  return answer;
}

/** Forgets the cached page answers, so the next open asks the server (tests, live changes). */
export function forgetIssuePages(): void {
  pages.clear();
}

/**
 * The issue of an address segment: its readable ID, a former one, or its internal ID. With
 * `aroundComment` (the `?comment=` deep link, CNT-007) the stream opens around that comment.
 */
export async function openIssue(segment: string, aroundComment?: number): Promise<OpenedIssue> {
  try {
    const answer = await fetchPage(segment, aroundComment);
    if (!answer) return { kind: 'missing' };
    const found = answer.issue;
    store.put('issue', [found], 'full');
    store.put(
      'comment',
      answer.activity.items.flatMap((item) => (item.type === 'comment' ? [item.comment] : [])),
      'full',
    );
    store.put('attachment', answer.attachments.attachments, 'full');
    const issue = await store.ensureEntity('issue', found.id, { form: 'full' });
    rememberRecent(found.id);
    const page: IssuePageData = {
      activity: answer.activity,
      links: answer.links,
      attachments: answer.attachments,
      boards: answer.boards,
      focusComment: aroundComment,
    };
    if (found.readable_id !== null && found.readable_id !== segment) {
      return { kind: 'moved', issue, page, readableId: found.readable_id };
    }
    return { kind: 'found', issue, page };
  } catch (error) {
    if (error instanceof ApiError && error.code === 'not_found') return { kind: 'missing' };
    throw error;
  }
}

/** Puts an opened issue first in the person's recent issues (LNK-005). */
function rememberRecent(id: number): void {
  const recent = preferences.value('recent_issues');
  if (recent[0] === id) return;
  preferences.set({
    recent_issues: [id, ...recent.filter((other) => other !== id)].slice(0, RECENT),
  });
}

/** The stored row of an issue, if loaded. Reactive. */
export function issueRow(id: number): Issue | undefined {
  return store.entities.table('issue').get(id);
}

/** The address of an issue page (UI-006): its readable ID, or the internal one for a draft. */
export function issueHref(issue: Pick<Issue, 'id' | 'readable_id'> | string): string {
  const id = typeof issue === 'string' ? issue : (issue.readable_id ?? String(issue.id));
  return resolve('/(app)/issues/[id]', { id });
}

/** The address of the issue list of a project (UI-005). */
export function projectIssuesHref(project: { key: string }): string {
  return `${resolve('/(app)/issues')}?project=${encodeURIComponent(project.key)}`;
}

/** One field of the issue, if the viewer sees it. */
export function fieldOf(issue: Pick<Issue, 'fields'>, fieldId: number): IssueField | undefined {
  return issue.fields.find((entry) => entry.field.id === fieldId);
}

/** An issue as references show it (IssueRef), from either form. */
export function briefOf(issue: Issue | IssueBrief): IssueBrief {
  if (!('fields' in issue)) return issue;
  return {
    id: issue.id,
    readable_id: issue.readable_id ?? String(issue.id),
    summary: issue.summary,
    project: issue.project,
    resolved: issue.resolved_at !== null,
    restricted: issue.audience.users.length > 0 || issue.audience.groups.length > 0,
  };
}
