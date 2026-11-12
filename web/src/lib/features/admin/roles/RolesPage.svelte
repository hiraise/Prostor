<script lang="ts">
  // The roles list (ACC-052, ACC-053): predefined roles first and marked, each with its
  // description, permission count and assignments; search by name and by permission. Each row is
  // one link to the role's page, so the whole row is the target on phones (D-019).
  import Lock from '@lucide/svelte/icons/lock';
  import Plus from '@lucide/svelte/icons/plus';
  import SearchX from '@lucide/svelte/icons/search-x';
  import { untrack } from 'svelte';
  import { resolve } from '$app/paths';

  import type { PermissionInfo, Role } from '#lib/api/schema.gen.ts';
  import { permissions, predefinedRoles, roleName } from '#lib/features/access/index.ts';
  import { can, canSeeRoles } from '#lib/session/index.ts';
  import type { ListHandle } from '#lib/store/index.ts';
  import Button from '#lib/ui/Button.svelte';
  import EmptyState from '#lib/ui/EmptyState.svelte';
  import ErrorState from '#lib/ui/ErrorState.svelte';
  import ForbiddenState from '#lib/ui/ForbiddenState.svelte';
  import PageHeader from '#lib/ui/PageHeader.svelte';
  import Select, { type SelectOption } from '#lib/ui/Select.svelte';
  import TextField from '#lib/ui/TextField.svelte';
  import { toasts } from '#lib/ui/toast.svelte.ts';

  import Badge from './Badge.svelte';
  import CreateRoleDialog from './CreateRoleDialog.svelte';
  import * as m from './messages.ts';
  import { ensureRoles, type RoleFilters } from './roles.ts';

  interface Props {
    /** The list for `filters`; `null` when the viewer may not see roles. */
    roles: ListHandle<Role> | null;
    catalog: readonly PermissionInfo[];
    filters?: RoleFilters;
    /** A role was created: open its page to choose permissions. */
    oncreated: (role: Role) => unknown;
    /** The filters changed: keep them in the address (UI-006). */
    onfilter?: (filters: RoleFilters) => void;
    /** Where "forbidden" offers to go. */
    home?: string;
  }

  let {
    roles,
    catalog,
    filters = { q: '', permission: '' },
    oncreated,
    onfilter,
    home,
  }: Props = $props();

  /** Typing pauses this long before the list is searched. */
  const DEBOUNCE_MS = 250;

  let text = $state(untrack(() => filters.q));
  let permission = $state<string>(untrack(() => filters.permission));
  let list = $state.raw(untrack(() => roles));
  let searching = $state(false);
  let searchError = $state<unknown>(null);
  let creating = $state(false);
  let timer: ReturnType<typeof setTimeout> | undefined;
  let request = 0;

  const options = $derived<SelectOption[]>([
    { value: '', label: m.anyPermission() },
    ...catalog.map((info) => ({ value: info.key, label: permissions[info.key].name() })),
  ]);
  const mayCreate = $derived(can('system.write'));

  function current(): RoleFilters {
    const key = catalog.find((info) => info.key === permission)?.key;
    return { q: text.trim(), permission: key ?? '' };
  }

  async function search(): Promise<void> {
    clearTimeout(timer);
    const wanted = current();
    onfilter?.(wanted);
    const mine = ++request;
    searching = true;
    try {
      const found = await ensureRoles(wanted);
      if (mine !== request) return;
      list = found;
      searchError = null;
    } catch (error) {
      if (mine === request) searchError = error;
    } finally {
      if (mine === request) searching = false;
    }
  }

  function typed(): void {
    clearTimeout(timer);
    timer = setTimeout(() => void search(), DEBOUNCE_MS);
  }

  $effect(() => () => {
    clearTimeout(timer);
  });

  function describe(role: Role): string {
    return role.builtin ? predefinedRoles[role.builtin].description() : role.description;
  }

  async function created(role: Role): Promise<void> {
    toasts.show({ tone: 'success', message: m.roleCreated() });
    void list?.refresh().catch(() => undefined);
    await oncreated(role);
  }
</script>

<svelte:head><title>{m.rolesTitle()} · trackflow</title></svelte:head>

