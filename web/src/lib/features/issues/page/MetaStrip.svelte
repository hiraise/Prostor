<script lang="ts">
  // The line under the summary (ISS-016): who reported the issue and when, who changed it last,
  // whom it is visible to, its votes and how many people watch it. Votes and the star are toggles
  // that show at once (ISS-030, ISS-032); the watcher count opens the list (NTF-015). One's own
  // issue cannot be voted for, and the button says why.
  import Star from '@lucide/svelte/icons/star';
  import ThumbsUp from '@lucide/svelte/icons/thumbs-up';

  import type { Issue } from '#lib/api/schema.gen.ts';
  import { PersonName } from '#lib/features/access/index.ts';
  import { WatchersPopup } from '#lib/features/notifications/index.ts';
  import Time from '#lib/time/Time.svelte';

  import { currentUser } from '#lib/session/index.ts';
  import { showError } from '#lib/ui/toast.svelte.ts';
  import { tooltip } from '#lib/ui/tooltip.ts';

  import { setStar, setVote } from '../actions.ts';
  import AudienceMarker from '../audience/AudienceMarker.svelte';
  import * as m from '../messages.ts';

  let { issue }: { issue: Issue } = $props();

  const id = $props.id();
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

<div role="group" aria-label={m.details()}>
  <dl class="strip">
    <div>
      <dt>{m.reportedBy()}</dt>
      <dd>
        <PersonName person={issue.reporter} />
        <Time value={issue.created_at} kind="relative" />
      </dd>
    </div>
    {#if issue.updater}
      <div>
        <dt>{m.updatedBy()}</dt>
        <dd>
          <PersonName person={issue.updater} />
          <Time value={issue.updated_at} kind="relative" />
        </dd>
      </div>
    {/if}
    <div class="plain">
      <dt class="sr-only">{m.visibility()}</dt>
      <dd><AudienceMarker audience={issue.audience} /></dd>
    </div>
    <div class="plain">
      <dt class="sr-only">{m.votes({ count: issue.votes })}</dt>
      <dd>
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
        {#if !issue.can.vote}<span class="sr-only" id="{id}-vote-reason">{voteReason}</span>{/if}
      </dd>
    </div>
    <div class="plain">
      <dt class="sr-only">{m.star()}</dt>
      <dd>
        <button
          type="button"
          class={['toggle', { on: issue.starred }]}
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
      </dd>
    </div>
  </dl>
</div>

<style>
  .strip {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-1) var(--space-5);
    margin: 0;
    color: var(--color-text-muted);
    font-size: var(--text-sm);
  }

  .strip > div {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-1);
  }

  dd {
    display: inline-flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-1);
    margin: 0;
    color: var(--color-text);
  }

  .toggle {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
    min-height: 2rem;
    padding: 0 var(--space-2);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-full);
    background: transparent;
    color: var(--color-text-muted);
    font: inherit;
    cursor: pointer;
  }

  .toggle:hover:not([aria-disabled='true']) {
    background: var(--color-surface-sunken);
  }

  .toggle.on {
    border-color: var(--color-link);
    color: var(--color-link);
  }

  .toggle.on :global(svg) {
    fill: currentColor;
  }

  .toggle[aria-disabled='true'] {
    cursor: not-allowed;
    opacity: 0.6;
  }

  @media (width < 48rem) {
    .toggle {
      min-width: 2.75rem;
      min-height: 2.75rem;
      justify-content: center;
    }
  }
</style>
