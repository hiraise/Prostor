<script lang="ts">
  // Password sign-in (AUTH-001, AUTH-003). Wrong credentials get one message for both fields, so
  // the form never tells whether an account exists.
  import { ApiError } from '#lib/api/client.ts';
  import { fieldErrorText, fieldErrorTexts } from '#lib/i18n/messages/errors.ts';
  import { forgotPasswordHref, signIn, type EndReason } from '#lib/session/index.ts';
  import { announce } from '#lib/ui/announce.svelte.ts';
  import Button from '#lib/ui/Button.svelte';
  import Checkbox from '#lib/ui/Checkbox.svelte';
  import Notice from '#lib/ui/Notice.svelte';
  import TextField from '#lib/ui/TextField.svelte';
  import { describeError } from '#lib/ui/toast.svelte.ts';

  import * as m from './messages.ts';

  interface Props {
    /** Why the person is here again, shown above the form. */
    reason?: EndReason | null;
    /** Outgoing email works, so a forgotten password can be reset by a link (AUTH-008). */
    emailEnabled?: boolean;
    onsignedin: () => unknown;
  }

  let { reason = null, emailEnabled = false, onsignedin }: Props = $props();

  let login = $state('');
  let password = $state('');
  let remember = $state(false);
  let busy = $state(false);
  let problem = $state<string | null>(null);
  let errors = $state<Record<string, string>>({});
  let passwordInput: HTMLInputElement | undefined = $state();

  async function submit(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    problem = null;
    errors = {};
    if (!login.trim()) errors.login = fieldErrorText.required();
    if (!password) errors.password = fieldErrorText.required();
    if (Object.keys(errors).length > 0) return;

    busy = true;
    try {
      await signIn({ login: login.trim(), password, remember });
      await onsignedin();
    } catch (error) {
      if (error instanceof ApiError && error.code === 'invalid_credentials') {
        problem = m.wrongCredentials();
        password = '';
        passwordInput?.focus();
      } else if (error instanceof ApiError && error.code === 'rate_limited') {
        problem = m.tooManyAttempts();
      } else if (error instanceof ApiError && error.code === 'invalid_request') {
        errors = fieldErrorTexts(error.body?.fields ?? []);
      } else {
        problem = describeError(error).message;
      }
      if (problem) announce(problem, 'assertive');
    } finally {
      busy = false;
    }
  }
</script>

<form class="form" novalidate onsubmit={submit}>
  {#if problem}
    <Notice tone="danger">{problem}</Notice>
  {:else if reason === 'signed-out'}
    <Notice tone="info">{m.signedOut()}</Notice>
  {:else if reason === 'expired'}
    <Notice tone="info">{m.sessionEnded()}</Notice>
  {/if}
  <TextField
    label={m.login()}
    bind:value={login}
    error={errors.login}
    autocomplete="username"
    autocapitalize="none"
    spellcheck="false"
    required
  />
  <TextField
    label={m.password()}
    type="password"
    bind:value={password}
    bind:input={passwordInput}
    error={errors.password}
    autocomplete="current-password"
    required
  />
  <Checkbox label={m.rememberMe()} hint={m.rememberMeHint()} bind:checked={remember} />
  <Button type="submit" variant="primary" size="lg" loading={busy}>{m.signIn()}</Button>
  {#if emailEnabled}
    <!-- forgotPasswordHref() resolves the route. -->
    <!-- eslint-disable-next-line svelte/no-navigation-without-resolve -->
    <p class="help"><a href={forgotPasswordHref()}>{m.forgotPasswordLink()}</a></p>
  {:else}
    <p class="help">{m.forgotPassword()}</p>
  {/if}
</form>

<style>
  .form {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }

  .form :global(button[type='submit']) {
    width: 100%;
  }

  .help {
    color: var(--color-text-muted);
    font-size: var(--text-sm);
  }

  .help a {
    color: var(--color-link);
  }
</style>
