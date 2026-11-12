<script lang="ts">
  // A person's identity on their profile (ACC-009, ACC-015): username, full name and email with its
  // confirmed state (AUTH-014). People with `user.update` edit them; the edit shows at once, and a
  // change someone else made meanwhile is reported here (architecture.md §15).
  import CircleCheck from '@lucide/svelte/icons/circle-check';
  import TriangleAlert from '@lucide/svelte/icons/triangle-alert';

  import { ApiError } from '#lib/api/client.ts';
  import { fieldErrorText, fieldErrorTexts } from '#lib/i18n/messages/errors.ts';
  import { ConflictError } from '#lib/store/index.ts';
  import Button from '#lib/ui/Button.svelte';
  import Notice from '#lib/ui/Notice.svelte';
  import TextField from '#lib/ui/TextField.svelte';
  import { toasts } from '#lib/ui/toast.svelte.ts';

  import * as m from './messages.ts';
  import { refusalText } from './problems.ts';
  import { saveProfile, type DirectoryAccount, type ProfileChanges } from './users.ts';

  interface Props {
    account: DirectoryAccount;
    /** The viewer may edit the profile. */
    editable: boolean;
    /** Outgoing email works on this server. */
    emailEnabled: boolean;
  }

  let { account, editable, emailEnabled }: Props = $props();

  // The form starts from the profile and follows it until someone types.
  let fullName = $state('');
  let username = $state('');
  let email = $state('');
  let edited = $state(false);
  let busy = $state(false);
  let errors = $state<Record<string, string>>({});
  let problem = $state<string | null>(null);
  let form: HTMLFormElement | undefined = $state();

  $effect(() => {
    if (edited) return;
    fullName = account.full_name;
    username = account.username;
    email = account.email ?? '';
  });

  const own = {
    username: { taken: m.usernameTaken },
    email: { taken: m.emailTaken },
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
    if (!email.trim() && account.email) found.email = fieldErrorText.required();
    errors = found;
    if (Object.keys(found).length > 0) {
      queueMicrotask(focusFirstError);
      return;
    }
    const changes: ProfileChanges = {};
    if (fullName.trim() !== account.full_name) changes.full_name = fullName.trim();
    if (username.trim() !== account.username) changes.username = username.trim();
    if (email.trim() && email.trim().toLowerCase() !== (account.email ?? '').toLowerCase())
      changes.email = email.trim();
    if (Object.keys(changes).length === 0) {
      edited = false;
      return;
    }
    busy = true;
    try {
      await saveProfile(account.id, changes);
      edited = false;
      toasts.show({ tone: 'success', message: m.profileSaved() });
    } catch (error) {
      if (error instanceof ConflictError) {
        // The row shows the other change now; the form follows it again.
        problem = m.profileConflict();
        edited = false;
      } else if (error instanceof ApiError && error.code === 'invalid_request') {
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

{#snippet emailState()}
  {#if account.email}
    {#if account.email_verified}
      <p class="state ok"><CircleCheck aria-hidden="true" />{m.emailConfirmed()}</p>
    {:else}
      <p class="state warn"><TriangleAlert aria-hidden="true" />{m.emailNotConfirmed()}</p>
    {/if}
  {/if}
{/snippet}

{#if editable}
  <form class="form" novalidate onsubmit={save} bind:this={form}>
    <p class="hint">{m.profileEditHint()}</p>
    {#if problem}<Notice tone="warning">{problem}</Notice>{/if}
    <TextField
      label={m.fullName()}
      bind:value={fullName}
      oninput={() => (edited = true)}
      error={errors.full_name}
      maxlength={100}
      autocomplete="off"
      required
    />
    <TextField
      label={m.username()}
      bind:value={username}
      oninput={() => (edited = true)}
      hint={m.usernameHint()}
      error={errors.username}
      maxlength={50}
      autocomplete="off"
      autocapitalize="none"
      spellcheck="false"
      required
    />
    <div class="email">
      <TextField
        label={m.email()}
        type="email"
        bind:value={email}
        oninput={() => (edited = true)}
        hint={emailEnabled ? m.emailChangeHint() : undefined}
        error={errors.email}
        maxlength={254}
        autocomplete="off"
        autocapitalize="none"
        spellcheck="false"
        required={account.email !== null}
      />
      {@render emailState()}
    </div>
    <div class="actions">
      <Button type="submit" variant="primary" loading={busy} disabled={!edited}>
        {m.saveProfile()}
      </Button>
    </div>
  </form>
{:else}
  <dl class="facts">
    <div>
      <dt>{m.username()}</dt>
      <dd class="mono">{account.username}</dd>
    </div>
    <div>
      <dt>{m.fullName()}</dt>
      <dd>{account.full_name}</dd>
    </div>
    <div>
      <dt>{m.email()}</dt>
      <dd>
        <span>{account.email ?? m.noEmail()}</span>
        {@render emailState()}
      </dd>
    </div>
  </dl>
{/if}

<style>
  .form {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
    max-width: 28rem;
  }

  .hint {
    color: var(--color-text-muted);
    font-size: var(--text-sm);
  }

  .email {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
  }

  .facts {
    display: grid;
    grid-template-columns: max-content minmax(0, 1fr);
    gap: var(--space-2) var(--space-6);
    margin: 0;
  }

  .facts > div {
    display: contents;
  }

  dt {
    color: var(--color-text-muted);
  }

  dd {
    margin: 0;
    overflow-wrap: anywhere;
  }

  .mono {
    font-family: var(--font-mono);
  }

  .state {
    display: flex;
    align-items: center;
    gap: var(--space-1_5);
    font-size: var(--text-sm);
  }

  .state :global(svg) {
    flex: none;
    width: var(--icon-md);
    height: var(--icon-md);
  }

  .ok :global(svg) {
    color: var(--color-success);
  }

  .warn :global(svg) {
    color: var(--color-warning);
  }

  @media (width < 48rem) {
    .facts {
      grid-template-columns: minmax(0, 1fr);
      gap: var(--space-0_5);
    }

    .facts > div + div dt {
      margin-top: var(--space-2);
    }

    .actions :global(button) {
      width: 100%;
    }
  }
</style>
