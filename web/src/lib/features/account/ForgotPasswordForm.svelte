<script lang="ts">
  // Asking for a password-reset link (AUTH-008). The answer is the same whether or not an account
  // has the address, so the page never tells who has an account.
  import { resolve } from '$app/paths';

  import { ApiError } from '#lib/api/client.ts';
  import { fieldErrorText } from '#lib/i18n/messages/errors.ts';
  import { announce } from '#lib/ui/announce.svelte.ts';
  import Button from '#lib/ui/Button.svelte';
  import Notice from '#lib/ui/Notice.svelte';
  import TextField from '#lib/ui/TextField.svelte';
  import { describeError } from '#lib/ui/toast.svelte.ts';

  import { requestReset } from './links.ts';
  import * as m from './messages.ts';

  let { emailEnabled }: { emailEnabled: boolean } = $props();

  let email = $state('');
  let error = $state<string | null>(null);
  let problem = $state<string | null>(null);
  let sent = $state(false);
  let busy = $state(false);

  async function submit(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    error = email.trim() ? null : fieldErrorText.required();
    problem = null;
    if (error) return;
    busy = true;
    try {
      await requestReset(email.trim());
      sent = true;
      announce(m.resetLinkSent());
    } catch (failure) {
      problem =
        failure instanceof ApiError && failure.code === 'conflict'
          ? m.resetUnavailable()
          : describeError(failure).message;
      announce(problem, 'assertive');
    } finally {
      busy = false;
    }
  }
</script>

{#if !emailEnabled}
  <Notice tone="info">{m.resetUnavailable()}</Notice>
  <Button href={resolve('/(public)/signin')}>{m.backToSignIn()}</Button>
{:else if sent}
  <Notice tone="success">{m.resetLinkSent()}</Notice>
  <Button href={resolve('/(public)/signin')}>{m.backToSignIn()}</Button>
{:else}
  <form class="form" novalidate onsubmit={submit}>
    <p class="intro">{m.forgotIntro()}</p>
    {#if problem}<Notice tone="danger">{problem}</Notice>{/if}
    <TextField
      label={m.email()}
      type="email"
      bind:value={email}
      {error}
      autocomplete="email"
      required
    />
    <Button type="submit" variant="primary" size="lg" loading={busy}>{m.sendResetLink()}</Button>
    <Button href={resolve('/(public)/signin')} variant="ghost">{m.backToSignIn()}</Button>
  </form>
{/if}

<style>
  .form {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }

  .form :global(button[type='submit']) {
    width: 100%;
  }

  .intro {
    color: var(--color-text-muted);
  }
</style>
