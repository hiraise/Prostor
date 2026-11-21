<script lang="ts">
  // The top line of the issue (ISS-016): the ID with a copy button (UI-006) at its start, the
  // reporter and the last updater inline beside it — one line on a wide screen — and at the line's
  // end the person's own marks: votes (ISS-030) and the star with the watcher count (ISS-032,
  // NTF-015); in the board popup the corner controls the page passes in (`corner`: «Открыть
  // страницей», the close button) join that end. On the full page the visibility lock sits beside
  // «Изменить»; in the popup its control joins this corner.
  // Votes and the star are toggles that show at once; one's own issue cannot be voted for, and the
  // button says why.
  import Star from '@lucide/svelte/icons/star';
  import ThumbsUp from '@lucide/svelte/icons/thumbs-up';
  import Copy from '@lucide/svelte/icons/copy';
  import type { Snippet } from 'svelte';

  import type { Issue } from '#lib/api/schema.gen.ts';
  import { PersonName } from '#lib/features/access/index.ts';
  import { WatchersPopup } from '#lib/features/notifications/index.ts';
  import Time from '#lib/time/Time.svelte';

  import { currentUser } from '#lib/session/index.ts';
  import { copyText } from '#lib/ui/copy.ts';
  import IconButton from '#lib/ui/IconButton.svelte';
  import { showError } from '#lib/ui/toast.svelte.ts';
  import { tooltip } from '#lib/ui/tooltip.ts';

  import { setStar, setVote } from '../actions.ts';
  import * as m from '../messages.ts';

  let {
    issue,
    corner,
    embedded = false,
  }: { issue: Issue; corner?: Snippet; embedded?: boolean } = $props();

  const id = $props.id();
  const readable = $derived(issue.readable_id);
  const voteReason = $derived(
    currentUser()?.id === issue.reporter.id ? m.ownIssueVote() : m.cannotVote(),
  );

  /** A refused toggle has rolled back already; the toast says why. */
  function run(change: Promise<void>): void {
    change.catch((error: unknown) => {
      showError(error);
    });
  }
</script>

<div class={['strip', { embedded }]} role="group" aria-label={m.details()}>
  {#if readable}
    <p class="id">
      <span>{readable}</span>
      <IconButton
        class="copy"
        icon={Copy}
        label={m.copyId()}
        onclick={() => void copyText(readable, m.idCopied({ id: readable }))}
      />
    </p>
  {:else}
    <p class="id"><span class="draft">{m.draft()}</span></p>
  {/if}
  <dl class="facts">
    <div>
      <dt>{m.reportedBy()}</dt>
      <dd>
        <PersonName person={issue.reporter} />
        <Time value={issue.created_at} kind="relative" />
      </dd>
    </div>
    {#if issue.updater}
      <div class="updated">
        <dt>{m.updatedBy()}</dt>
        <dd>
          <PersonName person={issue.updater} />
          <Time value={issue.updated_at} kind="relative" />
        </dd>
      </div>
    {/if}
  </dl>
  <div class="tail">
    <button
      type="button"
      class={['toggle', { on: issue.voted }]}
      aria-pressed={issue.voted}
      aria-disabled={!issue.can.vote}
      aria-describedby={issue.can.vote ? undefined : `${id}-vote-reason`}
      {@attach tooltip(issue.can.vote ? m.vote() : voteReason)}
      onclick={() => {
        if (issue.can.vote) run(setVote(issue, !issue.voted));
      }}
    >
      <ThumbsUp size={16} aria-hidden="true" />
      <span class="sr-only">{m.vote()}</span>
      <span aria-hidden="true">{issue.votes}</span>
    </button>
    <!-- The count is aria-hidden on the button; said in words beside it. -->
    <span class="sr-only">{m.votes({ count: issue.votes })}</span>
    <button
      type="button"
      class={['toggle star', { on: issue.starred }]}
      aria-pressed={issue.starred}
      {@attach tooltip(m.star())}
      onclick={() => {
        run(setStar(issue, !issue.starred));
      }}
    >
      <Star size={16} aria-hidden="true" />
      <span class="sr-only">{m.star()}</span>
    </button>
    <WatchersPopup {issue} />
    {#if !issue.can.vote}<span class="sr-only" id="{id}-vote-reason">{voteReason}</span>{/if}
    {@render corner?.()}
  </div>
</div>

<style>
  .strip {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    row-gap: var(--space-1);
    column-gap: var(--space-1) var(--space-4);
    color: var(--color-text-muted);
    font-size: var(--text-sm);
  }

  .id {
    display: flex;
    align-items: center;
    gap: var(--space-0_5);
    margin: 0;
    font-family: var(--font-mono);
  }

  .draft {
    color: var(--color-text-faint);
  }

  .facts {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-1) var(--space-4);
    margin: 0;
  }

  .facts > div {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-1);
  }

  .strip.embedded .facts {
    flex-wrap: nowrap;
  }

  .strip.embedded .facts > div {
    flex-wrap: nowrap;
    gap: var(--space-1_5);
  }

  .strip.embedded .tail {
    align-self: center;
  }

  dd {
    display: inline-flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-1);
    margin: 0;
    color: var(--color-text);
  }

  /* The person's own marks and the popup's corner stay together at the end. */
  .tail {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-1) var(--space-2);
    margin-inline-start: auto;
  }

  .toggle {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: var(--space-1);
    min-width: 2rem;
    min-height: 2rem;
    padding: 0 var(--space-1);
    border: 0;
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--color-text-muted);
    font: inherit;
    cursor: pointer;
  }

  .toggle:hover:not([aria-disabled='true']) {
    background: var(--color-hover);
    color: var(--color-text);
  }

  .toggle.on {
    color: var(--color-link);
  }

  .toggle.on :global(svg) {
    fill: currentColor;
  }

  .toggle[aria-disabled='true'] {
    cursor: not-allowed;
    opacity: 0.6;
  }

  /* The watcher count keeps the same height (WatchersPopup styles its own button). */
  .tail :global(.count) {
    min-width: 2rem;
    min-height: 2rem;
  }

  @media (width < 48rem) {
    .strip.embedded .facts .updated {
      display: none;
    }
  }

  @media (pointer: coarse) {
    .toggle,
    .tail :global(.count) {
      min-width: 2.75rem;
      min-height: 2.75rem;
    }
  }
</style>
