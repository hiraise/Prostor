<script lang="ts">
  // The groups of the instance (ACC-021, ACC-022): built-in ones first, then by name, with member
  // counts, roles and auto-join. A filter shows groups of people, project teams or both; a team is
  // marked as one and opens its project's people page, where it changes (ACC-033). People who
  // manage groups create them (ACC-023) and delete a selection with a replacement (ACC-032); teams
  // are never selected. Rows wrap on phones; selection is a visible checkbox, never a hover action
  // (D-019).
  import Plus from '@lucide/svelte/icons/plus';
  import Trash2 from '@lucide/svelte/icons/trash-2';
  import Users from '@lucide/svelte/icons/users';
  import { untrack } from 'svelte';
  import { resolve } from '$app/paths';

  import type { Group } from '#lib/api/schema.gen.ts';
  import {
    builtinGroups,
    groupName,
    roleName,
    type GroupKind,
  } from '#lib/features/access/index.ts';
  import { ProjectLabel } from '#lib/features/projects/index.ts';
  import { getLocale } from '#lib/i18n/locale.svelte.ts';
  import { formatNumber } from '#lib/i18n/message.ts';
  import type { ListHandle } from '#lib/store/index.ts';
  import Button from '#lib/ui/Button.svelte';
  import EmptyState from '#lib/ui/EmptyState.svelte';
  import ErrorState from '#lib/ui/ErrorState.svelte';
  import Notice from '#lib/ui/Notice.svelte';
  import PageHeader from '#lib/ui/PageHeader.svelte';
  import Select, { type SelectOption } from '#lib/ui/Select.svelte';
  import TextField from '#lib/ui/TextField.svelte';
  import { describeError } from '#lib/ui/toast.svelte.ts';

  import * as admin from '../messages.ts';
  import Badge from './Badge.svelte';
  import CreateGroupDialog from './CreateGroupDialog.svelte';
  import DeleteGroupsDialog from './DeleteGroupsDialog.svelte';
  import { canManageGroups, ensureGroups, forgetGroups, groupKind, groupRow } from './groups.ts';
  import * as m from './messages.ts';

  interface Props {
    /** The groups that match `query`, from the route's load. */
    groups: ListHandle<Group>;
    /** The search the page opened with (from the address). */
    query?: string;
    /** The kind of groups the page opened with (from the address). */
    kind?: GroupKind;
    /** The search or the kind changed; the route keeps them in the address (UI-006). */
    onsearch?: (text: string, kind: GroupKind) => void;
    /** A group was created; the route opens its page. */
    oncreated: (group: Group) => unknown;
  }

  let { groups, query = '', kind = 'groups', onsearch, oncreated }: Props = $props();

  let text = $state(untrack(() => query));
  let shownKind = $state<string>(untrack(() => kind));
  let reported = untrack(() => ({ text: query.trim(), kind }));
  /** The list of the latest search; the route's list until a search answers. */
  let found = $state.raw<ListHandle<Group> | null>(null);
  let searchProblem = $state<string | null>(null);
  let selected = $state<number[]>([]);
  let creating = $state(false);
  let deleting = $state(false);

  const manage = $derived(canManageGroups());
  const list = $derived(found ?? groups);
  const chosen = $derived(
    selected.map((id) => groupRow(id)).filter((group): group is Group => group !== undefined),
  );

  const kinds = $derived<SelectOption[]>([
    { value: 'groups', label: m.kindGroups() },
    { value: 'teams', label: m.kindTeams() },
    { value: 'all', label: m.kindAll() },
  ]);

  // Search a moment after typing stops, a new kind at once; the list shown stays until the new one
  // arrives.
  $effect(() => {
    const typed = text.trim();
    const wanted = groupKind(shownKind);
    const current = (): boolean => text.trim() === typed && groupKind(shownKind) === wanted;
    const changedKind = wanted !== reported.kind;
    const timer = setTimeout(
      () => {
        if (typed !== reported.text || changedKind) {
          reported = { text: typed, kind: wanted };
          onsearch?.(typed, wanted);
        }
        ensureGroups(typed, wanted).then(
          (result) => {
            if (!current()) return;
            found = result;
            searchProblem = null;
          },
          (error: unknown) => {
            if (current()) searchProblem = describeError(error).message;
          },
        );
      },
      changedKind ? 0 : 250,
    );
    return () => {
      clearTimeout(timer);
    };
  });

  function toggle(id: number, on: boolean): void {
    selected = on ? [...selected, id] : selected.filter((other) => other !== id);
  }

  /** A group opens its page; a team opens its project's people page. */
  function href(group: Group): string {
    if (group.team_of !== null) {
      return resolve('/(app)/projects/[key]/settings/people', { key: String(group.team_of) });
    }
    return resolve('/(app)/admin/groups/[id]', { id: String(group.id) });
  }

  async function deleted(ids: number[]): Promise<void> {
    selected = selected.filter((id) => !ids.includes(id));
    forgetGroups(ids);
    const lists = list === groups ? [groups] : [groups, list];
    await Promise.all(lists.map((shown) => shown.refresh())).catch(() => undefined);
  }
