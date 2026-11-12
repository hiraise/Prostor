<script lang="ts">
  // The boards of an issue (ISS-020, BRD-033): each board with the sprint holding the issue, both
  // links; people who may change the issue add it to a board and sprint or take it off. On a draft
  // (ISS-007) it also lists the boards the issue joins on its own when created, marked
  // «автоматически»; taking it off one keeps it off, «Вернуть» lets it back on. Without `boards`
  // the list loads itself, again when the project changes (a draft's project picker).
  import Plus from '@lucide/svelte/icons/plus';
  import Undo2 from '@lucide/svelte/icons/undo-2';
  import X from '@lucide/svelte/icons/x';

  import type { IssueBoard } from '#lib/api/schema.gen.ts';
  import { refusalText } from '#lib/features/fields/index.ts';
  import { announce } from '#lib/ui/announce.svelte.ts';
  import Button from '#lib/ui/Button.svelte';
  import IconButton from '#lib/ui/IconButton.svelte';
  import { toasts } from '#lib/ui/toast.svelte.ts';
  import { tooltip } from '#lib/ui/tooltip.ts';

  import { addIssueBoard, boardHref, issueBoards, removeIssueBoard } from '../boards.ts';
  import * as m from '../messages.ts';
  import AddToBoardDialog from './AddToBoardDialog.svelte';

  interface Props {
    issue: number;
    project: number;
    /** May add and remove (the issue's update right; boards check their own). */
    editable: boolean;
    /** The list from the issue page's answer; absent: loaded here. */
    boards?: IssueBoard[];
  }

  let { issue, project, editable, boards: given }: Props = $props();

  const id = $props.id();
  let loaded = $state<IssueBoard[] | null>(null);
  let problem = $state<string | null>(null);
  let adding = $state(false);
  let busy = $state<number | null>(null);

  const rows = $derived(loaded ?? given ?? []);
  const holding = $derived(rows.map((row) => row.board.id));

  // A new issue or project: the given list, else a fresh load.
  $effect(() => {
    const key = [issue, project, given] as const;
    loaded = null;
    problem = null;
    if (key[2]) return;
    const controller = new AbortController();
    issueBoards(issue, controller.signal).then(
      (list) => (loaded = list),
      (error: unknown) => {
        if (!controller.signal.aborted) problem = m.issueBoardsFailed() + ': ' + refusalText(error);
      },
    );
    return () => controller.abort();
  });

  async function remove(row: IssueBoard): Promise<void> {
    if (busy !== null) return;
    busy = row.board.id;
    try {
      loaded = await removeIssueBoard(issue, row.board.id);
      announce(m.removedFromBoard({ name: row.board.name }));
    } catch (error) {
      toasts.show({ tone: 'error', message: `${m.removeBoardFailed()}: ${refusalText(error)}` });
    } finally {
      busy = null;
    }
  }

  async function putBack(row: IssueBoard): Promise<void> {
    if (busy !== null) return;
    busy = row.board.id;
    try {
      loaded = await addIssueBoard(issue, row.board.id);
      announce(m.addedToBoard());
    } catch (error) {
      toasts.show({ tone: 'error', message: `${m.addFailed()}: ${refusalText(error)}` });
    } finally {
      busy = null;
    }
  }

  function added(list: IssueBoard[]): void {
    loaded = list;
    announce(m.addedToBoard());
  }
</script>

<section class="boards" aria-labelledby="{id}-title">
  <div class="head">
    <h3 id="{id}-title">{m.issueBoards()}</h3>
    {#if editable}
      <IconButton icon={Plus} size="sm" label={m.addToBoard()} onclick={() => (adding = true)} />
    {/if}
  </div>
  {#if problem}
    <p class="problem">{problem}</p>
  {:else if rows.length === 0}
    <p class="empty">{m.noIssueBoards()}</p>
  {:else}
    <ul>
      {#each rows as row (row.board.id)}
        <li class={{ skipped: row.skipped }}>
          <!-- Hrefs come from boardHref, which resolves them. -->
          <!-- eslint-disable svelte/no-navigation-without-resolve -->
          <span class="names">
            <a href={boardHref.board(row.board.id)}>{row.board.name}</a>
            {#if row.sprint && row.sprints_on}
              <span aria-hidden="true">·</span>
              <a href={boardHref.sprint(row.board.id, row.sprint.id)}>{row.sprint.name}</a>
            {/if}
            {#if row.skipped}
              <span class="mark">{m.skippedBoard()}</span>
            {:else if row.auto}
              <span class="mark" {@attach tooltip(m.autoBoardHint())}>{m.autoBoard()}</span>
            {/if}
          </span>
          <!-- eslint-enable svelte/no-navigation-without-resolve -->
          {#if editable}
            {#if row.skipped}
              <Button
                variant="ghost"
                size="sm"
                icon={Undo2}
                aria-label={m.putBack({ name: row.board.name })}
                disabled={busy !== null}
                onclick={() => void putBack(row)}>{m.putBackShort()}</Button
              >
            {:else}
              <IconButton
                icon={X}
                size="sm"
                label={m.removeFromBoardOf({ name: row.board.name })}
                disabled={busy !== null}
                onclick={() => void remove(row)}
              />
            {/if}
          {/if}
        </li>
      {/each}
    </ul>
  {/if}
</section>

{#if editable}
  <AddToBoardDialog bind:open={adding} {issue} {project} {holding} onadded={added} />
{/if}

<style>
  .boards {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    min-width: 0;
  }

  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2);
    min-height: var(--control-md);
  }

  h3 {
    color: var(--color-text-muted);
    font-size: var(--text-sm);
    font-weight: var(--weight-regular);
  }

  ul {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    margin: 0;
    padding: 0;
    list-style: none;
  }

  li {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2);
    min-height: var(--control-md);
  }

  .names {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: var(--space-1);
    min-width: 0;
    overflow-wrap: anywhere;
  }

  a {
    color: var(--color-link);
  }

  .skipped a {
    color: var(--color-text-muted);
    text-decoration: line-through;
  }

  .mark {
    color: var(--color-text-muted);
    font-size: var(--text-sm);
  }

  .empty {
    color: var(--color-text-muted);
  }

  .problem {
    color: var(--color-danger);
    font-size: var(--text-sm);
  }
</style>
