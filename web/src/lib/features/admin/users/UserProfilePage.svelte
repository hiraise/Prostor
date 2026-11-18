<script lang="ts">
  // The profile of a person as others see it (UI-010, ACC-015): the name block for everyone who may
  // see the person; the profile, groups, projects (ACC-002) and roles with the details tier. People who manage accounts
  // edit the profile (ACC-009) and act on the account from one menu: disable or enable (ACC-005),
  // ask for a new password (AUTH-010), send the invitation again (ACC-004), send a confirmation
  // link (AUTH-014) and delete it (ACC-006). One's own profile is the account page.
  import ArrowLeft from '@lucide/svelte/icons/arrow-left';
  import Ban from '@lucide/svelte/icons/ban';
  import Ellipsis from '@lucide/svelte/icons/ellipsis';
  import KeyRound from '@lucide/svelte/icons/key-round';
  import Mail from '@lucide/svelte/icons/mail';
  import MailCheck from '@lucide/svelte/icons/mail-check';
  import Trash2 from '@lucide/svelte/icons/trash-2';
  import UserCheck from '@lucide/svelte/icons/user-check';
  import UserRoundX from '@lucide/svelte/icons/user-round-x';
  import { resolve } from '$app/paths';

  import type { AccountSummary } from '#lib/api/schema.gen.ts';
  import { PersonName, personName } from '#lib/features/access/index.ts';
  import * as common from '#lib/i18n/messages/common.ts';
  import { can, canSeeDirectory } from '#lib/session/index.ts';
  import type { EntityHandle } from '#lib/store/index.ts';
  import Time from '#lib/time/Time.svelte';
  import Button from '#lib/ui/Button.svelte';
  import Dialog from '#lib/ui/Dialog.svelte';
  import EmptyState from '#lib/ui/EmptyState.svelte';
  import Menu, { type MenuEntry } from '#lib/ui/Menu.svelte';
  import NotFoundState from '#lib/ui/NotFoundState.svelte';
  import Notice from '#lib/ui/Notice.svelte';
  import { toasts } from '#lib/ui/toast.svelte.ts';

  import AccountBadges from './AccountBadges.svelte';
  import DeleteDialog from './DeleteDialog.svelte';
  import DisableDialog from './DisableDialog.svelte';
  import EnableDialog from './EnableDialog.svelte';
  import GroupsSection from './GroupsSection.svelte';
  import * as m from './messages.ts';
  import { refusalText } from './problems.ts';
  import ProfileSection from './ProfileSection.svelte';
  import ProjectsSection from './ProjectsSection.svelte';
  import RolesSection from './RolesSection.svelte';
  import {
    askForNewPassword,
    forgetAccounts,
    reloadAccount,
    resendInvitation,
    sendVerification,
    type DirectoryAccount,
  } from './users.ts';

  interface Props {
    profile: EntityHandle<DirectoryAccount>;
    /** Outgoing email works on this server. */
    emailEnabled: boolean;
  }

  let { profile, emailEnabled }: Props = $props();

  const account = $derived(profile.current);
  const tier = $derived(account && 'tier' in account ? account.tier : 'basic');
  const password = $derived(account && 'password' in account ? account.password : null);
  const directoryHref = resolve('/(app)/admin/users');

  let disabling = $state(false);
  let enabling = $state(false);
  let deleting = $state(false);
  let asking = $state(false);
  let busy = $state(false);
  let problem = $state<string | null>(null);
  let replacedBy = $state<AccountSummary | null>(null);
  let rolesKey = $state(0);

  async function reload(): Promise<void> {
    if (account) await reloadAccount(account.id).catch(() => undefined);
  }

  async function groupsChanged(): Promise<void> {
    rolesKey += 1;
    await reload();
  }

  async function send(kind: 'invitation' | 'verification'): Promise<void> {
    const target = account;
    if (!target) return;
    try {
      if (kind === 'invitation') {
        await resendInvitation(target.id);
        toasts.show({
          tone: 'success',
          message: m.invitationResent({ email: target.email ?? '' }),
        });
      } else {
        await sendVerification(target.id);
        toasts.show({
          tone: 'success',
          message: m.verificationSent({ email: target.email ?? '' }),
        });
      }
    } catch (error) {
      toasts.show({
        tone: 'error',
        message: refusalText(
          error,
          kind === 'invitation' ? m.resendConflict : m.verificationConflict,
        ),
      });
    }
  }

  async function askPassword(): Promise<void> {
    const target = account;
    if (!target) return;
    busy = true;
    problem = null;
    try {
      await askForNewPassword(target.id);
      asking = false;
      toasts.show({ tone: 'success', message: m.passwordAsked() });
      await reload();
    } catch (error) {
      problem = refusalText(error, m.askPasswordConflict);
    } finally {
      busy = false;
    }
  }

  function deleted(ids: number[], replacement: AccountSummary): void {
    replacedBy = replacement;
    forgetAccounts(ids);
  }

  const actions = $derived.by((): MenuEntry[] => {
    if (!account || account.guest) return [];
    const items: MenuEntry[] = [];
    if (can('user.update')) {
      items.push(
        account.disabled
          ? {
              type: 'action',
              label: m.enableAccount(),
              icon: UserCheck,
              onSelect: () => (enabling = true),
            }
          : {
              type: 'action',
              label: m.disableAccount(),
              icon: Ban,
              onSelect: () => (disabling = true),
            },
      );
      if (password?.has_password) {
        items.push(
          password.must_change
            ? {
                type: 'action',
                label: m.newPasswordAsked(),
                icon: KeyRound,
                disabled: true,
                onSelect: () => undefined,
              }
            : {
                type: 'action',
                label: m.askNewPassword(),
                icon: KeyRound,
                onSelect: () => {
                  problem = null;
                  asking = true;
                },
              },
        );
      }
      if (emailEnabled && account.email && !account.email_verified && !account.invited) {
        items.push({
          type: 'action',
          label: m.sendVerification(),
          icon: MailCheck,
          onSelect: () => void send('verification'),
        });
      }
    }
    if (can('user.create') && account.invited && emailEnabled) {
      items.push({
        type: 'action',
        label: m.resendInvitation(),
        icon: Mail,
        onSelect: () => void send('invitation'),
      });
    }
    if (can('user.delete')) {
      if (items.length > 0) items.push({ type: 'separator' });
      items.push({
        type: 'action',
        label: m.deleteAccount(),
        icon: Trash2,
        danger: true,
        onSelect: () => (deleting = true),
      });
    }
    return items;
  });
