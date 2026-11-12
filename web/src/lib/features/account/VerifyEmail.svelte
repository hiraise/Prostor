<script lang="ts">
  // Confirming an address from an emailed link (AUTH-014). Works without signing in, so a link
  // opened on another device confirms too.
  import { onMount } from 'svelte';
  import { resolve } from '$app/paths';

  import { ApiError } from '#lib/api/client.ts';
  import Button from '#lib/ui/Button.svelte';
  import Notice from '#lib/ui/Notice.svelte';
  import Spinner from '#lib/ui/Spinner.svelte';
  import { describeError } from '#lib/ui/toast.svelte.ts';

  import LinkProblem from './LinkProblem.svelte';
  import { confirmEmail, takeSecret } from './links.ts';
  import * as m from './messages.ts';

  let phase = $state<'working' | 'done' | 'invalid' | 'expired' | 'failed'>('working');
  let email = $state('');
  let message = $state('');

  onMount(async () => {
    const token = takeSecret();
    if (!token) {
      phase = 'invalid';
      return;
    }
    try {
      email = await confirmEmail(token);
      phase = 'done';
    } catch (error) {
      if (error instanceof ApiError && error.code === 'not_found') phase = 'invalid';
      else if (error instanceof ApiError && error.code === 'conflict') phase = 'expired';
      else {
        message = describeError(error).message;
        phase = 'failed';
      }
    }
  });
</script>

{#if phase === 'working'}
  <p class="working"><Spinner /> {m.verifying()}</p>
{:else if phase === 'done'}
  <Notice tone="success">{m.verified({ email })}</Notice>
  <Button href={resolve('/')} variant="primary">{m.openTrackflow()}</Button>
{:else if phase === 'invalid' || phase === 'expired'}
  <LinkProblem problem={phase} expiredText={m.verificationExpired()} />
{:else}
  <Notice tone="danger">{message}</Notice>
{/if}

<style>
  .working {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    color: var(--color-text-muted);
  }
</style>
