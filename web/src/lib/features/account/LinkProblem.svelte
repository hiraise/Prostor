<script lang="ts">
  // What a page opened from an emailed link says when the link no longer works: used or cut
  // short, or expired, with the way to a new one.
  import { resolve } from '$app/paths';

  import Button from '#lib/ui/Button.svelte';
  import Notice from '#lib/ui/Notice.svelte';

  import * as m from './messages.ts';

  interface Props {
    problem: 'invalid' | 'expired';
    /** What to do about an expired link. */
    expiredText: string;
    /** Where a new link can be asked for, if the person can do it themselves. */
    renewHref?: string;
  }

  let { problem, expiredText, renewHref }: Props = $props();
</script>

<div class="problem">
  <Notice tone="warning">
    <strong>{problem === 'expired' ? m.linkExpiredTitle() : m.linkInvalidTitle()}.</strong>
    {problem === 'expired' ? expiredText : m.linkInvalid()}
  </Notice>
  <div class="actions">
    {#if renewHref}
      <Button href={renewHref} variant="primary">{m.askNewLink()}</Button>
    {/if}
    <Button href={resolve('/(public)/signin')}>{m.backToSignIn()}</Button>
  </div>
</div>

<style>
  .problem {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }

  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
  }

  @media (width < 48rem) {
    .actions :global(a) {
      width: 100%;
    }
  }
</style>
