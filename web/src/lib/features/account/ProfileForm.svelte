<script lang="ts">
  // One's own profile (ACC-017): full name, username and email. A new email is confirmed with the
  // current password and then by a link sent to it (AUTH-014); an unconfirmed address can get a
  // new link, and a test message shows that email arrives (NTF-007).
  import CircleCheck from '@lucide/svelte/icons/circle-check';
  import TriangleAlert from '@lucide/svelte/icons/triangle-alert';

  import { ApiError, api } from '#lib/api/client.ts';
  import type { Account } from '#lib/api/schema.gen.ts';
  import { keepAccount } from '#lib/features/access/index.ts';
  import { fieldErrorText, fieldErrorTexts } from '#lib/i18n/messages/errors.ts';
  import { updateCurrentUser } from '#lib/session/index.ts';
  import Time from '#lib/time/Time.svelte';
  import Button from '#lib/ui/Button.svelte';
  import Notice from '#lib/ui/Notice.svelte';
  import TextField from '#lib/ui/TextField.svelte';
  import { showError, toasts } from '#lib/ui/toast.svelte.ts';

  import * as m from './messages.ts';

  interface Props {
    account: Account;
    /** Outgoing email works on this server. */
    emailEnabled: boolean;
  }

  let { account, emailEnabled }: Props = $props();

  // The form starts from the profile and follows it until the person types.
  let fullName = $state('');
  let username = $state('');
  let email = $state('');
  let currentPassword = $state('');
  let edited = $state(false);
  let busy = $state(false);
  let sending = $state<'verification' | 'test' | null>(null);
  let errors = $state<Record<string, string>>({});
  let problem = $state<string | null>(null);
  let form: HTMLFormElement | undefined = $state();

  $effect(() => {
    if (edited) return;
    fullName = account.full_name;
    username = account.username;
    email = account.email ?? '';
  });

  const emailChanged = $derived(email.trim().toLowerCase() !== (account.email ?? '').toLowerCase());
  const hasPassword = $derived(account.password?.has_password ?? false);

  const own = {
    username: { taken: m.usernameTaken },
    email: { taken: m.emailTaken },
    current_password: { incorrect: m.currentPasswordWrong },
  };

  function focusFirstError(): void {
    form?.querySelector<HTMLElement>('[aria-invalid="true"]')?.focus();
  }

  async function save(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    problem = null;
    const found: Record<string, string> = {};
    if (!fullName.trim()) found.full_name = fieldErrorText.required();
    if (!username.trim()) found.username = fieldErrorText.required();
    if (!email.trim()) found.email = fieldErrorText.required();
    if (emailChanged && hasPassword && !currentPassword)
      found.current_password = fieldErrorText.required();
    errors = found;
    if (Object.keys(found).length > 0) {
      queueMicrotask(focusFirstError);
      return;
    }
    busy = true;
    try {
      const saved = await api.patch('/users/{id}', {
        path: { id: account.id },
        body: {
          full_name: fullName.trim() === account.full_name ? null : fullName.trim(),
          username: username.trim() === account.username ? null : username.trim(),
          email: emailChanged ? email.trim() : null,
          current_password: emailChanged ? currentPassword : null,
          version: account.version,
        },
      });
      keepAccount(saved, 'full');
      updateCurrentUser({
        id: saved.id,
        username: saved.username,
        full_name: saved.full_name,
        email: saved.email,
        email_verified: saved.email_verified,
        avatar_color: saved.avatar_color,
        avatar_url: saved.avatar_url,
        version: saved.version,
      });
      currentPassword = '';
      edited = false;
      toasts.show({
        tone: 'success',
        message:
          emailChanged && emailEnabled && saved.email
            ? m.verificationSent({ email: saved.email })
            : m.profileSaved(),
      });
    } catch (error) {
      if (error instanceof ApiError && error.code === 'invalid_request') {
        errors = fieldErrorTexts(error.body?.fields ?? [], own);
        queueMicrotask(focusFirstError);
      } else if (error instanceof ApiError && error.code === 'conflict') {
        problem = m.profileChangedElsewhere();
        edited = false;
        const fresh = await api.get('/users/{id}', { path: { id: account.id } }).catch(() => null);
        if (fresh) keepAccount(fresh, 'full');
      } else {
        showError(error);
      }
    } finally {
      busy = false;
    }
  }

  async function send(kind: 'verification' | 'test'): Promise<void> {
    sending = kind;
    try {
      if (kind === 'verification') {
        await api.post('/users/{id}/email-verification', { path: { id: account.id } });
        toasts.show({
          tone: 'success',
          message: m.verificationSent({ email: account.email ?? '' }),
        });
      } else {
        const sent = await api.post('/users/me/test-email');
        toasts.show({ tone: 'success', message: m.testEmailSent({ email: sent.email }) });
      }
    } catch (error) {
      showError(error);
    } finally {
      sending = null;
    }
  }
