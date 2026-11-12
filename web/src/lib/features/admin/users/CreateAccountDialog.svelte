<script lang="ts">
  // Creating an account with a password (ACC-003): full name, email, a username taken from the
  // email until it is edited, the password twice, and whether the person must choose their own
  // at the first sign-in (AUTH-010). Problems are reported inside the dialog, by field.
  import { ApiError } from '#lib/api/client.ts';
  import type { Account } from '#lib/api/schema.gen.ts';
  import * as common from '#lib/i18n/messages/common.ts';
  import { fieldErrorText, fieldErrorTexts } from '#lib/i18n/messages/errors.ts';
  import Button from '#lib/ui/Button.svelte';
  import Checkbox from '#lib/ui/Checkbox.svelte';
  import Dialog from '#lib/ui/Dialog.svelte';
  import Notice from '#lib/ui/Notice.svelte';
  import TextField from '#lib/ui/TextField.svelte';

  import * as m from './messages.ts';
  import { refusalText } from './problems.ts';
  import { createAccount, usernameFromEmail } from './users.ts';

  interface Props {
    open?: boolean;
    oncreated: (account: Account) => unknown;
  }

  let { open = $bindable(false), oncreated }: Props = $props();

  const formId = $props.id();
  let fullName = $state('');
  let email = $state('');
  let username = $state('');
  let usernameEdited = $state(false);
  let password = $state('');
  let repeat = $state('');
  let mustChange = $state(true);
  let busy = $state(false);
  let errors = $state<Record<string, string>>({});
  let problem = $state<string | null>(null);
  let form: HTMLFormElement | undefined = $state();

  const own = {
    username: { taken: m.usernameTaken },
    email: { taken: m.emailTaken },
    password: { too_short: m.passwordTooShort },
  };

  // Every opening starts from a clean form.
  $effect(() => {
    if (!open) return;
    fullName = email = username = password = repeat = '';
    usernameEdited = false;
    mustChange = true;
    errors = {};
    problem = null;
  });

  // The username follows the email until the person types one.
  $effect(() => {
    const typed = email;
    if (!usernameEdited) username = usernameFromEmail(typed);
  });

  function focusFirstError(): void {
    form?.querySelector<HTMLElement>('[aria-invalid="true"]')?.focus();
  }

  async function submit(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    problem = null;
    const found: Record<string, string> = {};
    if (!fullName.trim()) found.full_name = fieldErrorText.required();
    if (!email.trim()) found.email = fieldErrorText.required();
    if (!username.trim()) found.username = fieldErrorText.required();
    if (!password) found.password = fieldErrorText.required();
    else if (Array.from(password).length < 8) found.password = m.passwordTooShort();
    else if (password !== repeat) found.repeat = m.passwordsDiffer();
    errors = found;
    if (Object.keys(found).length > 0) {
      queueMicrotask(focusFirstError);
      return;
    }
    busy = true;
    try {
      const account = await createAccount({
        full_name: fullName.trim(),
        email: email.trim(),
        username: username.trim(),
        password,
        must_change_password: mustChange,
      });
      open = false;
      await oncreated(account);
    } catch (error) {
      if (error instanceof ApiError && error.code === 'invalid_request') {
        errors = fieldErrorTexts(error.body?.fields ?? [], own);
        if (Object.keys(errors).length === 0) problem = refusalText(error);
        queueMicrotask(focusFirstError);
      } else {
        problem = refusalText(error);
      }
    } finally {
      busy = false;
    }
  }
</script>

<Dialog bind:open title={m.newAccountTitle()} dismissible={!busy}>
  <form id={formId} class="form" novalidate onsubmit={submit} bind:this={form}>
    <p class="intro">{m.newAccountIntro()}</p>
    {#if problem}<Notice tone="danger">{problem}</Notice>{/if}
    <TextField
      label={m.fullName()}
      bind:value={fullName}
      error={errors.full_name}
      maxlength={100}
      autocomplete="off"
      required
    />
    <TextField
      label={m.email()}
      type="email"
      bind:value={email}
      error={errors.email}
      maxlength={254}
      autocomplete="off"
      autocapitalize="none"
      spellcheck="false"
      required
    />
    <TextField
      label={m.username()}
      bind:value={username}
      oninput={() => (usernameEdited = true)}
      hint={m.usernameHint()}
      error={errors.username}
      maxlength={50}
      autocomplete="off"
      autocapitalize="none"
      spellcheck="false"
      required
    />
    <TextField
      label={m.password()}
      type="password"
      bind:value={password}
      hint={m.passwordHint()}
      error={errors.password}
      autocomplete="new-password"
      required
    />
    <TextField
      label={m.repeatPassword()}
      type="password"
      bind:value={repeat}
      error={errors.repeat}
      autocomplete="new-password"
      required
    />
    <Checkbox
      label={m.mustChangePassword()}
      hint={m.mustChangePasswordHint()}
      bind:checked={mustChange}
    />
  </form>

  {#snippet footer()}
    <Button
      disabled={busy}
      onclick={() => {
        open = false;
      }}
    >
      {common.cancel()}
    </Button>
    <Button type="submit" form={formId} variant="primary" loading={busy}>
      {m.createAccount()}
    </Button>
  {/snippet}
</Dialog>

<style>
  .form {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }

  .intro {
    color: var(--color-text-muted);
  }
</style>