</script>

<svelte:head>
  <title>{account ? personName(account) : m.deletedTitle()} · TaskMaster</title>
</svelte:head>

{#if replacedBy}
  <EmptyState
    icon={UserRoundX}
    title={m.deletedTitle()}
    description={m.deletedText({ name: personName(replacedBy) })}
    level={1}
  >
    {#snippet actions()}
      <Button href={directoryHref} variant="primary">{m.backToUsers()}</Button>
    {/snippet}
  </EmptyState>
{:else if !account}
  <NotFoundState />
{:else}
  <div class="page">
    {#if canSeeDirectory()}
      <!-- The address comes from resolve() above. -->
      <!-- eslint-disable-next-line svelte/no-navigation-without-resolve -->
      <a class="back" href={directoryHref}><ArrowLeft aria-hidden="true" />{m.allUsers()}</a>
    {/if}

    <header class="head">
      <div class="identity">
        <h1><PersonName person={account} size="lg" /></h1>
        <p class="sub">
          <span class="mono">{account.username}</span>
          <AccountBadges {account} />
        </p>
        <p class="registered">
          {m.registered()}: <Time value={account.created_at} kind="date" year="always" />
        </p>
      </div>
      {#if actions.length > 0}
        <Menu
          label={m.accountActions()}
          items={actions}
          placement={{ side: 'bottom', align: 'end' }}
        >
          {#snippet trigger(props)}
            <Button icon={Ellipsis} {...props}>{m.actions()}</Button>
          {/snippet}
        </Menu>
      {/if}
    </header>

    {#if account.disabled}
      <Notice tone="warning">
        {m.disabledSince()}
        <Time value={account.disabled.at} kind="date" year="always" />.
        {#if account.disabled.reason}{m.disabledReason({ reason: account.disabled.reason })}{/if}
      </Notice>
    {:else if account.invited}
      <Notice>{m.invitedNotice()}</Notice>
    {:else if account.guest}
      <Notice>{m.guestNotice()}</Notice>
    {/if}

    {#if tier === 'basic'}
      <p class="muted">{m.basicTierNote()}</p>
    {:else}
      <section class="section" aria-labelledby="profile-identity">
        <h2 id="profile-identity">{m.profileTitle()}</h2>
        <ProfileSection {account} editable={can('user.update') && !account.guest} {emailEnabled} />
      </section>
      <GroupsSection {account} onchanged={groupsChanged} />
      <ProjectsSection projects={account.projects} />
      {#if can('system.read')}
        <!-- Joining or leaving a group changes the roles: they load again. -->
        {#key rolesKey}<RolesSection {account} />{/key}
      {/if}
    {/if}
  </div>

  <DisableDialog bind:open={disabling} accounts={[account]} ondone={reload} />
  <EnableDialog bind:open={enabling} accounts={[account]} ondone={reload} />
  <DeleteDialog bind:open={deleting} accounts={[account]} ondone={deleted} />
  <Dialog
    bind:open={asking}
    title={m.askPasswordTitle({ name: personName(account) })}
    size="sm"
    dismissible={!busy}
  >
    <div class="body">
      <p>{m.askPasswordText()}</p>
      {#if problem}<Notice tone="danger">{problem}</Notice>{/if}
    </div>
    {#snippet footer()}
      <Button
        disabled={busy}
        onclick={() => {
          asking = false;
        }}
      >
        {common.cancel()}
      </Button>
      <Button variant="primary" loading={busy} onclick={askPassword}>{m.askNewPassword()}</Button>
    {/snippet}
  </Dialog>
{/if}

<style>
  .page {
    display: flex;
    flex-direction: column;
    gap: var(--space-6);
    /* One reading column: the name, the actions and the sections line up. */
    max-width: 48rem;
    padding: var(--space-4) var(--space-6) var(--space-12);
  }

  .back {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1_5);
    align-self: flex-start;
    min-height: var(--control-md);
    color: var(--color-link);
    font-size: var(--text-sm);
  }

  .back :global(svg) {
    width: var(--icon-md);
    height: var(--icon-md);
  }

  .head {
    display: flex;
    flex-wrap: wrap;
    align-items: flex-start;
    justify-content: space-between;
    gap: var(--space-3) var(--space-6);
    margin-top: calc(-1 * var(--space-2));
  }

  .identity {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    min-width: 0;
  }

  h1 {
    font-size: var(--text-xl);
  }

  .sub {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-1) var(--space-3);
    padding-inline-start: calc(3rem + var(--space-3));
  }

  .registered {
    padding-inline-start: calc(3rem + var(--space-3));
    color: var(--color-text-muted);
    font-size: var(--text-sm);
  }

  .mono {
    color: var(--color-text-muted);
    font-family: var(--font-mono);
    font-size: var(--text-sm);
    overflow-wrap: anywhere;
  }

  .muted {
    color: var(--color-text-muted);
  }

  .section {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }

  h2 {
    font-size: var(--text-lg);
  }

  .body {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }

  @media (width < 48rem) {
    .page {
      gap: var(--space-5);
      padding: var(--space-2) var(--space-4) var(--space-8);
    }

    .sub,
    .registered {
      padding-inline-start: 0;
    }
  }
</style>
