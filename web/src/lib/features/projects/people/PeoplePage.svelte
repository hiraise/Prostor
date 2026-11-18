<script lang="ts">
  // The people of a project (PRJ-046…PRJ-056, ACC-033): who sees it (PRJ-056), the owner and the
  // transfer of ownership (PRJ-053), the team with its roles, groups and accounts (PRJ-047,
  // PRJ-050, PRJ-051, PRJ-054), roles held outside the team (PRJ-052), and global roles that reach
  // the project, read-only. People with `project.update` change it; an archived project is
  // read-only for everyone. Sections stack on every width, so the page reads the same on a phone.
  import ShieldMinus from '@lucide/svelte/icons/shield-minus';
  import ShieldPlus from '@lucide/svelte/icons/shield-plus';
  import UserMinus from '@lucide/svelte/icons/user-minus';
  import UserPlus from '@lucide/svelte/icons/user-plus';
  import UserRoundCog from '@lucide/svelte/icons/user-round-cog';
  import { untrack } from 'svelte';
  import { resolve } from '$app/paths';

  import type {
    Assignment,
    GroupRef,
    PermissionKey,
    Project,
    ProjectPeople,
    Role,
    UserRef,
  } from '#lib/api/schema.gen.ts';
  import {
    PersonName,
    allRoles,
    builtinGroups,
    globalOnlyPermissions,
    groupName,
    personName,
    roleName,
  } from '#lib/features/access/index.ts';
  import { can, canSeeDirectory, canSeeGroups } from '#lib/session/index.ts';
  import Button from '#lib/ui/Button.svelte';
  import ErrorState from '#lib/ui/ErrorState.svelte';
  import IconButton from '#lib/ui/IconButton.svelte';
  import Notice from '#lib/ui/Notice.svelte';
  import PageHeader from '#lib/ui/PageHeader.svelte';
  import Spinner from '#lib/ui/Spinner.svelte';
  import { toasts } from '#lib/ui/toast.svelte.ts';

  import * as pm from '../messages.ts';
  import { fetchPeople, removeFromTeam, revokeProjectRole } from '../projects.ts';
  import AddPeopleDialog from './AddPeopleDialog.svelte';
  import AddTeamRoleDialog from './AddTeamRoleDialog.svelte';
  import Confirm from './Confirm.svelte';
  import * as m from './messages.ts';
  import { audienceOf, holderName, otherRoles, teamRoles, type AddMode } from './people.ts';
  import TeamPeople from './TeamPeople.svelte';
  import TransferDialog from './TransferDialog.svelte';

  let { project }: { project: Project } = $props();

  const uid = $props.id();

  let people = $state<ProjectPeople | null>(null);
  let failure = $state<unknown>(null);
  let roles = $state<Role[] | null>(null);
  let globalOnly = $state.raw<ReadonlySet<PermissionKey> | null>(null);
  let revision = $state(0);
  let directMembers = $state<number[]>([]);
  let request = 0;

  /** Changes are for people who manage the project, and never while it is archived. */
  const mayChange = $derived(can('project.update', project.id) && project.archived_at === null);
  const readOnly = $derived(!can('project.update', project.id));
  const team = $derived(people ? teamRoles(people) : []);
  const others = $derived(people ? otherRoles(people) : []);
  const audience = $derived(people && roles ? audienceOf(people, roles) : null);

  async function load(id: number): Promise<void> {
    const mine = ++request;
    failure = null;
    try {
      const found = await fetchPeople(id);
      if (mine === request) people = found;
    } catch (error) {
      if (mine === request) failure = error;
    }
  }

  $effect(() => {
    const id = project.id;
    untrack(() => {
      people = null;
      void load(id);
    });
  });

  // Roles name the team's roles, tell which of them open the project, and fill the pickers.
  $effect(() => {
    allRoles().then(
      (found) => {
        roles = found;
      },
      () => undefined,
    );
    globalOnlyPermissions().then(
      (found) => {
        globalOnly = found;
      },
      () => undefined,
    );
  });

  /** After a change: everything loads again. */
  async function changed(): Promise<void> {
    revision += 1;
    await load(project.id);
  }

  // ---- Dialogs ----

  let adding = $state(false);
  let addMode = $state<AddMode>('team');
  let addingRole = $state(false);
  let transferring = $state(false);

  function add(mode: AddMode): void {
    addMode = mode;
    adding = true;
  }

  interface Pending {
    title: string;
    text: string;
    action: string;
    run: () => Promise<unknown>;
  }

  let pending = $state<Pending | null>(null);
  let confirming = $state(false);

  function ask(next: Pending): void {
    pending = next;
    confirming = true;
  }

  function removeTeamRole(assignment: Assignment): void {
    const role = roleName(assignment.role);
    ask({
      title: m.removeTeamRoleTitle({ role }),
      text: m.removeTeamRoleText(),
      action: m.removeRole(),
      run: async () => {
        await revokeProjectRole(assignment.id);
        toasts.show({ tone: 'success', message: m.teamRoleRemoved() });
      },
    });
  }

  function removeGroup(group: GroupRef): void {
    const name = groupName(group);
    ask({
      title: m.removeGroupTitle({ group: name }),
      text: m.removeGroupText({ group: name }),
      action: m.removeFromTeam(),
      run: async () => {
        await removeFromTeam(project.id, { groups: [group.id] });
        toasts.show({ tone: 'success', message: m.leftTeam({ name }) });
      },
    });
  }

  function removePerson(person: UserRef): void {
    const name = personName(person);
    ask({
      title: m.removePersonTitle({ name }),
      text: m.removePersonText(),
      action: m.removeFromTeam(),
      run: async () => {
        await removeFromTeam(project.id, { users: [person.id] });
        toasts.show({ tone: 'success', message: m.leftTeam({ name }) });
      },
    });
  }

  function revoke(assignment: Assignment): void {
    const holder = holderName(assignment);
    ask({
      title: m.revokeTitle({ role: roleName(assignment.role) }),
      text: m.revokeText({ holder }),
      action: m.revoke(),
      run: async () => {
        await revokeProjectRole(assignment.id);
        toasts.show({ tone: 'success', message: m.roleRevoked() });
      },
    });
  }

  // ---- Links ----

  function personHref(person: UserRef): string | undefined {
    if (!canSeeDirectory() || person.kind !== 'person') return undefined;
    return resolve('/(app)/users/[id]', { id: String(person.id) });
  }

  function groupHref(group: GroupRef): string | undefined {
    if (!canSeeGroups()) return undefined;
    return resolve('/(app)/admin/groups/[id]', { id: String(group.id) });
  }
