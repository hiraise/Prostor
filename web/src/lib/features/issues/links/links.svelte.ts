// The links of an issue (LNK-005…LNK-015): the panel's groups with their paging, adding and
// removing links, the names people choose from, issue search for the link dialog with the
// person's recent issues before typing, and new issues created already linked (LNK-006, ISS-009).
import { api } from '#lib/api/client.ts';
import { showRuleMessages } from '#lib/ui/rule-messages.ts';
import type {
  Issue,
  IssueLinks,
  LinkGroup,
  LinkSide,
  LinkType,
  LinksAdded,
  NewIssueLink,
} from '#lib/api/schema.gen.ts';
import { preferences } from '#lib/session/index.ts';
import { store } from '#lib/store/index.ts';

import { issueRow } from '../issues.ts';

/** Issues one search of the link dialog shows. */
const FOUND = 10;
/** Rows one «Ещё» of a group adds. */
const GROUP_PAGE = 50;

/** One name of a link type, as people choose it: the side the issue takes. */
export interface LinkName {
  type: LinkType;
  side: LinkSide;
  name: string;
}

export function nameKey(name: Pick<LinkName, 'side'> & { type: { id: number } }): string {
  return `${String(name.type.id)}:${name.side}`;
}

function groupKey(group: LinkGroup): string {
  return `${String(group.link_type.id)}:${group.side}`;
}

let types: Promise<LinkType[]> | null = null;

/** The link types in use, loaded once per app run (oldest first, as the server lists them). */
export function linkTypes(): Promise<LinkType[]> {
  types ??= api.get('/link-types').then(
    (list) => list.items.filter((type) => type.deleted_at === null),
    (error: unknown) => {
      types = null;
      throw error;
    },
  );
  return types;
}

/** Forgets the loaded types (tests). */
export function forgetLinkTypes(): void {
  types = null;
}

/** Every name of every type: the source side, then the target side of directed types. */
export function linkNames(list: LinkType[]): LinkName[] {
  return list.flatMap((type) => {
    const names: LinkName[] = [{ type, side: 'source', name: type.source_name }];
    if (type.target_name !== null) names.push({ type, side: 'target', name: type.target_name });
    return names;
  });
}

/** «Подзадача для»: a new issue is a subtask of the parent (the target of the subtask type). */
export function subtaskLink(list: LinkType[], parent: number): NewIssueLink | null {
  const type = list.find((entry) => entry.builtin === 'subtask');
  return type ? { issue: parent, link_type: type.id, side: 'target' } : null;
}

/** The name «дубликат» this issue takes when it duplicates another (LNK-015). */
export function duplicateName(list: LinkType[]): LinkName | null {
  const type = list.find((entry) => entry.builtin === 'duplicate');
  if (!type?.target_name) return null;
  return { type, side: 'target', name: type.target_name };
}

/** The link a new issue gets when `issue` names it by `name` (ISS-009, LNK-006). */
export function newIssueLink(issue: number, name: LinkName): NewIssueLink {
  const side = name.type.direction === 'undirected' || name.side === 'target' ? 'source' : 'target';
  return { issue, link_type: name.type.id, side };
}

/** The new-issue form's `?link=<type>:<side>:<issue>` (ISS-009, LNK-006). */
export function linkParam(link: NewIssueLink): string {
  return `${String(link.link_type)}:${link.side}:${String(link.issue)}`;
}

export function parseLinkParam(text: string | null | undefined): NewIssueLink | null {
  const match = /^(\d+):(source|target):(\d+)$/.exec(text ?? '');
  if (!match) return null;
  return {
    link_type: Number(match[1]),
    side: match[2] as LinkSide,
    issue: Number(match[3]),
  };
}

