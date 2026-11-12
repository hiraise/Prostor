<script lang="ts">
  // Adding people to a project (PRJ-048, PRJ-052, PRJ-056): accounts and groups in two tabs, and a
  // switch — join the team and get its roles, or get only one role in the project. Choosing a
  // built-in group warns that it opens the project to every account. Refusals name the missing
  // permissions (ACC-050).
  import { untrack } from 'svelte';

  import type {
    AccountSummary,
    Group,
    PermissionKey,
    Project,
    ProjectPeople,
    Role,
  } from '#lib/api/schema.gen.ts';
  import {
    PeopleSearch,
    groupName,
    predefinedRoles,
    roleName,
    worksInProjects,
  } from '#lib/features/access/index.ts';
  import * as common from '#lib/i18n/messages/common.ts';
  import { canSeeDirectory } from '#lib/session/index.ts';
  import { announce } from '#lib/ui/announce.svelte.ts';
  import Button from '#lib/ui/Button.svelte';
  import Dialog from '#lib/ui/Dialog.svelte';
  import Notice from '#lib/ui/Notice.svelte';
  import RadioGroup from '#lib/ui/RadioGroup.svelte';
  import Select, { type SelectOption } from '#lib/ui/Select.svelte';
  import { toasts } from '#lib/ui/toast.svelte.ts';

  import { addToTeam } from '../projects.ts';
  import GroupPicker from './GroupPicker.svelte';
  import * as m from './messages.ts';
  import {
    assignRoles,
    problemOf,
    teamRoles,
    type AddMode,
    type Holder,
    type Problem,
  } from './people.ts';
  import ProblemNotice from './ProblemNotice.svelte';

  interface Props {
    open?: boolean;
    project: Project;
    people: ProjectPeople;
    roles: Role[] | null;
    globalOnly: ReadonlySet<PermissionKey> | null;
    /** The switch's position when the dialog opens. */
    mode?: AddMode;
    /** Accounts known to be in the team directly; not offered for joining it. */
    members?: readonly number[];
    ondone: () => unknown;
  }

  let {
    open = $bindable(false),
    project,
    people,
    roles,
    globalOnly,
    mode = 'team',
    members = [],
    ondone,
  }: Props = $props();

  const id = $props.id();
  type Tab = 'people' | 'groups';
  let tab = $state<Tab>('people');
  let chosenPeople = $state<AccountSummary[]>([]);
  let chosenGroups = $state<Group[]>([]);
  let how = $state<string>('team');
  let roleValue = $state('');
  let nobody = $state(false);
  let roleError = $state<string | null>(null);
  let busy = $state(false);
  let problem = $state<Problem | null>(null);

  const directory = $derived(canSeeDirectory());
  const tabs = $derived<Tab[]>(directory ? ['people', 'groups'] : ['groups']);
  const joining = $derived(how === 'team');
  const teamRoleNames = $derived(teamRoles(people).map((assignment) => roleName(assignment.role)));
  const offered = $derived(
    roles && globalOnly ? roles.filter((role) => worksInProjects(role, globalOnly)) : [],
  );
  const roleOptions = $derived<SelectOption[]>([
    { value: '', label: m.chooseRoleOption() },
    ...offered.map((role) => ({ value: String(role.id), label: roleName(role) })),
  ]);
  const role = $derived(offered.find((item) => String(item.id) === roleValue));
  const roleAbout = $derived.by(() => {
    if (!role) return undefined;
    if (role.builtin) return predefinedRoles[role.builtin].description();
    return role.description.trim() === '' ? undefined : role.description;
  });
  const howOptions = $derived([
    {
      value: 'team',
      label: m.joinTeam(),
      hint:
        teamRoleNames.length > 0
          ? m.joinTeamHint({ roles: teamRoleNames.join(', ') })
          : m.joinTeamNoRoles(),
    },
    { value: 'role', label: m.onlyRole(), hint: m.onlyRoleHint() },
  ]);
  /** Built-in groups among the chosen: they open the project to every account (PRJ-056). */
  const broad = $derived(chosenGroups.filter((group) => group.builtin !== null));
  const count = $derived(chosenPeople.length + chosenGroups.length);

  $effect(() => {
    if (!open) return;
    untrack(() => {
      tab = directory ? 'people' : 'groups';
      chosenPeople = [];
      chosenGroups = [];
      how = mode;
      roleValue = '';
      nobody = false;
      roleError = null;
      problem = null;
    });
  });

  function onTabKey(event: KeyboardEvent, index: number): void {
    const last = tabs.length - 1;
    let next: number | null = null;
    if (event.key === 'ArrowRight') next = index === last ? 0 : index + 1;
    else if (event.key === 'ArrowLeft') next = index === 0 ? last : index - 1;
    else if (event.key === 'Home') next = 0;
    else if (event.key === 'End') next = last;
    const target = next === null ? undefined : tabs[next];
    if (target === undefined) return;
    event.preventDefault();
    tab = target;
    document.getElementById(`${id}-tab-${target}`)?.focus();
  }

  function tabLabel(value: Tab): string {
    return value === 'people' ? m.peopleTab() : m.groupsTab();
  }

  async function submit(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    problem = null;
    nobody = count === 0;
    roleError = !joining && !role ? m.chooseRole() : null;
    if (nobody || roleError) {
      announce(nobody ? m.chooseSomeone() : m.chooseRole(), 'assertive');
      return;
    }
    busy = true;
    try {
      if (joining) {
        const added = await addToTeam(project.id, {
          users: chosenPeople.map((person) => person.id),
          groups: chosenGroups.map((group) => group.id),
        });
        const total = (added.users?.length ?? 0) + (added.groups?.length ?? 0);
        open = false;
        toasts.show({
          tone: 'success',
          message: total > 0 ? m.addedToTeam({ count: total }) : m.nothingNew(),
        });
        await ondone();
      } else if (role) {
        const holders: Holder[] = [
          ...chosenPeople.map((person) => ({ user_id: person.id })),
          ...chosenGroups.map((group) => ({ group_id: group.id })),
        ];
        const done: Holder[] = [];
        try {
          await assignRoles(project.id, role.id, holders, (holder) => {
            done.push(holder);
            // Holders done leave the choice, so trying again skips them.
            if ('user_id' in holder) {
              chosenPeople = chosenPeople.filter((person) => person.id !== holder.user_id);
            } else {
              chosenGroups = chosenGroups.filter((group) => group.id !== holder.group_id);
            }
          });
        } finally {
          if (done.length > 0) await ondone();
        }
        open = false;
        toasts.show({ tone: 'success', message: m.roleAssigned() });
      }
    } catch (failure) {
      problem = problemOf(failure, joining ? m.lackingToJoin : m.lackingToAssign);
    } finally {
      busy = false;
    }
  }
