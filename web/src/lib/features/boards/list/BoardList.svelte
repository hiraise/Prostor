<script lang="ts">
  import { brand } from '#lib/i18n/messages/brand.ts';
  // The boards list (BRD-001, BRD-002; a project's boards, PRJ-016): boards the person sees, their
  // favorites first, then by name; each row has the name, the owner, the projects and a visible
  // star, so nothing depends on hover (D-019). A search by name; «Показать ещё» loads the next page.
  import Plus from '@lucide/svelte/icons/plus';
  import SearchX from '@lucide/svelte/icons/search-x';
  import SquareKanban from '@lucide/svelte/icons/square-kanban';
  import Star from '@lucide/svelte/icons/star';
  import { untrack } from 'svelte';

  import type { BoardList, BoardSummary } from '#lib/api/schema.gen.ts';
  import { personName } from '#lib/features/access/index.ts';
  import type { ProjectChoice } from '#lib/features/projects/index.ts';
  import * as common from '#lib/i18n/messages/common.ts';
  import { getLocale } from '#lib/i18n/locale.svelte.ts';
  import { formatNumber } from '#lib/i18n/message.ts';
  import Button from '#lib/ui/Button.svelte';
  import EmptyState from '#lib/ui/EmptyState.svelte';
  import ErrorState from '#lib/ui/ErrorState.svelte';
  import IconButton from '#lib/ui/IconButton.svelte';
  import PageHeader from '#lib/ui/PageHeader.svelte';
  import TextField from '#lib/ui/TextField.svelte';
  import { describeError, showError, toasts } from '#lib/ui/toast.svelte.ts';

  import {
    boardHref,
    canCreateBoards,
    findBoards,
    orderBoards,
    setBoardFavorite,
  } from '../boards.ts';
  import * as m from '../messages.ts';
  import { openNewBoard } from './open.svelte.ts';

  interface Props {
    /** The first page, from the route's load. */
    first: BoardList;
    /** Only boards including this project (the project's «Доски» tab). */
    project?: ProjectChoice;
  }

  let { first, project }: Props = $props();

  /** Projects named on a row before «+N». */
  const PROJECTS_SHOWN = 3;
  const DEBOUNCE_MS = 250;

  let text = $state('');
  let term = $state('');
  let items = $state<BoardSummary[]>(untrack(() => first.items));
  let cursor = $state<string | null>(untrack(() => first.next_cursor));
  let searching = $state(false);
  let loadingMore = $state(false);
  let problem = $state<unknown>(null);
  let run = 0;

  const mayCreate = $derived(canCreateBoards());

  $effect(() => {
    const typed = text.trim();
    const timer = setTimeout(() => {
      if (typed !== untrack(() => term)) {
        term = typed;
        void load();
      }
    }, DEBOUNCE_MS);
    return () => {
      clearTimeout(timer);
    };
  });

  async function load(): Promise<void> {
    const mine = ++run;
    searching = true;
    problem = null;
    try {
      const page = await findBoards({ q: term, project: project?.id });
      if (mine !== run) return;
      items = page.items;
      cursor = page.next_cursor;
    } catch (error) {
      if (mine === run) problem = error;
    } finally {
      if (mine === run) searching = false;
    }
  }

  async function showMore(): Promise<void> {
    loadingMore = true;
    try {
      const page = await findBoards({ q: term, project: project?.id }, cursor);
      const known = new Set(items.map((board) => board.id));
      items = [...items, ...page.items.filter((board) => !known.has(board.id))];
      cursor = page.next_cursor;
    } catch (error) {
      showError(error, showMore);
    } finally {
      loadingMore = false;
    }
  }

  async function toggleStar(board: BoardSummary): Promise<void> {
    const favorite = !board.favorite;
    const flip = (value: boolean): void => {
      items = orderBoards(
        items.map((one) => (one.id === board.id ? { ...one, favorite: value } : one)),
      );
    };
    flip(favorite);
    try {
      await setBoardFavorite(board.id, favorite);
    } catch (error) {
      flip(!favorite);
      const { requestId } = describeError(error);
      toasts.show({
        tone: 'error',
        message: m.starFailed(),
        detail: requestId ? common.requestId({ id: requestId }) : undefined,
      });
    }
  }
</script>

<svelte:head><title>{m.boards()} · {brand()}</title></svelte:head>

