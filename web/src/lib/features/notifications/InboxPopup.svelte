<script lang="ts">
  // The inbox popup (NTF-047–NTF-050, plan step 1.10 §4.1): folder tabs, the unread-only toggle,
  // threads with their newest rows inline, «Показать ещё» for a whole thread and for the next
  // page, and the read state per row, per thread and everywhere at once. Rows that arrive over
  // the `inbox:` topic reload the page; the badge itself lives in `inbox.svelte.ts`. Following a
  // link into an issue closes the popup (`onnavigate`).
  import CheckCheck from '@lucide/svelte/icons/check-check';

  import type { InboxNotification, InboxThread } from '#lib/api/schema.gen.ts';
  import { canAdminister } from '#lib/session/index.ts';
  import Button from '#lib/ui/Button.svelte';
  import Checkbox from '#lib/ui/Checkbox.svelte';
  import EmptyState from '#lib/ui/EmptyState.svelte';
  import IconButton from '#lib/ui/IconButton.svelte';
  import Spinner from '#lib/ui/Spinner.svelte';
  import { showError } from '#lib/ui/toast.svelte.ts';

  import { listInbox, markInboxRead, onInboxRows, type InboxFolder } from './inbox.svelte.ts';
  import NotificationRow from './NotificationRow.svelte';
  import * as m from './messages.ts';

  interface Props {
    /** Closes the popup, e.g. because a link into an issue was followed. */
    onnavigate?: () => void;
  }

  let { onnavigate }: Props = $props();

  const folders: [InboxFolder, () => string][] = [
    ['all', m.folderAll],
    ['direct', m.folderDirect],
    ['subscriptions', m.folderSubscriptions],
  ];

  let folder = $state<InboxFolder>('all');
  let unreadOnly = $state(false);
  let threads = $state<InboxThread[]>([]);
  /** The rows of a thread the reader expanded with «Показать ещё», by the issue's id. */
  let expanded = $state<Record<number, InboxNotification[]>>({});
  let next = $state<number | null>(null);
  let loading = $state(false);
  let failed = $state(false);
  /** Grows with every load; an answer applies only when it is still the newest. */
  let version = 0;

  const tabs = $derived<[InboxFolder, () => string][]>(
    canAdminister() ? [...folders, ['system', m.folderSystem]] : folders,
  );

  const canExpand = (thread: InboxThread): boolean => {
    const issue = thread.head.issue_id;
    return issue !== null && issue !== undefined && !expanded[issue];
  };

  // The rows a thread shows: the expanded whole thread, else the inline newest rows.
  function rowsOf(thread: InboxThread): InboxNotification[] {
    const issue = thread.head.issue_id;
    if (issue !== null && issue !== undefined && expanded[issue]) return expanded[issue];
    const inline = thread.latest.length > 0 ? thread.latest : [thread.head];
    return inline.some((row) => row.id === thread.head.id) ? inline : [thread.head, ...inline];
  }

  async function load(before?: number): Promise<void> {
    const current = ++version;
    loading = true;
    failed = false;
    try {
      const page = await listInbox({
        folder,
        unread: unreadOnly ? 'only' : undefined,
        before,
      });
      if (current !== version) return;
      if (before === undefined) {
        threads = page.threads;
        expanded = {};
      } else {
        // A page may repeat a thread that changed its head between the pages; keep the first.
        const known = new Set(threads.map((thread) => thread.head.issue_id ?? -thread.head.id));
        threads = [
          ...threads,
          ...page.threads.filter((thread) => !known.has(thread.head.issue_id ?? -thread.head.id)),
        ];
      }
      next = page.next ?? null;
    } catch (error) {
      if (current === version) failed = true;
      showError(error);
      return;
    } finally {
      if (current === version) loading = false;
    }
  }

  // The page follows the folder, the unread filter and live arrivals; the first run loads it.
  $effect(() => {
    void load();
  });
  $effect(() => {
    return onInboxRows(() => {
      void load();
    });
  });

  async function expand(thread: InboxThread): Promise<void> {
    const issue = thread.head.issue_id;
    if (issue === null || issue === undefined) return;
    try {
      const page = await listInbox({ issue });
      expanded = { ...expanded, [issue]: page.threads.map((one) => one.head) };
    } catch (error) {
      showError(error);
    }
  }

  async function toggleRead(row: InboxNotification): Promise<void> {
    const toUnread = row.read;
    row.read = !row.read;
    try {
      await markInboxRead({ ids: [row.id], unread: toUnread });
      // A row turned read leaves the unread-only view.
      if (unreadOnly && !toUnread) void load();
    } catch (error) {
      showError(error);
      void load();
    }
  }

  async function readThread(thread: InboxThread): Promise<void> {
    const issue = thread.head.issue_id;
    if (issue === null || issue === undefined) return;
    for (const row of rowsOf(thread)) row.read = true;
    try {
      await markInboxRead({ issue });
      if (unreadOnly) void load();
    } catch (error) {
      showError(error);
      void load();
    }
  }

  async function readAll(): Promise<void> {
    for (const thread of threads) for (const row of rowsOf(thread)) row.read = true;
    try {
      await markInboxRead({ all: true });
      if (unreadOnly) void load();
    } catch (error) {
      showError(error);
      void load();
    }
  }

  const threadLabel = (thread: InboxThread): string => {
    const content = thread.head.content;
    return 'excerpt' in content ? content.excerpt : content.job;
  };
