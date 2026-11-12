// Live updates of the open issue (ISS-048, plan §3.10): the page follows the issue's topic. Changes
// of the issue, its comments and files go through the store; link changes reload the links panel;
// every event fetches the newer activity, which the stream appends or keeps for «Показать». After
// a reconnect the gateway replays what was missed; only when it cannot (`resync`) the page shows
// that it may be out of date.
import type { LiveEvent } from '#lib/api/schema.gen.ts';
import type { LiveConnection } from '#lib/live/connection.ts';
import { liveConnection, liveSync, store } from '#lib/store/index.ts';

import type { ActivityStream } from '../activity/activity.svelte.ts';
import type { IssueFiles } from '../files/files.svelte.ts';
import type { IssueLinksState } from '../links/links.svelte.ts';

export type LiveSource = Pick<LiveConnection, 'subscribe'>;

export interface IssueLiveParts {
  stream: ActivityStream | null;
  links: IssueLinksState | null;
  files: IssueFiles | null;
}

export function issueTopic(issueId: number): string {
  return `issue:${String(issueId)}`;
}

function quietly(work: Promise<unknown>): void {
  // A failed refresh keeps what the page shows; the next event tries again.
  work.catch(() => undefined);
}

export class IssueLive {
  /** Missed changes could not be replayed: the page may be out of date. */
  stale = $state(false);

  readonly #issueId: number;
  readonly #parts: IssueLiveParts;

  constructor(issueId: number, parts: IssueLiveParts) {
    this.#issueId = issueId;
    this.#parts = parts;
  }

  /** Follows the issue's topic until the returned function is called. */
  follow(connection: LiveSource = liveConnection()): () => void {
    return liveSync.follow(connection, issueTopic(this.#issueId), {
      event: (event) => {
        this.#handle(event);
      },
      resync: () => {
        this.stale = true;
        this.#refreshIssue();
        this.#catchUp();
      },
      // The topic is refused once the issue is gone or hidden: the store drops it on a refetch.
      denied: () => {
        this.#refreshIssue();
      },
    });
  }

  /** Loads everything the page shows again (the hint's «Обновить»). */
  async refresh(): Promise<void> {
    const { stream, links, files } = this.#parts;
    await Promise.all([
      store.refetch('issue', [this.#issueId]),
      stream?.reload(),
      links?.reload(),
      files?.reload(),
    ]);
    this.stale = false;
  }

  #handle(event: LiveEvent): void {
    const { kind, id } = event.entity;
    const { links, files } = this.#parts;
    if (kind === 'issue' && event.change === 'deleted') return;
    if (kind === 'issue_link' && links) quietly(links.reload());
    if (kind === 'attachment' && files) {
      if (event.change === 'deleted') files.forget(id);
      else if (event.change === 'created') quietly(files.fetchOne(id));
    }
    this.#catchUp();
  }

  #refreshIssue(): void {
    quietly(store.refetch('issue', [this.#issueId]));
  }

  #catchUp(): void {
    const { stream } = this.#parts;
    if (stream) quietly(stream.catchUp(stream.atEnd));
  }
}
