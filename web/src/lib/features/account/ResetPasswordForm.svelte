<script lang="ts">
  // Choosing a new password from an emailed link (AUTH-008). The link signs the person in and
  // ends their other sessions.
  import { onMount } from 'svelte';
  import { resolve } from '$app/paths';

  import { ApiError } from '#lib/api/client.ts';
  import type { LinkInfo, Session } from '#lib/api/schema.gen.ts';
  import { fieldErrorTexts } from '#lib/i18n/messages/errors.ts';
  import Button from '#lib/ui/Button.svelte';
  import Spinner from '#lib/ui/Spinner.svelte';
  import { showError } from '#lib/ui/toast.svelte.ts';

  import LinkProblem from './LinkProblem.svelte';
  import { confirmReset, inspectLink, takeSecret } from './links.ts';
  import * as m from './messages.ts';
  import NewPasswordFields from './NewPasswordFields.svelte';
  import { checkNewPassword } from './passwords.ts';

  let { ondone }: { ondone: (session: Session) => unknown } = $props();

  let token = $state<string | null>(null);
  let link = $state<LinkInfo | 'invalid' | 'expired' | null>(null);
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
      link =
        found === 'invalid' || found.purpose !== 'password_reset'
          ? 'invalid'
          : found.expired
            ? 'expired'
            : found;
    } catch (error) {
      showError(error);
      link = 'invalid';
    }
  });

  async function submit(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    errors = checkNewPassword(password, repeat, 'new_password');
    if (Object.keys(errors).length > 0 || !token) return;
    busy = true;
    try {
      await ondone(await confirmReset(token, password));
    } catch (error) {
      if (error instanceof ApiError && error.code === 'invalid_request') {
        errors = fieldErrorTexts(error.body?.fields ?? [], {
          new_password: { too_short: m.passwordTooShort },
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
  <LinkProblem
    problem={link}
    expiredText={m.resetExpired()}
    renewHref={resolve('/(public)/forgot-password')}
  />
{:else}
  <form class="form" novalidate onsubmit={submit}>
    <p class="intro">{m.resetFor({ username: link.username })}</p>
    <NewPasswordFields
      bind:password
      bind:repeat
      {errors}
      path="new_password"
      username={link.username}
    />
    <Button type="submit" variant="primary" size="lg" loading={busy}>{m.setPassword()}</Button>
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