/** Characters a query attribute cannot hold unquoted. */
const SYNTAX = /[:(){}"#,]/;

/**
 * The query of «Открыть списком» (QRY-030): the issues linked by a group are those that take the
 * other side's name towards this issue; «Все» is any link. `null` for a name the query cannot
 * spell.
 */
export function listQuery(readable: string, group?: LinkGroup): string | null {
  if (!group) return `links: ${readable}`;
  const type = group.link_type;
  const name = group.side === 'source' ? (type.target_name ?? type.source_name) : type.source_name;
  if (SYNTAX.test(name)) return null;
  return `${name}: ${readable}`;
}

/** The links of one issue as its page shows them; refreshed by its own changes and live events. */
export class IssueLinksState {
  groups = $state<LinkGroup[]>([]);
  canLink = $state(false);
  /** The server's links are known (not just the empty start of a draft's form). */
  loaded = $state(false);
  /** Groups whose next page is loading, by `type:side`. */
  loading = $state<string[]>([]);

  readonly #issue: number;

  constructor(issue: number, links: IssueLinks, loaded = true) {
    this.#issue = issue;
    this.#take(links);
    this.loaded = loaded;
  }

  get total(): number {
    return this.groups.reduce((sum, group) => sum + group.total, 0);
  }

  isLoading(group: LinkGroup): boolean {
    return this.loading.includes(groupKey(group));
  }

  #take(links: IssueLinks): void {
    this.groups = links.groups;
    this.canLink = links.can_link;
    this.loaded = true;
  }

  /** The next rows of a group («Ещё»). */
  async more(group: LinkGroup): Promise<void> {
    const cursor = group.next_cursor;
    if (cursor === null) return;
    const key = groupKey(group);
    this.loading = [...this.loading, key];
    try {
      const page = await api.get('/issues/{id}/links/{link_type}/{side}', {
        path: { id: this.#issue, link_type: group.link_type.id, side: group.side },
        query: { cursor, limit: GROUP_PAGE },
      });
      this.groups = this.groups.map((shown) =>
        groupKey(shown) === key
          ? {
              ...page,
              items: [
                ...shown.items,
                ...page.items.filter((row) => !shown.items.some((had) => had.link === row.link)),
              ],
            }
          : shown,
      );
    } finally {
      this.loading = this.loading.filter((other) => other !== key);
    }
  }

  async reload(): Promise<void> {
    this.#take(await api.get('/issues/{id}/links', { path: { id: this.#issue } }));
  }

  /** Links issues by a name; the answer names those linked before. */
  async add(name: Pick<LinkName, 'side'> & { type: { id: number } }, issues: number[]) {
    const answer: LinksAdded = await api.post('/issues/{id}/links', {
      path: { id: this.#issue },
      body: { link_type: name.type.id, side: name.side, issues },
    });
    showRuleMessages(answer.messages);
    this.#take(answer.links);
    await refreshEnds([this.#issue, ...answer.added]);
    return answer;
  }

  /** Removes a link from both issues (LNK-008). */
  async remove(link: number): Promise<void> {
    const before = this.groups;
    const other = before.flatMap((group) => group.items).find((row) => row.link === link);
    this.groups = before
      .map((group) =>
        group.items.some((row) => row.link === link)
          ? { ...group, total: group.total - 1, items: group.items.filter((r) => r.link !== link) }
          : group,
      )
      .filter((group) => group.total > 0);
    try {
      // The answer carries what the link's rules did (§3.11); removals answer 200.
      const answer = await api.delete('/issues/{id}/links/{link}', {
        path: { id: this.#issue, link },
      });
      showRuleMessages(answer.messages);
    } catch (error) {
      this.groups = before;
      throw error;
    }
    await refreshEnds(other ? [this.#issue, other.issue.id] : [this.#issue]);
  }
}

/** The parent and subtask counts of both ends change with a link (LNK-013, LNK-014). */
function refreshEnds(ids: number[]): Promise<void> {
  return store.refetch('issue', ids).catch(() => undefined);
}

/** Issues the search finds for the link dialog, stored as rows (LNK-005). */
export async function findIssues(text: string, signal?: AbortSignal): Promise<Issue[]> {
  const page = await api.get('/issues', { query: { q: text, limit: FOUND }, signal });
  store.put('issue', page.items);
  return page.items;
}

/** The person's recently opened issues, loaded in one batch where the store lacks them. */
export async function recentIssues(signal?: AbortSignal): Promise<Issue[]> {
  const ids = preferences.value('recent_issues');
  const missing = ids.filter((id) => !issueRow(id));
  if (missing.length > 0) {
    const page = await api.post('/issues/batch', { body: { ids: missing }, signal });
    store.put('issue', page.items);
  }
  return ids.map((id) => issueRow(id)).filter((issue): issue is Issue => issue !== undefined);
}

/** Linked issues as rows, for the star and the priority mark of the panel (LNK-010). */
export async function loadLinked(ids: number[]): Promise<void> {
  const missing = ids.filter((id, at) => ids.indexOf(id) === at && !issueRow(id));
  if (missing.length === 0) return;
  const page = await api.post('/issues/batch', { body: { ids: missing } });
  store.put('issue', page.items);
}

/**
 * Creates a published issue linked to `issue` by a name of `issue`'s side (LNK-006). A draft
 * cannot be named by a new issue's links, so the draft links to it afterwards (LNK-007).
 */
export async function createLinked(
  project: number,
  summary: string,
  issue: Issue,
  name: LinkName,
  links: IssueLinksState,
): Promise<Issue> {
  if (issue.draft) {
    const answer = await api.post('/issues', { body: { project, summary } });
    showRuleMessages(answer.messages);
    store.put('issue', [answer.issue]);
    await links.add(name, [answer.issue.id]);
    return answer.issue;
  }
  const answer = await api.post('/issues', {
    body: {
      project,
      summary,
      links: [newIssueLink(issue.id, name)],
    },
  });
  showRuleMessages(answer.messages);
  store.put('issue', [answer.issue]);
  await links.reload();
  await refreshEnds([issue.id]);
  return answer.issue;
}
