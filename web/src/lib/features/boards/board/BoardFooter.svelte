<script lang="ts">
  // The board's footer (BRD-084): the owner with a link to their profile, the goal of the shown
  // sprint, for an archived sprint its restore and delete (BRD-103, BRD-104), and on a board of
  // parent lanes the new-lane button (BRD-054).
  import Plus from '@lucide/svelte/icons/plus';

  import { resolve } from '$app/paths';

  import type { Board, Sprint } from '#lib/api/schema.gen.ts';
  import { PersonName } from '#lib/features/access/index.ts';
  import { canSeeDirectory } from '#lib/session/index.ts';
  import Button from '#lib/ui/Button.svelte';
  import Notice from '#lib/ui/Notice.svelte';

  import * as m from '../messages.ts';

  interface Props {
    board: Board;
    sprint: Sprint | null;
    onrestore: () => unknown;
    ondelete: () => unknown;
    /** Makes a parent lane; absent when the person may not. */
    onnewlane?: () => unknown;
  }

  let { board, sprint, onrestore, ondelete, onnewlane }: Props = $props();

  const ownerHref = $derived(
    canSeeDirectory() && board.owner.kind === 'person'
      ? resolve('/(app)/users/[id]', { id: String(board.owner.id) })
      : undefined,
  );
  const shown = $derived(sprint !== null && board.sprints_on && !sprint.internal ? sprint : null);
</script>

<footer class="footer" aria-label={m.boardFooter()}>
  {#if shown?.archived}
    <Notice tone="warning">
      <span class="archived">
        <span>{m.archivedSprintNote()}</span>
        {#if board.rights.edit}
          <Button size="sm" variant="secondary" onclick={() => onrestore()}
            >{m.restoreSprint()}</Button
          >
          <Button size="sm" variant="ghost" onclick={() => ondelete()}>{m.deleteSprint()}</Button>
        {/if}
      </span>
    </Notice>
  {/if}
  <dl class="facts">
    {#if shown && shown.goal.trim() !== ''}
      <div class="fact goal">
        <dt>{m.goalLabel()}</dt>
        <dd>{shown.goal}</dd>
      </div>
    {/if}
    <div class="fact">
      <dt>{m.ownerLabel()}</dt>
      <dd><PersonName person={board.owner} href={ownerHref} /></dd>
    </div>
    {#if onnewlane}
      <div class="fact lane">
        <Button size="sm" variant="ghost" icon={Plus} onclick={() => onnewlane()}
          >{m.newLane()}</Button
        >
      </div>
    {/if}
  </dl>
</footer>

<style>
  .footer {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    margin-top: auto;
    padding: var(--space-3) var(--space-6);
    border-top: 1px solid var(--color-border);
    font-size: var(--text-sm);
  }

  .archived {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-2);
  }

  .facts {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2) var(--space-6);
    margin: 0;
  }

  .fact {
    display: flex;
    gap: var(--space-2);
    min-width: 0;
  }

  .lane {
    margin-inline-start: auto;
  }

  .goal {
    flex: 1 1 20rem;
  }

  dt {
    flex: none;
    color: var(--color-text-muted);
  }

  dd {
    min-width: 0;
    margin: 0;
    overflow-wrap: anywhere;
    white-space: pre-line;
  }

  @media (width < 48rem) {
    .footer {
      padding: var(--space-3) var(--space-4);
    }
  }
</style>
