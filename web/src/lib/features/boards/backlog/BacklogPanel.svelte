<script lang="ts">
  // The backlog beside the board (BRD-089…BRD-096): its saved search's issues with the header
  // filter applied, ordered by hand first (BRD-093). Rows drag to reorder (those who edit the
  // search) and onto a cell (BRD-096); a card dropped here leaves the board (BRD-074). «В спринт»
  // on a row adds it without dragging; «Новая задача» opens the new-issue form. On phones the
  // page shows it as a sheet. Live like the issue list (BRD-116): rows in place, «Обновить».
  import ArrowRight from '@lucide/svelte/icons/arrow-right';
  import Ellipsis from '@lucide/svelte/icons/ellipsis';
  import Plus from '@lucide/svelte/icons/plus';

  import type { Board, Issue } from '#lib/api/schema.gen.ts';
  import { IssueRef, listHref, openNewIssue } from '#lib/features/issues/index.ts';
  import * as common from '#lib/i18n/messages/common.ts';
  import { liveConnection, type LiveSource } from '#lib/store/index.ts';
  import Button from '#lib/ui/Button.svelte';
  import IconButton from '#lib/ui/IconButton.svelte';
  import Menu, { type MenuEntry } from '#lib/ui/Menu.svelte';
  import Notice from '#lib/ui/Notice.svelte';
  import Spinner from '#lib/ui/Spinner.svelte';
  import { showError, toasts } from '#lib/ui/toast.svelte.ts';

  import type { BoardDrag } from '../drag/drag.svelte.ts';
  import * as m from '../messages.ts';
  import type { BacklogState } from './backlog.svelte.ts';

  interface Props {
    board: Board;
    backlog: BacklogState;
    drag: BoardDrag;
    /** The person may put this issue on the shown sprint (or the board). */
    canAdd: (issue: Issue) => boolean;
    onadd: (issue: Issue) => unknown;
    /** Where live changes come from; the app's connection by default. */
    live?: LiveSource;
  }

  let { board, backlog, drag, canAdd, onadd, live }: Props = $props();

  const meta = $derived(backlog.meta);
  const list = $derived(backlog.list);
  const search = $derived(meta?.search ?? null);
  const rows = $derived(list?.items ?? []);
  const label = $derived(board.sprints_on ? m.toSprintOf : m.toBoardOf);

  $effect(() => {
    const topic = meta?.liveTopic;
    const shown = list;
    if (!topic || !shown) return;
    return shown.follow(live ?? liveConnection(), topic);
  });

  let refreshing = $state(false);
  async function refresh(): Promise<void> {
    refreshing = true;
    try {
      await backlog.reload();
    } finally {
      refreshing = false;
    }
  }

  async function discard(): Promise<void> {
    try {
      await backlog.discard();
      toasts.show({ tone: 'success', message: m.orderReset() });
    } catch (error) {
      showError(new Error(m.orderFailed(), { cause: error }));
    }
  }

  const actions = $derived<MenuEntry[]>([
    ...(search?.may_order
      ? [{ type: 'action' as const, label: m.resetOrder(), onSelect: () => void discard() }]
      : []),
  ]);
</script>

<div class="backlog" {@attach drag.panel}>
  <div class="head">
    <h2>{m.backlog()}</h2>
    {#if search}<span class="name">{search.name}</span>{/if}
  </div>

  {#if backlog.error}
    <Notice tone="danger">
      {m.backlogFailed()}
      <Button variant="secondary" size="sm" onclick={() => void backlog.reload()}
        >{common.retry()}</Button
      >
    </Notice>
  {:else if !list || (list.refreshing && meta === undefined)}
    <div class="loading"><Spinner label={common.loading()} /></div>
  {:else if meta?.missing === 'hidden'}
    <p class="note">{m.backlogHidden()}</p>
  {:else if meta?.missing === 'none'}
    <p class="note">
      {m.backlogNone()}
      {#if board.rights.edit}{m.backlogNoneEditor()}{/if}
    </p>
  {:else}
    {#if list.stale}
      <Notice tone="info">
        {m.backlogChanged()}
        <Button variant="secondary" size="sm" loading={refreshing} onclick={() => void refresh()}
          >{m.refresh()}</Button
        >
      </Notice>
    {/if}
    <ul class="rows" aria-label={m.backlogRows()} aria-busy={list.refreshing} data-drop-backlog>
      {#each rows as issue (issue.id)}
        <li class="row" data-card={issue.id}>
          <span class="ref"><IssueRef {issue} /></span>
          {#if canAdd(issue)}
            <IconButton
              icon={ArrowRight}
              size="sm"
              label={label({ id: issue.readable_id ?? '' })}
              onclick={() => onadd(issue)}
            />
          {/if}
        </li>
      {:else}
        <li class="empty">{m.backlogEmpty()}</li>
      {/each}
    </ul>
    {#if list.hasMore}
      <Button variant="ghost" size="sm" loading={list.loadingMore} onclick={() => list.loadMore()}
        >{m.showMore()}</Button
      >
    {/if}
  {/if}

  <div class="foot">
    <Button
      size="sm"
      variant="ghost"
      icon={Plus}
      onclick={() => {
        openNewIssue({ project: board.projects[0]?.project.key });
      }}>{m.newIssue()}</Button
    >
    {#if search}
      <!-- eslint-disable-next-line svelte/no-navigation-without-resolve -- listHref resolves -->
      <a class="link" href={listHref({ q: '', context: { kind: 'search', id: search.id } })}
        >{m.openBacklogList()}</a
      >
    {/if}
    {#if actions.length > 0}
      <Menu label={m.backlogActions()} items={actions} placement={{ side: 'top', align: 'end' }}>
        {#snippet trigger(props)}
          <IconButton icon={Ellipsis} size="sm" label={m.backlogActions()} {...props} />
        {/snippet}
      </Menu>
    {/if}
  </div>
</div>

<style>
  .backlog {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    min-height: 0;
  }

  .head {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
  }

  h2 {
    font-size: var(--text-md);
  }

  .name,
  .note {
    color: var(--color-text-muted);
    font-size: var(--text-sm);
  }

  .rows {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    min-height: 4rem;
    margin: 0;
    padding: 0;
    overflow-y: auto;
    list-style: none;
  }

  .row {
    display: flex;
    align-items: flex-start;
    gap: var(--space-2);
    padding: var(--space-2) var(--space-3);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    background: var(--color-surface);
    cursor: grab;
    user-select: none;
  }

  .ref {
    flex: 1;
    min-width: 0;
    overflow-wrap: anywhere;
  }

  .empty {
    padding: var(--space-4) 0;
    color: var(--color-text-muted);
    text-align: center;
  }

  .loading {
    display: flex;
    justify-content: center;
    padding: var(--space-6);
  }

  .foot {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-2);
  }

  .link {
    color: var(--color-link);
    font-size: var(--text-sm);
  }
</style>