</script>

<Dialog bind:open title={m.addPeopleTitle({ project: project.name })} dismissible={!busy}>
  <form id="{id}-form" class="form" novalidate onsubmit={submit}>
    {#if problem}<ProblemNotice {problem} />{/if}

    {#if tabs.length > 1}
      <div class="tabs" role="tablist" aria-label={m.whoTabs()}>
        {#each tabs as value, index (value)}
          <button
            type="button"
            role="tab"
            id="{id}-tab-{value}"
            class="tab"
            aria-selected={tab === value}
            aria-controls="{id}-panel-{value}"
            tabindex={tab === value ? 0 : -1}
            onclick={() => {
              tab = value;
            }}
            onkeydown={(event) => {
              onTabKey(event, index);
            }}
          >
            {tabLabel(value)}
          </button>
        {/each}
      </div>
    {:else}
      <p class="note">{m.noDirectory()}</p>
    {/if}

    {#if directory}
      <div
        role="tabpanel"
        id="{id}-panel-people"
        aria-labelledby="{id}-tab-people"
        hidden={tab !== 'people'}
      >
        <PeopleSearch
          label={m.choosePeople()}
          bind:chosen={chosenPeople}
          exclude={joining ? members : []}
        />
      </div>
    {/if}
    <div
      role={tabs.length > 1 ? 'tabpanel' : undefined}
      id="{id}-panel-groups"
      aria-labelledby={tabs.length > 1 ? `${id}-tab-groups` : undefined}
      hidden={tab !== 'groups'}
    >
      <GroupPicker
        label={m.chooseGroups()}
        bind:chosen={chosenGroups}
        exclude={joining ? people.team_groups.map((group) => group.id) : []}
      />
    </div>

    <p class={['summary', { error: nobody && count === 0 }]} aria-live="polite">
      {nobody && count === 0
        ? m.chooseSomeone()
        : m.chosenSummary({ people: chosenPeople.length, groups: chosenGroups.length })}
    </p>

    {#each broad as group (group.id)}
      <Notice tone="warning">{m.broadGroup({ group: groupName(group) })}</Notice>
    {/each}

    <RadioGroup legend={m.howTitle()} options={howOptions} bind:value={how} />
    {#if !joining}
      <Select
        label={m.role()}
        options={roleOptions}
        bind:value={roleValue}
        hint={roleAbout}
        error={roleError}
        required
        onchange={() => {
          roleError = null;
        }}
      />
    {/if}
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
    <Button type="submit" form="{id}-form" variant="primary" loading={busy}>
      {joining ? m.addToTeam() : m.assignRole()}
    </Button>
  {/snippet}
</Dialog>

<style>
  .form {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }

  .tabs {
    display: flex;
    gap: var(--space-1);
    border-bottom: 1px solid var(--color-border);
  }

  .tab {
    display: inline-flex;
    align-items: center;
    min-height: var(--control-md);
    padding: 0 var(--space-3);
    border: none;
    border-bottom: 2px solid transparent;
    background: none;
    color: var(--color-text-muted);
    font: inherit;
    font-weight: var(--weight-medium);
    cursor: pointer;
  }

  .tab:hover {
    color: var(--color-text);
  }

  /* The open tab: a bar, the text color, weight and aria-selected; never color alone. */
  .tab[aria-selected='true'] {
    border-bottom-color: var(--color-brand);
    color: var(--color-text);
    font-weight: var(--weight-semibold);
  }

  [hidden] {
    display: none;
  }

  .summary {
    color: var(--color-text-muted);
    font-size: var(--text-sm);
  }

  .summary.error {
    color: var(--color-danger);
  }

  .note {
    color: var(--color-text-muted);
    font-size: var(--text-sm);
  }

  @media (pointer: coarse) {
    .tab {
      flex: 1;
      justify-content: center;
    }
  }

  @media (forced-colors: active) {
    .tab[aria-selected='true'] {
      border-bottom-color: Highlight;
    }
  }
</style>
