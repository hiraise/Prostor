<script lang="ts">
  // The watcher popup (NTF-015, NTF-016, plan step 1.10 §4.3): the counter next to the star on
  // the issue page opens who watches the issue. Those with `issue.watchers.update` add people —
  // only ones who can read the issue, as the mention list offers — and take anyone off; everyone
  // may take their own marker off (the star does the same). Every change replaces the whole list
  // in one request; the count on the row follows the store.
  import type { Issue } from '#lib/api/schema.gen.ts';
  import { watchers as watchersLabel } from '#lib/features/issues/messages.ts';

  import Popover from '#lib/ui/Popover.svelte';

  import * as m from './messages.ts';
  import WatchersList from './WatchersList.svelte';

  let { issue }: { issue: Issue } = $props();
</script>

{#if issue.watchers !== null}
  <!-- A fixed title, not the count: the phone sheet mounts its title even closed, and a
       counting title would name a second element on the page. -->
  <Popover label={m.watchersTitle()}>
    {#snippet trigger(props)}
      <button type="button" class="count" {...props}>
        <span aria-hidden="true">{issue.watchers ?? 0}</span>
        <span class="sr-only">{watchersLabel({ count: issue.watchers ?? 0 })}</span>
      </button>
    {/snippet}
    <WatchersList {issue} />
  </Popover>
{/if}

<style>
  .count {
    min-width: 2rem;
    min-height: 2rem;
    padding: 0 var(--space-1);
    border: none;
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--color-text-muted);
    font: inherit;
    cursor: pointer;
  }

  .count:hover {
    background: var(--color-hover);
    color: var(--color-text);
  }

  @media (pointer: coarse) {
    .count {
      min-width: 2.75rem;
      min-height: 2.75rem;
      justify-content: center;
    }
  }
</style>
