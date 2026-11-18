<script lang="ts">
  import { brand } from '#lib/i18n/messages/brand.ts';
  // The user directory (ACC-002): accounts by name, found by name, username or email and filtered
  // by state, each with what sets it apart, its groups and its projects (a few on phones, each
  // opening the project); a row opens the profile. People who manage accounts
  // create and invite them (ACC-003, ACC-004) and act on a selection: disable and enable (ACC-005),
  // delete (ACC-006), add to a group or remove from one (ACC-026). Rows wrap on phones, where the
  // selection's actions sit in a bar at the bottom of the screen within reach of a thumb (D-019).
  import Ban from '@lucide/svelte/icons/ban';
  import Ellipsis from '@lucide/svelte/icons/ellipsis';
  import Mail from '@lucide/svelte/icons/mail';
  import SearchX from '@lucide/svelte/icons/search-x';
  import Trash2 from '@lucide/svelte/icons/trash-2';
  import UserCheck from '@lucide/svelte/icons/user-check';
  import UserMinus from '@lucide/svelte/icons/user-minus';
  import UserPlus from '@lucide/svelte/icons/user-plus';
  import Users from '@lucide/svelte/icons/users';
  import X from '@lucide/svelte/icons/x';
  import type { LucideIcon } from '@lucide/svelte';
  import { MediaQuery } from 'svelte/reactivity';
  import { goto } from '$app/navigation';
  import { resolve } from '$app/paths';

  import type { Account, AccountSummary } from '#lib/api/schema.gen.ts';
  import {
    accessMessages,
    groupName,
    PersonName,
    personName,
    type AccountState,
  } from '#lib/features/access/index.ts';
  import { ProjectLogo, projectHref } from '#lib/features/projects/index.ts';
  import * as common from '#lib/i18n/messages/common.ts';
  import { can, canAnywhere, canSeeDirectory, currentUser } from '#lib/session/index.ts';
  import type { ListHandle } from '#lib/store/index.ts';
  import { NARROW } from '#lib/theme/breakpoints.ts';
  import Time from '#lib/time/Time.svelte';
  import Button from '#lib/ui/Button.svelte';
  import EmptyState from '#lib/ui/EmptyState.svelte';
  import ErrorState from '#lib/ui/ErrorState.svelte';
  import ForbiddenState from '#lib/ui/ForbiddenState.svelte';
  import IconButton from '#lib/ui/IconButton.svelte';
  import Menu, { type MenuEntry } from '#lib/ui/Menu.svelte';
  import PageHeader from '#lib/ui/PageHeader.svelte';
  import Select, { type SelectOption } from '#lib/ui/Select.svelte';
  import Spinner from '#lib/ui/Spinner.svelte';
  import TextField from '#lib/ui/TextField.svelte';
  import { toasts } from '#lib/ui/toast.svelte.ts';

  import * as admin from '../messages.ts';
  import AccountBadges from './AccountBadges.svelte';
  import CreateAccountDialog from './CreateAccountDialog.svelte';
  import DeleteDialog from './DeleteDialog.svelte';
  import DisableDialog from './DisableDialog.svelte';
  import EnableDialog from './EnableDialog.svelte';
  import GroupDialog from './GroupDialog.svelte';
  import InviteDialog from './InviteDialog.svelte';
  import * as m from './messages.ts';
  import { ensureDirectory, forgetAccounts, type DirectoryAccount } from './users.ts';

  interface Props {
    /** The whole directory, loaded by the route; the page loads it itself without one. */
    directory?: ListHandle<DirectoryAccount> | null;
    /** Outgoing email works on this server. */
    emailEnabled: boolean;
  }

  let { directory = null, emailEnabled }: Props = $props();

  const narrow = new MediaQuery(NARROW, false);
  /** Projects shown in a row; the rest are counted. */
  const projectLimit = $derived(narrow.current ? 2 : 4);

  // ---- The list for the filter ----

  let text = $state('');
  let shown = $state<string>('all');
  let loaded = $state<ListHandle<DirectoryAccount> | null>(null);
  let failure = $state<unknown>(null);
  let pending = $state(false);
  let request = 0;
  // The filter the list shows; not state: only the effect below reads it.
  let applied = { text: '', state: 'all' };

  const list = $derived(loaded ?? directory);
  const rows = $derived(list?.items ?? []);
  const states = $derived<SelectOption[]>([
    { value: 'all', label: m.stateAll() },
    { value: 'enabled', label: m.stateEnabled() },
    { value: 'disabled', label: m.stateDisabled() },
  ]);

  function asState(value: string): AccountState {
    return value === 'enabled' || value === 'disabled' ? value : 'all';
  }

  async function load(filter: { text: string; state: string }): Promise<void> {
    const mine = ++request;
    pending = true;
    failure = null;
    try {
      const handle = await ensureDirectory({ text: filter.text, state: asState(filter.state) });
      if (mine === request) loaded = handle;
    } catch (error) {
      if (mine === request) failure = error;
    } finally {
      if (mine === request) pending = false;
    }
  }

  // A new state shows at once; typed text a moment after typing stops.
  $effect(() => {
    const next = { text: text.trim(), state: shown };
    if (!canSeeDirectory()) return;
    if (!list && !failure && request === 0) {
      void load(next);
      applied = next;
      return;
    }
    if (next.text === applied.text && next.state === applied.state) return;
    const timer = setTimeout(
      () => {
        applied = next;
        void load(next);
      },
      next.text === applied.text ? 0 : 250,
    );
    return () => {
      clearTimeout(timer);
    };
  });

  async function retry(): Promise<void> {
    await load(applied);
  }

  // ---- What the viewer may do ----

  const me = $derived(currentUser()?.id ?? null);
  const mayCreate = $derived(can('user.create'));
  const mayUpdate = $derived(can('user.update'));
  const mayDelete = $derived(can('user.delete'));
  const mayGroup = $derived(can('system.write') || canAnywhere('project.update'));
  const maySelect = $derived(mayUpdate || mayDelete || mayGroup);

  // ---- Selection ----

  let selected = $state<number[]>([]);
  const chosen = $derived(rows.filter((row) => selected.includes(row.id)));
  const allChosen = $derived(rows.length > 0 && chosen.length === rows.length);

  function pick(id: number, on: boolean): void {
    selected = on
      ? [...selected.filter((other) => other !== id), id]
      : selected.filter((other) => other !== id);
  }

  function pickAll(on: boolean): void {
    selected = on ? rows.map((row) => row.id) : [];
  }

  // ---- Dialogs ----

  let creating = $state(false);
  let inviting = $state(false);
  let disabling = $state(false);
  let enabling = $state(false);
  let deleting = $state(false);
  let grouping = $state<'add' | 'remove'>('add');
  let groupOpen = $state(false);

  function openGroups(mode: 'add' | 'remove'): void {
    grouping = mode;
    groupOpen = true;
  }

  interface BulkAction {
    id: string;
    label: string;
    icon: LucideIcon;
    danger?: boolean;
    run: () => void;
  }

  const bulk = $derived.by((): BulkAction[] => {
    const actions: BulkAction[] = [];
    if (mayUpdate && chosen.some((row) => !row.disabled)) {
      actions.push({
        id: 'disable',
        label: m.disable(),
        icon: Ban,
        run: () => (disabling = true),
      });
    }
    if (mayUpdate && chosen.some((row) => row.disabled)) {
      actions.push({
        id: 'enable',
        label: m.enable(),
        icon: UserCheck,
        run: () => (enabling = true),
      });
    }
    if (mayGroup) {
      actions.push(
        { id: 'add', label: m.addToGroup(), icon: Users, run: () => openGroups('add') },
        {
          id: 'remove',
          label: m.removeFromGroup(),
          icon: UserMinus,
          run: () => openGroups('remove'),
        },
      );
    }
    if (mayDelete) {
      actions.push({
        id: 'delete',
        label: m.remove(),
        icon: Trash2,
        danger: true,
        run: () => (deleting = true),
      });
    }
    return actions;
  });

  const bulkMenu = $derived<MenuEntry[]>(
    bulk.map((action) => ({
      type: 'action',
      label: action.label,
      icon: action.icon,
      danger: action.danger,
      onSelect: action.run,
    })),
  );

  /** After a change: the first page again, and nothing selected. */
  async function changed(): Promise<void> {
    selected = [];
    await list?.refresh().catch(() => undefined);
  }

  async function deleted(ids: number[]): Promise<void> {
    forgetAccounts(ids);
    await changed();
  }

  async function created(account: Account): Promise<void> {
    toasts.show({ tone: 'success', message: m.accountCreated({ name: account.full_name }) });
    await goto(resolve('/(app)/users/[id]', { id: String(account.id) }));
  }

  function profileHref(account: AccountSummary): string {
    return resolve('/(app)/users/[id]', { id: String(account.id) });
  }