</script>

<svelte:head><title>{pm.people()} · {project.name} · TaskMaster</title></svelte:head>

{#snippet holder(assignment: Assignment)}
  {#if assignment.holder.kind === 'user'}
    <PersonName person={assignment.holder.user} href={personHref(assignment.holder.user)} />
  {:else}
    {@const href = groupHref(assignment.holder.group)}
    <span class="group-name">
      {#if href}
        <!-- groupHref resolves the path. -->
        <!-- eslint-disable-next-line svelte/no-navigation-without-resolve -->
        <a {href}>{groupName(assignment.holder.group)}</a>
      {:else}
        {groupName(assignment.holder.group)}
      {/if}
      <span class="badge">{m.groupBadge()}</span>
    </span>
  {/if}
{/snippet}

<PageHeader title={pm.people()}>
  {#snippet actions()}
    {#if mayChange && people}
      <Button
        variant="primary"
        icon={UserPlus}
        onclick={() => {
          add('team');
        }}
      >
        {m.addPeople()}
      </Button>
    {/if}
  {/snippet}
</PageHeader>

<div class="page">
  {#if readOnly}<p class="muted">{m.readOnly()}</p>{/if}

  {#if failure && !people}
    <ErrorState error={failure} level={2} onretry={() => load(project.id)} />
  {:else if !people}
    <p class="loading"><Spinner label={m.loadingPeople()} /></p>
  {:else}
    {#if audience}
      {#if audience.kind === 'private'}
        <Notice>{m.privateProject()}</Notice>
      {:else if audience.kind === 'members'}
        <Notice tone="warning">{m.membersSee({ group: groupName(audience.group) })}</Notice>
      {:else}
        <Notice tone="warning">{m.everyoneSees({ group: groupName(audience.group) })}</Notice>
      {/if}
    {/if}

    <section class="section" aria-labelledby="{uid}-owner">
      <div class="head">
        <h2 id="{uid}-owner">{m.ownerTitle()}</h2>
        {#if mayChange}
          <Button
            icon={UserRoundCog}
            onclick={() => {
              transferring = true;
            }}
          >
            {m.transferOwnership()}
          </Button>
        {/if}
      </div>
      <p class="hint">{m.ownerIntro()}</p>
      {#if people.owner}
        <div class="owner">
          <PersonName
            person={people.owner}
            size="md"
            withUsername
            href={personHref(people.owner)}
          />
        </div>
      {:else}
        <p class="muted">{m.noOwner()}</p>
      {/if}
    </section>

    <section class="section" aria-labelledby="{uid}-team">
      <div class="head">
        <h2 id="{uid}-team">
          {m.teamTitle()} <span class="size">· {m.teamSize({ count: people.team_size })}</span>
        </h2>
      </div>
      <p class="hint">{m.teamIntro()}</p>
      <p class="hint">{m.assigneesNote()}</p>

      <section class="part" aria-labelledby="{uid}-roles">
        <div class="head">
          <h3 id="{uid}-roles">{m.teamRolesTitle()}</h3>
          {#if mayChange}
            <Button
              size="sm"
              icon={ShieldPlus}
              onclick={() => {
                addingRole = true;
              }}
            >
              {m.addTeamRole()}
            </Button>
          {/if}
        </div>
        <p class="hint">{m.teamRolesIntro()}</p>
        {#if team.length === 0}
          <p class="muted">{m.noTeamRoles()}</p>
        {:else}
          <ul class="chips" aria-label={m.teamRoleList()}>
            {#each team as assignment (assignment.id)}
              {@const name = roleName(assignment.role)}
              <li class="chip">
                <span class="chip-text">{name}</span>
                {#if mayChange}
                  <IconButton
                    icon={ShieldMinus}
                    size="sm"
                    label={m.removeTeamRoleLabel({ role: name })}
                    onclick={() => {
                      removeTeamRole(assignment);
                    }}
                  />
                {/if}
              </li>
            {/each}
          </ul>
        {/if}
      </section>

      <section class="part" aria-labelledby="{uid}-groups">
        <h3 id="{uid}-groups">{m.teamGroupsTitle()}</h3>
        {#if people.team_groups.length === 0}
          <p class="muted">{m.noTeamGroups()}</p>
        {:else}
          <p class="hint">{m.teamGroupsIntro()}</p>
          <ul class="list">
            {#each people.team_groups as group (group.id)}
              {@const href = groupHref(group)}
              {@const name = groupName(group)}
              <li class="row">
                <span class="row-text">
                  {#if href}
                    <!-- groupHref resolves the path. -->
                    <!-- eslint-disable-next-line svelte/no-navigation-without-resolve -->
                    <a class="name" {href}>{name}</a>
                  {:else}
                    <span class="name">{name}</span>
                  {/if}
                  {#if group.builtin}
                    <span class="about">{builtinGroups[group.builtin].description()}</span>
                  {/if}
                </span>
                {#if mayChange}
                  <IconButton
                    icon={UserMinus}
                    label={m.removeGroupLabel({ group: name })}
                    onclick={() => {
                      removeGroup(group);
                    }}
                  />
                {/if}
              </li>
            {/each}
          </ul>
        {/if}
      </section>

      <TeamPeople
        projectId={project.id}
        {revision}
        {mayChange}
        onmembers={(ids: number[]) => {
          directMembers = ids;
        }}
        onremove={removePerson}
      />
    </section>

    <section class="section" aria-labelledby="{uid}-other">
      <div class="head">
        <h2 id="{uid}-other">{m.otherTitle()}</h2>
        {#if mayChange}
          <Button
            icon={ShieldPlus}
            onclick={() => {
              add('role');
            }}
          >
            {m.assignRole()}
          </Button>
        {/if}
      </div>
      <p class="hint">{m.otherIntro()}</p>
      {#if others.length === 0}
        <p class="muted">{m.noOther()}</p>
      {:else}
        <ul class="list" aria-label={m.otherList()}>
          {#each others as assignment (assignment.id)}
            {@const role = roleName(assignment.role)}
            <li class="row">
              <span class="row-text">
                {@render holder(assignment)}
                <span class="role">{role}</span>
              </span>
              {#if mayChange}
                <IconButton
                  icon={ShieldMinus}
                  label={m.revokeLabel({ role, holder: holderName(assignment) })}
                  onclick={() => {
                    revoke(assignment);
                  }}
                />
              {/if}
            </li>
          {/each}
        </ul>
      {/if}
    </section>

    <section class="section" aria-labelledby="{uid}-global">
      <h2 id="{uid}-global">{m.globalTitle()}</h2>
      <p class="hint">{m.globalIntro()}</p>
      {#if people.global.length === 0}
        <p class="muted">{m.noGlobal()}</p>
      {:else}
        <ul class="list" aria-label={m.globalList()}>
          {#each people.global as assignment (assignment.id)}
            <li class="row">
              <span class="row-text">
                {@render holder(assignment)}
                <span class="role">{roleName(assignment.role)}</span>
              </span>
            </li>
          {/each}
        </ul>
      {/if}
    </section>
  {/if}
</div>

{#if people && mayChange}
  <AddPeopleDialog
    bind:open={adding}
    {project}
    {people}
    {roles}
    {globalOnly}
    mode={addMode}
    members={directMembers}
    ondone={changed}
  />
  <AddTeamRoleDialog
    bind:open={addingRole}
    {project}
    {people}
    {roles}
    {globalOnly}
    ondone={changed}
  />
  <TransferDialog bind:open={transferring} {project} owner={people.owner} ondone={changed} />
{/if}
{#if pending}
  <Confirm
    bind:open={confirming}
    title={pending.title}
    text={pending.text}
    action={pending.action}
    run={pending.run}
    ondone={changed}
  />
{/if}

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
    gap: var(--space-3);
  }

  .part {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    margin-top: var(--space-2);
  }

  .head {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2) var(--space-4);
  }

  h2 {
    font-size: var(--text-lg);
  }

  h3 {
    font-size: var(--text-md);
    font-weight: var(--weight-semibold);
  }

  .size {
    color: var(--color-text-muted);
    font-weight: var(--weight-regular);
    font-variant-numeric: tabular-nums;
  }

  .hint,
  .muted {
    max-width: 44rem;
    color: var(--color-text-muted);
    font-size: var(--text-sm);
  }

  .loading {
    display: flex;
    justify-content: center;
    padding: var(--space-12) 0;
    color: var(--color-text-muted);
  }

  .owner {
    display: flex;
    align-items: center;
    min-height: var(--control-md);
  }

  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .chip {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
    min-height: var(--control-md);
    padding: 0 var(--space-0_5) 0 var(--space-3);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-full);
    background: var(--color-surface);
    font-weight: var(--weight-medium);
  }

  /* A chip without a button keeps its padding even. */
  .chip:not(:has(button)) {
    padding-inline-end: var(--space-3);
  }

  .chip-text {
    overflow-wrap: anywhere;
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
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3);
    min-height: calc(var(--control-md) + var(--space-2));
    padding: var(--space-1) var(--space-2) var(--space-1) var(--space-4);
  }

  .row + .row {
    border-top: 1px solid var(--color-border);
  }

  .row-text {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-1) var(--space-3);
    min-width: 0;
  }

  .name {
    font-weight: var(--weight-medium);
    overflow-wrap: anywhere;
  }

  /* Names that open a page look like the names of people in lists (PersonName). */
  a.name,
  .group-name a {
    display: inline-flex;
    align-items: center;
    min-width: var(--control-md);
    min-height: var(--control-md);
    color: inherit;
    font-weight: var(--weight-medium);
    text-decoration: none;
  }

  a.name:hover,
  .group-name a:hover {
    color: var(--color-link);
    text-decoration: underline;
  }

  .group-name {
    display: inline-flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-2);
    font-weight: var(--weight-medium);
  }

  .about {
    color: var(--color-text-muted);
    font-size: var(--text-sm);
  }

  .role,
  .badge {
    padding: 0.0625rem var(--space-2);
    border-radius: var(--radius-full);
    background: var(--color-surface-sunken);
    color: var(--color-text-muted);
    font-size: var(--text-xs);
    font-weight: var(--weight-medium);
  }

  .role {
    color: var(--color-text);
    font-size: var(--text-sm);
  }

  @media (width < 48rem) {
    .page {
      gap: var(--space-6);
      padding: var(--space-2) var(--space-4) var(--space-8);
    }
  }

  @media (forced-colors: active) {
    .role,
    .badge,
    .chip {
      border: 1px solid CanvasText;
    }
  }
</style>
