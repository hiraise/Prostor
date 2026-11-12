<script lang="ts">
  // Changing one's own password (AUTH-009). The server ends the other sessions of the account.
  import { ApiError, api } from '#lib/api/client.ts';
  import { fieldErrorText, fieldErrorTexts } from '#lib/i18n/messages/errors.ts';
  import { currentUser } from '#lib/session/index.ts';
  import Button from '#lib/ui/Button.svelte';
  import TextField from '#lib/ui/TextField.svelte';
  import { showError, toasts } from '#lib/ui/toast.svelte.ts';

  import * as m from './messages.ts';

  let current = $state('');
  let next = $state('');
  let repeat = $state('');
  let busy = $state(false);
  let errors = $state<Record<string, string>>({});
  let form: HTMLFormElement | undefined = $state();

  const own = {
    current_password: { incorrect: m.currentPasswordWrong },
    new_password: { too_short: m.passwordTooShort },
  };

  function focusFirstError(): void {
    form?.querySelector<HTMLElement>('[aria-invalid="true"]')?.focus();
  }

  async function submit(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    const found: Record<string, string> = {};
    if (!current) found.current_password = fieldErrorText.required();
    if (!next) found.new_password = fieldErrorText.required();
    else if (Array.from(next).length < 8) found.new_password = m.passwordTooShort();
    else if (next !== repeat) found.repeat = m.passwordsDiffer();
    errors = found;
    if (Object.keys(found).length > 0) {
      queueMicrotask(focusFirstError);
      return;
    }
    busy = true;
    try {
      await api.post('/users/me/password', {
        body: { current_password: current, new_password: next },
      });
      current = next = repeat = '';
      toasts.show({ tone: 'success', message: m.passwordChanged() });
    } catch (error) {
      if (error instanceof ApiError && error.code === 'invalid_request') {
        errors = fieldErrorTexts(error.body?.fields ?? [], own);
        queueMicrotask(focusFirstError);
      } else {
        showError(error);
      }
    } finally {
      busy = false;
    }
  }
</script>

<form class="form" novalidate onsubmit={submit} bind:this={form}>
  <!-- Lets password managers tie the new password to the account. -->
  <input
    class="hidden"
    type="text"
    autocomplete="username"
    value={currentUser()?.username ?? ''}
    readonly
    tabindex="-1"
  />
  <TextField
    label={m.currentPassword()}
    type="password"
    bind:value={current}
    error={errors.current_password}
    autocomplete="current-password"
    required
  />
  <TextField
    label={m.newPassword()}
    type="password"
    bind:value={next}
    hint={m.passwordHint()}
    error={errors.new_password}
    autocomplete="new-password"
    required
  />
  <TextField
    label={m.repeatNewPassword()}
    type="password"
    bind:value={repeat}
    error={errors.repeat}
    autocomplete="new-password"
    required
  />
  <p class="note">{m.changePasswordNote()}</p>
  <div class="actions">
    <Button type="submit" variant="primary" loading={busy}>{m.changePassword()}</Button>
  </div>
</form>

<style>
  .form {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
    max-width: 28rem;
  }

  .hidden {
    display: none;
  }

  .note {
    color: var(--color-text-muted);
    font-size: var(--text-sm);
  }

  @media (width < 48rem) {
    .actions :global(button) {
      width: 100%;
    }
  }
</style>
