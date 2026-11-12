<script lang="ts">
  // Joining from an invitation (ACC-004): the account exists already; the person checks the name
  // and username it started with and chooses a password, which also confirms the address.
  import { onMount } from 'svelte';

  import { ApiError } from '#lib/api/client.ts';
  import type { LinkInfo, Session } from '#lib/api/schema.gen.ts';
  import { fieldErrorText, fieldErrorTexts } from '#lib/i18n/messages/errors.ts';
  import Button from '#lib/ui/Button.svelte';
  import Spinner from '#lib/ui/Spinner.svelte';
  import TextField from '#lib/ui/TextField.svelte';
  import { showError } from '#lib/ui/toast.svelte.ts';

  import LinkProblem from './LinkProblem.svelte';
  import { acceptInvitation, inspectLink, takeSecret } from './links.ts';
  import * as m from './messages.ts';
  import NewPasswordFields from './NewPasswordFields.svelte';
  import { checkNewPassword } from './passwords.ts';

  let { ondone }: { ondone: (session: Session) => unknown } = $props();

  let token = $state<string | null>(null);
  let link = $state<LinkInfo | 'invalid' | 'expired' | null>(null);
  let fullName = $state('');
  let username = $state('');
  let password = $state('');
  let repeat = $state('');
  let errors = $state<Record<string, string>>({});
  let busy = $state(false);

  onMount(async () => {
    token = takeSecret();
    if (!token) {
      link = 'invalid';
      return;
    }
    try {
      const found = await inspectLink(token);
      if (found === 'invalid' || found.purpose !== 'invitation') link = 'invalid';
      else if (found.expired) link = 'expired';
      else {
        link = found;
        fullName = found.full_name;
        username = found.username;
      }
    } catch (error) {
      showError(error);
      link = 'invalid';
    }
  });

  async function submit(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    const found = checkNewPassword(password, repeat);
    if (!fullName.trim()) found.full_name = fieldErrorText.required();
    if (!username.trim()) found.username = fieldErrorText.required();
    errors = found;
    if (Object.keys(found).length > 0 || !token) return;
    busy = true;
    try {
      await ondone(await acceptInvitation(token, password, fullName.trim(), username.trim()));
    } catch (error) {
      if (error instanceof ApiError && error.code === 'invalid_request') {
        errors = fieldErrorTexts(error.body?.fields ?? [], {
          username: { taken: m.usernameTaken },
          password: { too_short: m.passwordTooShort },
        });
      } else if (error instanceof ApiError && error.code === 'conflict') {
        link = 'expired';
      } else if (error instanceof ApiError && error.code === 'not_found') {
        link = 'invalid';
      } else {
        showError(error);
      }
    } finally {
      busy = false;
    }
  }
</script>

{#if link === null}
  <Spinner />
{:else if link === 'invalid' || link === 'expired'}
  <LinkProblem problem={link} expiredText={m.invitationExpired()} />
{:else}
  <form class="form" novalidate onsubmit={submit}>
    <p class="intro">{m.invitationIntro({ email: link.email })}</p>
    <TextField
      label={m.fullName()}
      bind:value={fullName}
      error={errors.full_name}
      autocomplete="name"
      required
    />
    <TextField
      label={m.username()}
      bind:value={username}
      hint={m.usernameHint()}
      error={errors.username}
      autocomplete="username"
      autocapitalize="none"
      spellcheck="false"
      required
    />
    <NewPasswordFields bind:password bind:repeat {errors} />
    <Button type="submit" variant="primary" size="lg" loading={busy}>{m.join()}</Button>
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
    overflow-wrap: anywhere;
  }
</style>