</script>

<div class="profile">
  <p class="hint">{m.profileHint()}</p>
  <form class="form" novalidate onsubmit={save} bind:this={form}>
    {#if problem}<Notice tone="warning">{problem}</Notice>{/if}
    <TextField
      label={m.fullName()}
      bind:value={fullName}
      oninput={() => (edited = true)}
      error={errors.full_name}
      autocomplete="name"
      required
    />
    <TextField
      label={m.username()}
      bind:value={username}
      oninput={() => (edited = true)}
      hint={m.usernameHint()}
      error={errors.username}
      autocomplete="username"
      autocapitalize="none"
      spellcheck="false"
      required
    />
    <TextField
      label={m.email()}
      type="email"
      bind:value={email}
      oninput={() => (edited = true)}
      hint={hasPassword ? m.emailChangeHint() : undefined}
      error={errors.email}
      autocomplete="email"
      required
    />
    {#if emailChanged && hasPassword}
      <TextField
        label={m.currentPassword()}
        type="password"
        bind:value={currentPassword}
        error={errors.current_password}
        autocomplete="current-password"
        required
      />
    {/if}
    <div class="actions">
      <Button type="submit" variant="primary" loading={busy} disabled={!edited}>
        {m.saveProfile()}
      </Button>
    </div>
  </form>

  {#if account.email}
    <div class="email-state">
      {#if account.email_verified}
        <p class="state ok">
          <CircleCheck aria-hidden="true" />
          <span>{account.email} · {m.emailVerified()}</span>
        </p>
      {:else}
        <p class="state warn">
          <TriangleAlert aria-hidden="true" />
          <span>{account.email} · {m.emailNotVerified()}. {m.emailNotVerifiedText()}</span>
        </p>
      {/if}
      {#if emailEnabled}
        <div class="buttons">
          {#if !account.email_verified}
            <Button
              size="sm"
              loading={sending === 'verification'}
              onclick={() => send('verification')}
            >
              {m.sendVerification()}
            </Button>
          {/if}
          <Button
            size="sm"
            variant="ghost"
            loading={sending === 'test'}
            onclick={() => send('test')}
          >
            {m.sendTestEmail()}
          </Button>
        </div>
      {:else}
        <p class="hint">{m.emailOff()}</p>
      {/if}
    </div>
  {/if}
  <p class="hint">
    {m.registered()}: <Time value={account.created_at} kind="date" />
  </p>
</div>

<style>
  .profile {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
    max-width: 28rem;
  }

  .form {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }

  .hint {
    color: var(--color-text-muted);
    font-size: var(--text-sm);
  }

  .email-state {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }

  .state {
    display: flex;
    align-items: flex-start;
    gap: var(--space-2);
    font-size: var(--text-sm);
    overflow-wrap: anywhere;
  }

  .state :global(svg) {
    flex: none;
    width: var(--icon-md);
    height: var(--icon-md);
    margin-top: 0.1em;
  }

  .ok :global(svg) {
    color: var(--color-success);
  }

  .warn :global(svg) {
    color: var(--color-warning);
  }

  .buttons {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
  }

  @media (width < 48rem) {
    .actions :global(button) {
      width: 100%;
    }
  }
</style>
