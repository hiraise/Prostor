<script lang="ts">
  // The star of a list row (QRY-076, ISS-030): a toggle that shows at once and rolls back with a
  // toast when the server refuses.
  import Star from '@lucide/svelte/icons/star';

  import type { Issue } from '#lib/api/schema.gen.ts';
  import IconButton from '#lib/ui/IconButton.svelte';
  import { showError } from '#lib/ui/toast.svelte.ts';

  import { setStar } from '../actions.ts';
  import * as m from './messages.ts';

  let { issue, size = 'sm' }: { issue: Issue; size?: 'sm' | 'md' } = $props();
</script>

<IconButton
  class={['row-star', { on: issue.starred }]}
  icon={Star}
  {size}
  label={m.starIssue({ id: issue.readable_id ?? '' })}
  pressed={issue.starred}
  onclick={() => {
    setStar(issue, !issue.starred).catch((error: unknown) => {
      showError(error);
    });
  }}
/>

<style>
  :global(.row-star.on) {
    color: var(--color-warning);
  }

  :global(.row-star.on svg) {
    fill: currentColor;
  }
</style>
