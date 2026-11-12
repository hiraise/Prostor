<script lang="ts">
  // Choosing a new password before anything else, when an administrator asked for it or the
  // account was given a one-time password (AUTH-010).
  import { ApiError, api } from '#lib/api/client.ts';
  import { fieldErrorText, fieldErrorTexts } from '#lib/i18n/messages/errors.ts';
  import { currentUser, passwordChanged } from '#lib/session/index.ts';
  import Button from '#lib/ui/Button.svelte';
  import TextField from '#lib/ui/TextField.svelte';
  import { showError } from '#lib/ui/toast.svelte.ts';

  import * as m from './messages.ts';
  import NewPasswordFields from './NewPasswordFields.svelte';
  import { checkNewPassword } from './passwords.ts';

  let { ondone }: { ondone: () => unknown } = $props();

  let current = $state('');
  let password = $state('');
  let repeat = $state('');
  let errors = $state<Record<string, string>>({});
  let busy = $state(false);

  async function submit(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    const found = checkNewPassword(password, repeat, 'new_password');
    if (!current) found.current_password = fieldErrorText.required();
    else if (password && password === current) found.new_password = m.forcedSameAsOld();
    errors = found;
    if (Object.keys(found).length > 0) return;
    busy = true;
    try {
      await api.post('/users/me/password', {
        body: { current_password: current, new_password: password },
      });
      passwordChanged();
      await ondone();
    } catch (error) {
      if (error instanceof ApiError && error.code === 'invalid_request') {
        errors = fieldErrorTexts(error.body?.fields ?? [], {
          current_password: { incorrect: m.currentPasswordWrong },
          new_password: { too_short: m.passwordTooShort },
        });
      } else {
        showError(error);
      }
    } finally {
      busy = false;
    }
  }
</script>

<form class="form" novalidate onsubmit={submit}>
  <p class="intro">{m.forcedIntro()}</p>
  <TextField
    label={m.currentPassword()}
    type="password"
    bind:value={current}
    error={errors.current_password}
    autocomplete="current-password"
    required
  />
  <NewPasswordFields
    bind:password
    bind:repeat
    {errors}
    path="new_password"
    username={currentUser()?.username}
  />
  <Button type="submit" variant="primary" size="lg" loading={busy}>{m.setPassword()}</Button>
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

  .intro {
    color: var(--color-text-muted);
  }
</style>