</script>

<div class="inbox">
  <div class="tabs" role="group" aria-label={m.bell()}>
    {#each tabs as [value, label] (value)}
      <button
        type="button"
        class="tab"
        aria-pressed={folder === value}
        onclick={() => {
          folder = value;
        }}
      >
        {label()}
      </button>
    {/each}
  </div>
  <div class="tools">
    <Checkbox label={m.unreadOnly()} bind:checked={unreadOnly} />
    {#if threads.some((thread) => rowsOf(thread).some((row) => !row.read))}
      <IconButton
        class="read-all"
        icon={CheckCheck}
        label={m.markAllRead()}
        size="sm"
        onclick={() => {
          void readAll();
        }}
      />
    {/if}
  </div>

  <div class="list">
    {#if failed}
      <EmptyState title={m.inboxFailed()}>
        {#snippet actions()}
          <Button
            variant="secondary"
            onclick={() => {
              void load();
            }}
          >
            {m.retry()}
          </Button>
        {/snippet}
      </EmptyState>
    {:else if loading && threads.length === 0}
      <div class="spinner"><Spinner /></div>
    {:else if threads.length === 0}
      <EmptyState title={unreadOnly ? m.inboxEmptyUnread() : m.inboxEmpty()} />
    {:else}
      {#each threads as thread (thread.head.issue_id ?? -thread.head.id)}
        <section class="thread" aria-label={threadLabel(thread)}>
          {#if thread.head.issue_id !== null && thread.head.issue_id !== undefined}
            <button
              type="button"
              class="thread-read"
              onclick={() => {
                void readThread(thread);
              }}
            >
              {m.markThreadRead()}
            </button>
          {/if}
          {#each rowsOf(thread) as row (row.id)}
            <NotificationRow {row} {onnavigate} ontoggleread={toggleRead} />
          {/each}
          {#if canExpand(thread) && rowsOf(thread).length >= 3}
            <button
              type="button"
              class="expand"
              onclick={() => {
                void expand(thread);
              }}
            >
              {m.showMore()}
            </button>
          {/if}
        </section>
      {/each}
      {#if next !== null}
        <button
          type="button"
          class="expand"
          onclick={() => {
            void load(next ?? undefined);
          }}
        >
          {m.showMore()}
        </button>
      {/if}
    {/if}
  </div>
</div>

<style>
  .inbox {
    display: flex;
    flex-direction: column;
    width: min(26rem, 100vw);
  }

  .tabs {
    display: flex;
    gap: var(--space-1);
    padding: var(--space-2) var(--space-3) 0;
  }

  .tab {
    padding: var(--space-1) var(--space-2);
    border: none;
    border-radius: var(--radius-md);
    background: none;
    color: var(--color-text-muted);
    font-size: var(--text-sm);
    font-weight: var(--weight-medium);
    cursor: pointer;
  }

  .tab:hover {
    background: var(--color-hover);
  }

  .tab[aria-pressed='true'] {
    background: var(--color-selected);
    color: var(--color-text);
  }

  .tools {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2);
    padding: var(--space-2) var(--space-3);
  }

  .list {
    display: flex;
    flex-direction: column;
    max-height: min(60vh, 30rem);
    overflow-y: auto;
    border-top: 1px solid var(--color-border);
  }

  .spinner {
    display: flex;
    justify-content: center;
    padding: var(--space-4);
  }

  .thread {
    display: flex;
    flex-direction: column;
  }

  .thread :global(.row + .row) {
    border-top: 1px solid var(--color-border);
  }

  .thread + .thread {
    border-top: var(--space-2) solid var(--color-surface-sunken);
  }

  .thread-read {
    align-self: flex-end;
    margin: var(--space-1) var(--space-2) 0;
    padding: var(--space-0_5) var(--space-2);
    border: none;
    border-radius: var(--radius-md);
    background: none;
    color: var(--color-text-muted);
    font-size: var(--text-xs);
    cursor: pointer;
  }

  .thread-read:hover {
    background: var(--color-hover);
    color: var(--color-text);
  }

  .expand {
    padding: var(--space-2);
    border: none;
    border-top: 1px solid var(--color-border);
    background: none;
    color: var(--color-link);
    font-size: var(--text-sm);
    cursor: pointer;
  }

  .expand:hover {
    background: var(--color-hover);
  }

  @media (width < 48rem) {
    /* A sheet on phones (D-019): the list takes the width and the safe area. */
    .inbox {
      width: 100%;
    }

    .list {
      max-height: min(70vh, 26rem);
      padding-bottom: var(--safe-bottom);
    }
  }
</style>
