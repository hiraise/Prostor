<script lang="ts">
  import { brand } from '#lib/i18n/messages/brand.ts';
  // The signed-in person's account (UI-010): the picture and the profile (ACC-017, AUTH-014),
  // their groups and roles (ACC-018), the password (AUTH-009) and API tokens (AUTH-029).
  // Preferences and notifications have their own sections; the security page with sessions comes
  // with its feature.
  import type { Account, AccountSummary, ApiToken } from '#lib/api/schema.gen.ts';
  import { NotificationSettings } from '#lib/features/notifications/index.ts';
  import * as ntf from '#lib/features/notifications/messages.ts';
  import type { EntityHandle, ListHandle } from '#lib/store/index.ts';
  import PageHeader from '#lib/ui/PageHeader.svelte';

  import AvatarForm from './AvatarForm.svelte';
  import InterfaceSettings from './InterfaceSettings.svelte';
  import * as m from './messages.ts';
  import MyAccess from './MyAccess.svelte';
  import PasswordForm from './PasswordForm.svelte';
  import ProfileForm from './ProfileForm.svelte';
  import TokenList from './TokenList.svelte';

  interface Props {
    account: EntityHandle<AccountSummary | Account>;
    tokens: ListHandle<ApiToken>;
    emailEnabled: boolean;
    /** Pictures can be taken from Gravatar. */
    gravatar: boolean;
  }

  let { account, tokens, emailEnabled, gravatar }: Props = $props();

  const user = $derived(account.current && 'tier' in account.current ? account.current : null);
</script>

<svelte:head><title>{m.accountTitle()} · {brand()}</title></svelte:head>

<PageHeader title={m.accountTitle()} />
<div class="page">
  {#if user}
    <AvatarForm account={user} {gravatar} />
    <section class="section" aria-labelledby="account-profile">
      <h2 id="account-profile">{m.profile()}</h2>
      <ProfileForm account={user} {emailEnabled} />
    </section>
    <section class="section" aria-labelledby="account-access">
      <h2 id="account-access">{m.accessTitle()}</h2>
      <MyAccess account={user} />
    </section>
  {/if}

  <section class="section" aria-labelledby="account-interface">
    <h2 id="account-interface">{m.interfaceTitle()}</h2>
    <InterfaceSettings />
  </section>

  <section class="section" aria-labelledby="account-notifications">
    <h2 id="account-notifications">{ntf.settingsTitle()}</h2>
    <NotificationSettings email={user?.email ?? null} {emailEnabled} />
  </section>

  <section class="section" aria-labelledby="account-password">
    <h2 id="account-password">{m.changePasswordTitle()}</h2>
    <PasswordForm />
  </section>

  <section class="section" aria-labelledby="account-tokens">
    <h2 id="account-tokens">{m.tokensTitle()}</h2>
    <TokenList {tokens} />
  </section>
</div>

<style>
  .page {
    display: flex;
    flex-direction: column;
    gap: var(--space-8);
    max-width: var(--content-max);
    padding: var(--space-2) var(--space-6) var(--space-12);
  }

  .section {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }

  h2 {
    font-size: var(--text-lg);
  }

  @media (width < 48rem) {
    .page {
      gap: var(--space-6);
      padding: var(--space-2) var(--space-4) var(--space-8);
    }
  }
</style>
