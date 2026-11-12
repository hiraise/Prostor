<script lang="ts">
  // Giving a person a role (ACC-059): the role, then where it applies — globally (for people who
  // may change system settings) or in projects they manage, several at once. In projects only roles
  // that give something there are offered (ACC-044, PRJ-052). Passing on a permission one lacks is
  // refused, and the refusal names it (ACC-050).
  import { untrack } from 'svelte';

  import type { PermissionKey, Role } from '#lib/api/schema.gen.ts';
  import {
    allRoles,
    globalOnlyPermissions,
    personName,
    predefinedRoles,
    roleName,
    worksInProjects,
  } from '#lib/features/access/index.ts';
  import type { ProjectChoice } from '#lib/features/projects/index.ts';
  import * as common from '#lib/i18n/messages/common.ts';
  import Button from '#lib/ui/Button.svelte';
  import Dialog from '#lib/ui/Dialog.svelte';
  import Notice from '#lib/ui/Notice.svelte';
  import Select, { type SelectOption } from '#lib/ui/Select.svelte';
  import Spinner from '#lib/ui/Spinner.svelte';
  import { toasts } from '#lib/ui/toast.svelte.ts';

  import * as m from './messages.ts';
  import { refusalText } from './problems.ts';
  import ScopeField from './ScopeField.svelte';
  import { assignRole, type DirectoryAccount, type ScopeKind } from './users.ts';

  interface Props {
    open?: boolean;
    account: DirectoryAccount;
    /** Roles the person holds directly everywhere already; they are not offered globally. */
    held: readonly number[];
    onassigned: () => unknown;
  }

  let { open = $bindable(false), account, held, onassigned }: Props = $props();

  const formId = $props.id();
  let roles = $state<Role[] | null>(null);
  let globalOnly = $state.raw<ReadonlySet<PermissionKey> | null>(null);
  let chosen = $state('');
  let scope = $state<ScopeKind>('global');
  let projects = $state<ProjectChoice[]>([]);
  let error = $state<string | null>(null);
  let projectsError = $state<string | null>(null);
  let problem = $state<string | null>(null);
  let busy = $state(false);
  let opened = 0;

  const offered = $derived(
    (roles ?? []).filter((role) =>
      scope === 'global'
        ? !held.includes(role.id)
        : globalOnly !== null && worksInProjects(role, globalOnly),
    ),
  );
  const options = $derived<SelectOption[]>([
    { value: '', label: m.chooseRolePlaceholder() },
    ...offered.map((role) => ({ value: String(role.id), label: roleName(role) })),
  ]);
  const role = $derived(offered.find((item) => String(item.id) === chosen) ?? null);
  const description = $derived(
    role ? (role.builtin ? predefinedRoles[role.builtin].description() : role.description) : '',
  );

  $effect(() => {
    if (!open) return;
    const mine = ++opened;
    untrack(() => {
      chosen = '';
      scope = 'global';
      projects = [];
      error = null;
      projectsError = null;
      problem = null;
    });
    Promise.all([allRoles(), globalOnlyPermissions()]).then(
      ([found, keys]) => {
        if (mine !== opened) return;
        roles = found;
        globalOnly = keys;
      },
      (failure: unknown) => {
        if (mine === opened) problem = refusalText(failure);
      },
    );
  });

  async function submit(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    problem = null;
    error = role ? null : m.chooseRole();
    projectsError = scope === 'projects' && projects.length === 0 ? m.chooseProjects() : null;
    if (!role || projectsError) return;
    busy = true;
    try {
      await assignRole(
        { user_id: account.id },
        role.id,
        scope === 'global' ? { global: true } : { projects: projects.map((project) => project.id) },
      );
      open = false;
      toasts.show({ tone: 'success', message: m.roleAssigned() });
      await onassigned();
    } catch (failure) {
      problem = refusalText(failure);
    } finally {
      busy = false;
    }
  }
</script>

<Dialog bind:open title={m.assignRoleTitle({ name: personName(account) })} dismissible={!busy}>
  <form id={formId} class="form" novalidate onsubmit={submit}>
    {#if problem}<Notice tone="danger">{problem}</Notice>{/if}
    {#if roles === null && !problem}
      <p class="muted"><Spinner label={common.loading()} /></p>
    {:else}
      <Select
        label={m.role()}
        {options}
        bind:value={chosen}
        hint={description || (scope === 'projects' ? m.projectRolesOnly() : undefined)}
        {error}
        required
      />
    {/if}
    <ScopeField bind:scope bind:projects error={projectsError} />
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
      {m.assignRole()}
    </Button>
  {/snippet}
</Dialog>

<style>
  .form {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }

  .muted {
    color: var(--color-text-muted);
    font-size: var(--text-sm);
  }
</style>