<PageHeader title={m.boards()}>
  {#snippet actions()}
    {#if mayCreate}
      <Button variant="primary" icon={Plus} onclick={() => openNewBoard(project)}>
        {m.createBoard()}
      </Button>
    {/if}
  {/snippet}
</PageHeader>

<div class="page">
  <search class="tools">
    <TextField
      class="find"
      type="search"
      label={m.findBoards()}
      hint={m.findBoardsHint()}
      bind:value={text}
      autocomplete="off"
      spellcheck="false"
      maxlength={100}
    />
  </search>

  {#if problem}
    <ErrorState error={problem} level={2} onretry={load} />
  {:else if items.length === 0}
    {#if term}
      <EmptyState
        icon={SearchX}
        title={m.noBoardsFound()}
        description={m.noBoardsFoundText()}
        level={2}
      />
    {:else}
      <EmptyState
        icon={SquareKanban}
        title={project ? m.noProjectBoards() : m.noBoards()}
        description={mayCreate ? m.noBoardsCreate() : m.noBoardsShared()}
        level={2}
      >
        {#snippet actions()}
          {#if mayCreate}
            <Button variant="primary" icon={Plus} onclick={() => openNewBoard(project)}>
              {m.createBoard()}
            </Button>
          {/if}
        {/snippet}
      </EmptyState>
    {/if}
  {:else}
    <ul class="list" aria-label={m.boardList()} aria-busy={searching}>
      {#each items as board (board.id)}
        {@const hidden = board.projects.length - PROJECTS_SHOWN}
        <li class="row">
          <span class="text">
            <!-- Hrefs come from resolve(). -->
            <!-- eslint-disable-next-line svelte/no-navigation-without-resolve -->
            <a class="name" href={boardHref.board(board.id)}>{board.name}</a>
            <span class="meta">
              <span>{m.ownedBy({ name: personName(board.owner) })}</span>
              <span class="projects">
                {board.projects
                  .slice(0, PROJECTS_SHOWN)
                  .map((one) => one.name)
                  .join(', ')}
                {#if hidden > 0}
                  {m.moreProjects({ count: formatNumber(getLocale(), hidden) })}
                {/if}
              </span>
            </span>
          </span>
          <span class="actions">
            <IconButton
              icon={Star}
              label={board.favorite
                ? m.unstarBoard({ name: board.name })
                : m.starBoard({ name: board.name })}
              pressed={board.favorite}
              class={['star', { starred: board.favorite }]}
              onclick={() => toggleStar(board)}
            />
          </span>
        </li>
      {/each}
    </ul>
    {#if cursor}
      <div><Button loading={loadingMore} onclick={showMore}>{m.showMore()}</Button></div>
    {/if}
  {/if}
</div>

<style>
  .page {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
    padding: var(--space-2) var(--space-6) var(--space-12);
  }

  .tools :global(.find) {
    max-width: 24rem;
  }

  .list {
    margin: 0;
    padding: 0;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-lg);
    background: var(--color-surface);
    list-style: none;
  }

  .row {
    position: relative;
    display: flex;
    align-items: center;
    gap: var(--space-3);
    min-height: calc(var(--control-md) + var(--space-4));
    padding: var(--space-1) var(--space-2) var(--space-1) var(--space-4);
  }

  .row + .row {
    border-top: 1px solid var(--color-border);
  }

  .row:first-child {
    border-start-start-radius: var(--radius-lg);
    border-start-end-radius: var(--radius-lg);
  }

  .row:last-child {
    border-end-start-radius: var(--radius-lg);
    border-end-end-radius: var(--radius-lg);
  }

  .row:hover {
    background: var(--color-hover);
  }

  .text {
    display: flex;
    flex: 1;
    flex-wrap: wrap;
    align-items: center;
    gap: 0 var(--space-4);
    min-width: 0;
  }

  .name {
    display: inline-flex;
    flex: 1 1 14rem;
    align-items: center;
    min-width: var(--control-md);
    min-height: var(--control-md);
    color: var(--color-text);
    font-weight: var(--weight-medium);
    text-decoration: none;
    overflow-wrap: anywhere;
  }

  /* The name is the link; it covers the whole row, so the row is the target (D-019). */
  .name::after {
    content: '';
    position: absolute;
    inset: 0;
  }

  .row:hover .name {
    color: var(--color-link);
    text-decoration: underline;
  }

  .meta {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-0_5) var(--space-4);
    padding-bottom: var(--space-1);
    color: var(--color-text-muted);
    font-size: var(--text-sm);
  }

  .projects {
    overflow-wrap: anywhere;
  }

  /* Above the row's link, so the star stays its own target. */
  .actions {
    position: relative;
    z-index: 1;
    display: flex;
    flex: none;
    align-items: center;
  }

  .actions :global(.star.starred svg) {
    fill: var(--color-brand);
    stroke: var(--color-text);
  }

  @media (width < 48rem) {
    .page {
      padding: var(--space-2) var(--space-4) var(--space-8);
    }

    .tools :global(.find) {
      max-width: none;
    }

    .row {
      align-items: flex-start;
      padding-top: var(--space-2);
      padding-bottom: var(--space-2);
    }
  }
</style>
