<script lang="ts">
  // The board switcher (BRD-001, BRD-002): the board's name is a button that opens the boards the
  // person sees, favorites first, with a filter, the owner and a star on each; «Все доски» opens
  // the list. On phones the popover is a bottom sheet.
  import ChevronDown from '@lucide/svelte/icons/chevron-down';
  import Star from '@lucide/svelte/icons/star';

  import type { BoardSummary } from '#lib/api/schema.gen.ts';
  import { personName } from '#lib/features/access/index.ts';
  import * as common from '#lib/i18n/messages/common.ts';
  import IconButton from '#lib/ui/IconButton.svelte';
  import Popover from '#lib/ui/Popover.svelte';
  import Spinner from '#lib/ui/Spinner.svelte';
  import TextField from '#lib/ui/TextField.svelte';
  import { describeError, toasts } from '#lib/ui/toast.svelte.ts';

  import { boardHref, findBoards, orderBoards, setBoardFavorite } from '../boards.ts';
  import * as m from '../messages.ts';

  interface Props {
    /** The board on screen. */
    board: { id: number; name: string };
  }

  let { board }: Props = $props();

  let open = $state<boolean>(false);
  let text = $state('');
  let items = $state<BoardSummary[] | null>(null);
  let problem = $state<string | null>(null);
  let run = 0;

  // The boards load when it opens and follow the filter a moment after typing.
  $effect(() => {
    if (!open) return;
    const typed = text.trim();
    const controller = new AbortController();
    const mine = ++run;
    const timer = setTimeout(
      () => {
        findBoards({ q: typed }, null, controller.signal).then(
          (page) => {
            if (mine !== run) return;
            items = page.items;
            problem = null;
          },
          (error: unknown) => {
            if (mine === run && !controller.signal.aborted) problem = describeError(error).message;
          },
        );
      },
      typed ? 250 : 0,
    );
    return () => {
      clearTimeout(timer);
      controller.abort();
    };
  });

  async function toggleStar(one: BoardSummary): Promise<void> {
    const favorite = !one.favorite;
    const flip = (value: boolean): void => {
      items = orderBoards(
        (items ?? []).map((other) => (other.id === one.id ? { ...other, favorite: value } : other)),
      );
    };
    flip(favorite);
    try {
      await setBoardFavorite(one.id, favorite);
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

<Popover label={m.boards()} bind:open placement={{ side: 'bottom', align: 'start' }}>
  {#snippet trigger(props)}
    <button
      type="button"
      class="trigger"
      aria-label={m.switchBoard({ name: board.name })}
      {...props}
    >
      <span class="title">{board.name}</span>
      <ChevronDown aria-hidden="true" />
    </button>
  {/snippet}
  {#snippet children({ close })}
    <div class="panel">
      <TextField
        label={m.filterBoards()}
        type="search"
        bind:value={text}
        autocomplete="off"
        spellcheck="false"
        maxlength={100}
      />
      {#if problem}
        <p class="note danger">{problem}</p>
      {:else if items === null}
        <p class="note"><Spinner label={common.loading()} /></p>
      {:else if items.length === 0}
        <p class="note">{m.noBoardsFound()}</p>
      {:else}
        <ul class="list" aria-label={m.boardList()}>
          {#each items as one (one.id)}
            <li class="row">
              <!-- Hrefs come from resolve(). -->
              <!-- eslint-disable-next-line svelte/no-navigation-without-resolve -->
              <a
                class="name"
                href={boardHref.board(one.id)}
                aria-current={one.id === board.id ? 'page' : undefined}
                onclick={close}
              >
                <span>{one.name}</span>
                <span class="owner">{personName(one.owner)}</span>
              </a>
              <IconButton
                icon={Star}
                label={one.favorite
                  ? m.unstarBoard({ name: one.name })
                  : m.starBoard({ name: one.name })}
                pressed={one.favorite}
                class={['star', { starred: one.favorite }]}
                onclick={() => toggleStar(one)}
              />
            </li>
          {/each}
        </ul>
      {/if}
      <!-- eslint-disable-next-line svelte/no-navigation-without-resolve -->
      <a class="all" href={boardHref.list()} onclick={close}>{m.allBoards()}</a>
    </div>
  {/snippet}
</Popover>

<style>
  .trigger {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
    max-width: 100%;
    min-height: var(--control-md);
    padding: 0 var(--space-2);
    margin-left: calc(-1 * var(--space-2));
    border: none;
    border-radius: var(--radius-md);
    background: none;
    color: var(--color-text);
    font: inherit;
    cursor: pointer;
  }

  .trigger:hover {
    background: var(--color-hover);
  }

  .trigger :global(svg) {
    flex: none;
    width: var(--icon-md);
    height: var(--icon-md);
  }

  .title {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .panel {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    width: min(24rem, 100%);
    padding: var(--space-2);
  }

  .list {
    max-height: 22rem;
    margin: 0;
    padding: 0;
    overflow-y: auto;
    list-style: none;
  }

  .row {
    display: flex;
    align-items: center;
    gap: var(--space-1);
  }

  .name {
    display: flex;
    flex: 1;
    flex-direction: column;
    justify-content: center;
    min-width: 0;
    min-height: var(--control-md);
    padding: var(--space-1) var(--space-2);
    border-radius: var(--radius-md);
    color: var(--color-text);
    text-decoration: none;
    overflow-wrap: anywhere;
  }

  .name:hover {
    background: var(--color-hover);
  }

  .name[aria-current='page'] {
    font-weight: var(--weight-semibold);
  }

  .owner {
    color: var(--color-text-muted);
    font-size: var(--text-sm);
  }

  .row :global(.star.starred svg) {
    fill: var(--color-brand);
    stroke: var(--color-text);
  }

  .note {
    padding: var(--space-2);
    color: var(--color-text-muted);
    font-size: var(--text-sm);
  }

  .note.danger {
    color: var(--color-danger);
  }

  .all {
    display: inline-flex;
    align-items: center;
    min-height: var(--control-md);
    padding: 0 var(--space-2);
    border-top: 1px solid var(--color-border);
    color: var(--color-link);
  }
</style>