{#if !roles || !canSeeRoles()}
  <ForbiddenState {home} />
{:else}
  <PageHeader title={m.rolesTitle()}>
    {#snippet actions()}
      {#if mayCreate}
        <Button
          variant="primary"
          icon={Plus}
          onclick={() => {
            creating = true;
          }}
        >
          {m.createRole()}
        </Button>
      {/if}
    {/snippet}
  </PageHeader>

  <div class="page">
    <p class="intro">{m.rolesIntro()}</p>

    <search class="filters">
      <TextField
        class="find"
        type="search"
        label={m.findRoles()}
        hint={m.findRolesHint()}
        bind:value={text}
        oninput={typed}
        onkeydown={(event: KeyboardEvent) => {
          if (event.key === 'Enter') void search();
        }}
        autocomplete="off"
        maxlength={100}
      />
      <Select
        class="permission"
        label={m.withPermission()}
        {options}
        bind:value={permission}
        onchange={() => void search()}
      />
    </search>

    {#if searchError && !searching}
      <ErrorState error={searchError} level={2} onretry={search} />
    {:else if list}
      {#if list.error && list.ids.length === 0}
        <ErrorState error={list.error} level={2} onretry={() => list?.refresh()} />
      {:else if list.ids.length === 0}
        <EmptyState icon={SearchX} title={m.noRolesFound()} description={m.noRolesFoundText()} />
      {:else}
        <ul class="list" aria-busy={searching || list.refreshing}>
          {#each list.items as role (role.id)}
            {@const description = describe(role)}
            <li class="role">
              <span class="title">
                <a
                  class="name"
                  href={resolve('/(app)/admin/roles/[id]', { id: String(role.id) })}
                  aria-describedby="role-{role.id}-details"
                >
                  {roleName(role)}
                </a>
                {#if role.builtin}<Badge icon={Lock} text={m.predefined()} />{/if}
              </span>
              <span id="role-{role.id}-details" class="details">
                {#if role.builtin}<span class="sr-only">{m.predefined()}.</span>{/if}
                {#if description}<span class="description">{description}</span>{/if}
                <span class="counts">
                  {m.permissionCount({ count: role.permissions.length })} ·
                  {m.assignmentCount({ count: role.assignments })}
                </span>
              </span>
            </li>
          {/each}
        </ul>
        {#if list.hasMore}
          <div>
            <Button loading={list.loadingMore} onclick={() => list?.loadMore()}>
              {m.showMore()}
            </Button>
          </div>
        {/if}
      {/if}
    {/if}
  </div>

  <CreateRoleDialog bind:open={creating} oncreated={created} />
{/if}

<style>
  .page {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
    padding: var(--space-2) var(--space-6) var(--space-12);
  }

  .intro {
    max-width: 44rem;
    color: var(--color-text-muted);
  }

  .filters {
    display: flex;
    flex-wrap: wrap;
    align-items: flex-start;
    gap: var(--space-3) var(--space-4);
  }

  .filters :global(.find) {
    flex: 1 1 16rem;
    max-width: 24rem;
  }

  .filters :global(.permission) {
    flex: 1 1 14rem;
    max-width: 20rem;
  }

  .list {
    margin: 0;
    padding: 0;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-lg);
    background: var(--color-surface);
    list-style: none;
  }

  .role + .role {
    border-top: 1px solid var(--color-border);
  }

  /* The name is the link; it covers the whole row, so the row is the target (D-019). */
  .role {
    position: relative;
    display: flex;
    flex-direction: column;
    gap: var(--space-0_5);
    padding: var(--space-2) var(--space-4) var(--space-3);
  }

  .role:first-child {
    border-start-start-radius: var(--radius-lg);
    border-start-end-radius: var(--radius-lg);
  }

  .role:last-child {
    border-end-start-radius: var(--radius-lg);
    border-end-end-radius: var(--radius-lg);
  }

  .role:hover {
    background: var(--color-hover);
  }

  .title {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-1) var(--space-2);
  }

  .name {
    display: inline-flex;
    align-items: center;
    min-width: var(--control-md);
    min-height: var(--control-md);
    color: inherit;
    font-weight: var(--weight-medium);
    text-decoration: none;
    overflow-wrap: anywhere;
  }

  .name::after {
    content: '';
    position: absolute;
    inset: 0;
  }

  .role:hover .name {
    color: var(--color-link);
    text-decoration: underline;
  }

  .details {
    display: flex;
    flex-direction: column;
    gap: var(--space-0_5);
    color: var(--color-text-muted);
    font-size: var(--text-sm);
  }

  .description {
    max-width: 48rem;
    overflow-wrap: anywhere;
  }

  .counts {
    font-variant-numeric: tabular-nums;
  }

  @media (width < 48rem) {
    .page {
      padding: var(--space-2) var(--space-4) var(--space-8);
    }

    .filters :global(.find),
    .filters :global(.permission) {
      max-width: none;
    }
  }
</style>