</script>

<svelte:head><title>{admin.users()} · {brand()}</title></svelte:head>

{#if !canSeeDirectory()}
  <ForbiddenState />
{:else}
  <PageHeader title={admin.users()}>
    {#snippet actions()}
      {#if mayCreate}
        <Button icon={Mail} onclick={() => (inviting = true)}>{m.invitePeople()}</Button>
        <Button variant="primary" icon={UserPlus} onclick={() => (creating = true)}>
          {m.createAccount()}
        </Button>
      {/if}
    {/snippet}
  </PageHeader>

  <div class={['page', { selecting: chosen.length > 0 }]}>
    <div class="filters">
      <TextField
        class="search"
        label={accessMessages.searchPeople()}
        type="search"
        hint={accessMessages.searchPeopleHint()}
        bind:value={text}
        autocomplete="off"
        autocapitalize="none"
        spellcheck="false"
      />
      <Select class="state" label={m.stateFilter()} options={states} bind:value={shown} />
      {#if pending && list}<span class="pending"><Spinner /></span>{/if}
    </div>

    {#if maySelect && chosen.length > 0}
      <div class="bulk" role="group" aria-label={m.selectionActions()}>
        <p class="count">{m.selectedCount({ count: chosen.length })}</p>
        <div class="bulk-actions">
          {#if narrow.current}
            <Menu
              label={m.selectionActions()}
              items={bulkMenu}
              placement={{ side: 'top', align: 'end' }}
            >
              {#snippet trigger(props)}
                <Button icon={Ellipsis} {...props}>{m.actions()}</Button>
              {/snippet}
            </Menu>
          {:else}
            {#each bulk as action (action.id)}
              <Button icon={action.icon} onclick={action.run}>{action.label}</Button>
            {/each}
          {/if}
          <IconButton icon={X} label={m.clearSelection()} onclick={() => (selected = [])} />
        </div>
      </div>
    {/if}

    {#if failure}
      <ErrorState error={failure} level={2} onretry={retry} />
    {:else if !list}
      <p class="loading"><Spinner label={common.loading()} /></p>
    {:else if list.error && rows.length === 0}
      <ErrorState error={list.error} level={2} onretry={() => list.refresh()} />
    {:else if rows.length === 0}
      <EmptyState icon={SearchX} title={m.nobodyFound()} description={m.nobodyFoundText()} />
    {:else}
      {#if maySelect}
        <label class="pick-all">
          <span class="box">
            <input
              type="checkbox"
              checked={allChosen}
              {@attach (input: HTMLInputElement) => {
                input.indeterminate = chosen.length > 0 && !allChosen;
              }}
              onchange={(event) => {
                pickAll(event.currentTarget.checked);
              }}
            />
          </span>
          {m.selectAllShown()}
        </label>
      {/if}
      <ul class="list" aria-busy={list.refreshing || pending}>
        {#each rows as account (account.id)}
          {@const picked = selected.includes(account.id)}
          {@const groups = account.groups.filter((group) => group.team_of === null)}
          <li class={['row', { picking: maySelect, picked }]}>
            {#if maySelect}
              <label class="pick">
                <input
                  type="checkbox"
                  checked={picked}
                  onchange={(event) => {
                    pick(account.id, event.currentTarget.checked);
                  }}
                />
                <span class="sr-only">{m.selectPerson({ name: personName(account) })}</span>
              </label>
            {/if}
            <div class="who">
              <PersonName person={account} withUsername href={profileHref(account)} />
              <AccountBadges {account} own={account.id === me} />
            </div>
            <div class="details">
              {#if account.email}<span class="email">{account.email}</span>{/if}
              {#if groups.length > 0}
                <ul class="chips" aria-label={m.groupsOf({ name: personName(account) })}>
                  {#each groups as group (group.id)}<li>{groupName(group)}</li>{/each}
                </ul>
              {/if}
              {#if account.projects.length > 0}
                <ul class="projects" aria-label={m.projectsOf({ name: personName(account) })}>
                  {#each account.projects.slice(0, projectLimit) as project (project.id)}
                    <li>
                      <!-- projectHref resolves the path. -->
                      <!-- eslint-disable-next-line svelte/no-navigation-without-resolve -->
                      <a class="project" href={projectHref.overview(project)}>
                        <ProjectLogo
                          projectKey={project.key}
                          color={project.logo_color}
                          src={project.logo_url}
                          size="sm"
                        />
                        <span>{project.name}</span>
                      </a>
                    </li>
                  {/each}
                  {#if account.projects.length > projectLimit}
                    <li class="more">
                      {m.moreProjects({ count: account.projects.length - projectLimit })}
                    </li>
                  {/if}
                </ul>
              {/if}
            </div>
            <p class="date">
              {m.registeredPrefix()}
              <Time value={account.created_at} kind="date" />
            </p>
          </li>
        {/each}
      </ul>
      {#if list.hasMore}
        <div>
          <Button loading={list.loadingMore} onclick={() => list.loadMore()}>
            {accessMessages.showMore()}
          </Button>
        </div>
      {/if}
    {/if}
  </div>

  <CreateAccountDialog bind:open={creating} oncreated={created} />
  <InviteDialog bind:open={inviting} {emailEnabled} oninvited={changed} />
  <DisableDialog bind:open={disabling} accounts={chosen} ondone={changed} />
  <EnableDialog
    bind:open={enabling}
    accounts={chosen.filter((row) => row.disabled)}
    ondone={changed}
  />
  <DeleteDialog bind:open={deleting} accounts={chosen} ondone={deleted} />
  <GroupDialog bind:open={groupOpen} mode={grouping} accounts={chosen} ondone={changed} />
{/if}

<style>
  .page {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
    padding: var(--space-2) var(--space-6) var(--space-12);
  }

  .filters {
    display: flex;
    flex-wrap: wrap;
    align-items: flex-start;
    gap: var(--space-3) var(--space-4);
  }

  .filters :global(.search) {
    flex: 1 1 18rem;
    max-width: 32rem;
  }

  .filters :global(.state) {
    flex: 0 1 12rem;
  }

  .pending {
    align-self: center;
    color: var(--color-text-muted);
  }

  .loading {
    display: flex;
    justify-content: center;
    padding: var(--space-12) 0;
    color: var(--color-text-muted);
  }

  /* The selection's actions stay in view while the list scrolls. */
  .bulk {
    position: sticky;
    top: 0;
    z-index: var(--z-sticky);
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2) var(--space-4);
    padding: var(--space-2) var(--space-2) var(--space-2) var(--space-4);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-lg);
    background: var(--color-surface-raised);
    box-shadow: var(--shadow-sm);
  }

  .count {
    font-weight: var(--weight-medium);
    font-variant-numeric: tabular-nums;
  }

  .bulk-actions {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-2);
  }

  /* Lined up with the checkboxes of the rows below. */
  .pick-all {
    display: inline-flex;
    align-items: center;
    gap: var(--space-3);
    align-self: flex-start;
    min-height: var(--control-md);
    padding-inline: calc(var(--space-2) + 1px) var(--space-3);
    color: var(--color-text-muted);
    cursor: pointer;
  }

  .box {
    display: inline-flex;
    justify-content: center;
    min-width: var(--control-md);
  }

  input[type='checkbox'] {
    width: 1rem;
    height: 1rem;
    margin: 0;
    accent-color: var(--color-ink);
  }

  .list {
    margin: 0;
    padding: 0;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-lg);
    background: var(--color-surface);
    list-style: none;
  }

  .row {
    position: relative;
    display: grid;
    grid-template-columns: minmax(14rem, 2fr) minmax(0, 3fr) auto;
    align-items: center;
    gap: var(--space-1) var(--space-4);
    padding: var(--space-2) var(--space-4);
  }

  .row.picking {
    grid-template-columns: auto minmax(14rem, 2fr) minmax(0, 3fr) auto;
    padding-inline-start: var(--space-2);
  }

  .row + .row {
    border-top: 1px solid var(--color-border);
  }

  .row:hover {
    background: var(--color-hover);
  }

  /* The selected row has a bar at its edge as well as its checked box. */
  .row.picked {
    box-shadow: inset 3px 0 0 var(--color-link);
  }

  /* The whole row opens the profile; the checkbox stays its own target. */
  .who :global(a.name)::after {
    content: '';
    position: absolute;
    inset: 0;
  }

  .row:has(:global(a.name:focus-visible)) {
    outline: var(--focus-width) solid var(--color-focus);
    outline-offset: calc(-1 * var(--focus-width));
  }

  .row :global(a.name:focus-visible) {
    outline: none;
  }

  .pick {
    position: relative;
    z-index: 1;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-width: var(--control-md);
    min-height: var(--control-md);
    cursor: pointer;
  }

  .who {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-1) var(--space-3);
    min-width: 0;
  }

  .details {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-1) var(--space-3);
    min-width: 0;
    font-size: var(--text-sm);
  }

  .email {
    color: var(--color-text-muted);
    overflow-wrap: anywhere;
  }

  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-1);
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .chips li {
    padding: var(--space-0_5) var(--space-2);
    border-radius: var(--radius-full);
    background: var(--color-surface-sunken);
    color: var(--color-text-muted);
    font-size: var(--text-xs);
    white-space: nowrap;
  }

  .projects {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-1);
    margin: 0;
    padding: 0;
    list-style: none;
  }

  /* Above the link that makes the whole row open the profile. */
  .project {
    position: relative;
    z-index: 1;
    display: inline-flex;
    align-items: center;
    gap: var(--space-1_5);
    min-height: var(--control-md);
    padding: 0 var(--space-2) 0 var(--space-1);
    border-radius: var(--radius-md);
    color: var(--color-text);
    font-size: var(--text-xs);
    font-weight: var(--weight-medium);
    text-decoration: none;
  }

  .project:hover {
    background: var(--color-selected);
    text-decoration: underline;
  }

  .more {
    color: var(--color-text-muted);
    font-size: var(--text-xs);
    white-space: nowrap;
  }

  .date {
    color: var(--color-text-muted);
    font-size: var(--text-sm);
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }

  @media (pointer: coarse) {
    input[type='checkbox'] {
      width: 1.25rem;
      height: 1.25rem;
    }

    /* The name link is a full-size target on touch screens. */
    .who :global(a.name) {
      display: inline-flex;
      align-items: center;
      min-width: var(--control-md);
      min-height: var(--control-md);
    }
  }

  @media (width < 48rem) {
    .page {
      padding: var(--space-2) var(--space-4) var(--space-8);
    }

    /* Room for the bar at the bottom while something is selected. */
    .page.selecting {
      padding-bottom: calc(var(--space-16) + var(--safe-bottom));
    }

    .bulk {
      position: fixed;
      inset: auto 0 0;
      flex-wrap: nowrap;
      padding: var(--space-2) calc(var(--space-2) + var(--safe-right))
        calc(var(--space-2) + var(--safe-bottom)) calc(var(--space-4) + var(--safe-left));
      border-width: 1px 0 0;
      border-radius: 0;
      box-shadow: var(--shadow-md);
    }

    .bulk-actions {
      flex-wrap: nowrap;
    }

    .row,
    .row.picking {
      grid-template-columns: minmax(0, 1fr);
      align-items: start;
    }

    .row.picking {
      grid-template-columns: auto minmax(0, 1fr);
      column-gap: var(--space-2);
    }

    .picking .pick {
      grid-row: span 3;
    }

    .picking > :not(.pick) {
      grid-column: 2;
    }
  }

  @media (forced-colors: active) {
    .chips li {
      border: 1px solid CanvasText;
    }
  }
</style>