</script>

<PageHeader title={admin.groups()}>
  {#snippet actions()}
    {#if manage}
      <Button
        variant="primary"
        icon={Plus}
        onclick={() => {
          creating = true;
        }}
      >
        {m.createGroup()}
      </Button>
    {/if}
  {/snippet}
</PageHeader>

<div class="page">
  <p class="intro">{m.groupsIntro()}</p>

  <div class="tools">
    <TextField
      class="search"
      label={m.findGroups()}
      type="search"
      hint={m.findGroupsHint()}
      bind:value={text}
      autocomplete="off"
      spellcheck="false"
    />
    <Select class="kind" label={m.kindFilter()} options={kinds} bind:value={shownKind} />
  </div>

  {#if searchProblem}<Notice tone="danger">{searchProblem}</Notice>{/if}

  {#if manage && selected.length > 0}
    <div class="selection" role="group" aria-label={m.selection()}>
      <span class="count" aria-live="polite">
        {m.selectedCount({ count: formatNumber(getLocale(), selected.length) })}
      </span>
      <div class="selection-actions">
        <Button
          variant="ghost"
          onclick={() => {
            selected = [];
          }}
        >
          {m.clearSelection()}
        </Button>
        <Button
          variant="danger"
          icon={Trash2}
          disabled={chosen.length === 0}
          onclick={() => {
            deleting = true;
          }}
        >
          {m.deleteGroups({ count: selected.length })}
        </Button>
      </div>
    </div>
  {/if}

  {#if list.error && list.ids.length === 0}
    <ErrorState error={list.error} level={2} onretry={() => list.refresh()} />
  {:else if list.ids.length === 0}
    <EmptyState
      icon={Users}
      title={m.noGroupsFound()}
      description={m.noGroupsFoundText()}
      level={2}
    />
  {:else}
    <ul class="list" aria-label={m.groupList()}>
      {#each list.items as group (group.id)}
        {@const name = groupName(group)}
        <li class="group">
          {#if manage}
            <div class="select">
              {#if !group.builtin && group.team_of === null}
                <label class="check">
                  <input
                    type="checkbox"
                    aria-label={m.selectGroup({ name })}
                    checked={selected.includes(group.id)}
                    onchange={(event) => {
                      toggle(group.id, event.currentTarget.checked);
                    }}
                  />
                </label>
              {/if}
            </div>
          {/if}
          <div class="text">
            <div class="title">
              <!-- href() resolves the path. -->
              <!-- eslint-disable-next-line svelte/no-navigation-without-resolve -->
              <a class="name" href={href(group)}>{name}</a>
              {#if group.builtin}<Badge tone="info">{m.builtinBadge()}</Badge>{/if}
              {#if group.team_of !== null}<Badge tone="info">{m.teamBadge()}</Badge>{/if}
              {#if group.auto_join && !group.builtin}<Badge>{m.autoJoinBadge()}</Badge>{/if}
            </div>
            {#if group.builtin}
              <p class="description">{builtinGroups[group.builtin].description()}</p>
            {/if}
            {#if group.description}<p class="description">{group.description}</p>{/if}
            <div class="meta">
              <span class="members">{m.memberCount({ count: group.members })}</span>
              {#if group.roles.length > 0}
                <ul class="roles" aria-label={m.groupRoles({ name })}>
                  {#each group.roles as held (held.assignment_id)}
                    <li>
                      <Badge>
                        {roleName(held.role)} ·
                        {#if held.project_id === null}
                          {m.globalScope()}
                        {:else}
                          <ProjectLabel id={held.project_id} />
                        {/if}
                      </Badge>
                    </li>
                  {/each}
                </ul>
              {/if}
            </div>
          </div>
        </li>
      {/each}
    </ul>
    {#if list.hasMore}
      <div>
        <Button loading={list.loadingMore} onclick={() => list.loadMore()}>{m.showMore()}</Button>
      </div>
    {/if}
  {/if}
</div>

{#if manage}
  <CreateGroupDialog bind:open={creating} {oncreated} />
  <DeleteGroupsDialog bind:open={deleting} groups={chosen} ondeleted={deleted} />
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

  .tools {
    display: flex;
    flex-wrap: wrap;
    align-items: flex-start;
    gap: var(--space-3) var(--space-4);
  }

  .tools :global(.search) {
    flex: 1 1 18rem;
    max-width: 24rem;
  }

  .tools :global(.kind) {
    flex: 0 1 14rem;
  }

  .selection {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2) var(--space-4);
    padding: var(--space-2) var(--space-2) var(--space-2) var(--space-4);
    border-radius: var(--radius-lg);
    background: var(--color-selected);
  }

  .count {
    font-weight: var(--weight-medium);
    font-variant-numeric: tabular-nums;
  }

  .selection-actions {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
  }

  .list {
    margin: 0;
    padding: 0;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-lg);
    background: var(--color-surface);
    list-style: none;
  }

  .group {
    display: flex;
    align-items: flex-start;
    gap: var(--space-1);
    padding: var(--space-2) var(--space-4) var(--space-3) var(--space-2);
  }

  .group + .group {
    border-top: 1px solid var(--color-border);
  }

  .select {
    flex: none;
    width: var(--control-md);
  }

  /* The whole square around the checkbox is the target. */
  .check {
    display: flex;
    align-items: center;
    justify-content: center;
    width: var(--control-md);
    height: var(--control-md);
    cursor: pointer;
  }

  .check input {
    width: 1rem;
    height: 1rem;
    margin: 0;
    accent-color: var(--color-ink);
  }

  @media (pointer: coarse) {
    .check input {
      width: 1.25rem;
      height: 1.25rem;
    }
  }

  .text {
    display: flex;
    flex: 1;
    flex-direction: column;
    gap: var(--space-1);
    min-width: 0;
    padding-left: var(--space-2);
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
    color: var(--color-text);
    font-weight: var(--weight-medium);
    text-decoration: none;
    overflow-wrap: anywhere;
  }

  .name:hover {
    color: var(--color-link);
    text-decoration: underline;
  }

  .description {
    max-width: 44rem;
    color: var(--color-text-muted);
    font-size: var(--text-sm);
    overflow-wrap: anywhere;
  }

  .meta {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-1) var(--space-3);
    color: var(--color-text-muted);
    font-size: var(--text-sm);
  }

  .members {
    font-variant-numeric: tabular-nums;
  }

  .roles {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-1);
    margin: 0;
    padding: 0;
    list-style: none;
  }

  @media (width < 48rem) {
    .page {
      padding: var(--space-2) var(--space-4) var(--space-8);
    }

    .group {
      padding-right: var(--space-3);
    }
  }
</style>
