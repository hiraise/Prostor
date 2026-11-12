<script lang="ts">
  // The projects list (PRJ-010…PRJ-013): projects the person may see, their favorites first, the
  // first ten with "Show more"; a search by name or key kept in the address. Every row has a
  // visible star and a menu of what the person may do there (PRJ-011), so nothing depends on
  // hover (D-019). Archived projects sit in a collapsed section that loads when opened and offers
  // restore (PRJ-063); people who may delete projects also see those being deleted, with the
  // state of the job and a second try for failed ones (PRJ-065).
  import type { LucideIcon } from '@lucide/svelte';
  import Archive from '@lucide/svelte/icons/archive';
  import ChevronRight from '@lucide/svelte/icons/chevron-right';
  import CircleAlert from '@lucide/svelte/icons/circle-alert';
  import Clock from '@lucide/svelte/icons/clock';
  import Ellipsis from '@lucide/svelte/icons/ellipsis';
  import FolderKanban from '@lucide/svelte/icons/folder-kanban';
  import FolderOpen from '@lucide/svelte/icons/folder-open';
  import LoaderCircle from '@lucide/svelte/icons/loader-circle';
  import Plus from '@lucide/svelte/icons/plus';
  import SearchX from '@lucide/svelte/icons/search-x';
  import Settings2 from '@lucide/svelte/icons/settings-2';
  import Star from '@lucide/svelte/icons/star';
  import Trash2 from '@lucide/svelte/icons/trash-2';
  import { untrack } from 'svelte';
  import { resolve } from '$app/paths';

  import type { DeletionState, Project } from '#lib/api/schema.gen.ts';
  import { personName } from '#lib/features/access/index.ts';
  import * as common from '#lib/i18n/messages/common.ts';
  import type { ListHandle } from '#lib/store/index.ts';
  import Button from '#lib/ui/Button.svelte';
  import EmptyState from '#lib/ui/EmptyState.svelte';
  import ErrorState from '#lib/ui/ErrorState.svelte';
  import IconButton from '#lib/ui/IconButton.svelte';
  import Menu, { type MenuEntry } from '#lib/ui/Menu.svelte';
  import PageHeader from '#lib/ui/PageHeader.svelte';
  import Spinner from '#lib/ui/Spinner.svelte';
  import TextField from '#lib/ui/TextField.svelte';
  import { describeError, showError, toasts } from '#lib/ui/toast.svelte.ts';

  import ArchiveDialog from '../ArchiveDialog.svelte';
  import DeleteProjectDialog from '../DeleteProjectDialog.svelte';
  import * as shared from '../messages.ts';
  import ProjectLogo from '../ProjectLogo.svelte';
  import {
    canConfigure,
    canCreateProjects,
    canDelete,
    canDeleteSome,
    ensureProjects,
    projectHref,
    restartDeletion,
    restoreProject,
    setFavorite,
  } from '../projects.ts';
  import * as m from './messages.ts';

  interface Props {
    /** Active projects matching `query`, from the route's load. */
    active: ListHandle<Project>;
    /** Projects being deleted that match `query`; `null` for people who may delete none. */
    deleting?: ListHandle<Project> | null;
    /** The search the page opened with (from the address). */
    query?: string;
    /** The search changed; the route keeps it in the address (UI-006). */
    onsearch?: (text: string) => void;
    /** Opens a page of the app (row menu items). */
    onnavigate: (href: string) => unknown;
  }

  let { active, deleting = null, query = '', onsearch, onnavigate }: Props = $props();

  /** Rows shown before "Show more" (PRJ-013). */
  const FIRST = 10;
  /** Typing pauses this long before the list is searched. */
  const DEBOUNCE_MS = 250;
  /** How often the state of running deletions is asked again. */
  const POLL_MS = 5000;

  const id = $props.id();
  let text = $state(untrack(() => query));
  /** The search in effect: the typed text once typing pauses. */
  let term = $state(untrack(() => query.trim()));
  /** The lists of the search in effect; the route's lists until a search answers. */
  let lists = $state.raw({
    term: untrack(() => query.trim()),
    active: untrack(() => active),
    deleting: untrack(() => deleting),
  });
  let searching = $state(false);
  let searchProblem = $state<unknown>(null);
  let expanded = $state(false);
  let archivedOpen = $state(false);
  let archived = $state.raw<{ term: string; list: ListHandle<Project> } | null>(null);
  let archivedProblem = $state<unknown>(null);
  /** Rows with an action in flight (restore, delete again). */
  let pending = $state<number[]>([]);
  let archiving = $state<Project | null>(null);
  let archiveOpen = $state(false);
  let removing = $state<Project | null>(null);
  let deleteOpen = $state(false);
  let searchRun = 0;
  let archivedRun = 0;

  const mayCreate = $derived(canCreateProjects());
  // A row changes section at once when it is archived, restored or deleted; the lists catch up.
  const shownActive = $derived(
    lists.active.items.filter((project) => !project.archived_at && !project.deletion),
  );
  const visible = $derived(expanded ? shownActive : shownActive.slice(0, FIRST));
  const moreToShow = $derived(shownActive.length > visible.length || lists.active.hasMore);
  const shownArchived = $derived(
    archived?.list.items.filter((project) => project.archived_at && !project.deletion) ?? [],
  );
  const shownDeleting = $derived(
    canDeleteSome() ? (lists.deleting?.items.filter((project) => project.deletion) ?? []) : [],
  );

  // Search a moment after typing stops.
  $effect(() => {
    const typed = text.trim();
    const timer = setTimeout(() => {
      if (typed === untrack(() => term)) return;
      term = typed;
      onsearch?.(typed);
    }, DEBOUNCE_MS);
    return () => {
      clearTimeout(timer);
    };
  });

  // The lists of a new search; the lists shown stay until the new ones arrive.
  $effect(() => {
    const wanted = term;
    if (wanted === untrack(() => lists.term)) {
      // Back to the search shown: forget an answer still on its way.
      searchRun++;
      searching = false;
      searchProblem = null;
      return;
    }
    untrack(() => {
      search(wanted);
    });
  });

  // The archived section loads only when opened, and follows the search while open.
  $effect(() => {
    if (!archivedOpen) return;
    const wanted = term;
    if (untrack(() => archived?.term) === wanted) return;
    void loadArchived(wanted);
  });

  // While some deletion waits or runs, ask for its state now and then.
  $effect(() => {
    const list = lists.deleting;
    if (!list || !shownDeleting.some((project) => project.deletion?.state !== 'failed')) return;
    const timer = setInterval(() => {
      void list.refresh().catch(() => undefined);
    }, POLL_MS);
    return () => {
      clearInterval(timer);
    };
  });

  async function loadArchived(wanted: string): Promise<void> {
    const mine = ++archivedRun;
    archivedProblem = null;
    try {
      const list = await ensureProjects('archived', wanted);
      if (mine === archivedRun) archived = { term: wanted, list };
    } catch (error) {
      if (mine === archivedRun) archivedProblem = error;
    }
  }

  function search(wanted: string): void {
    const mine = ++searchRun;
    searching = true;
    Promise.all([
      ensureProjects('active', wanted),
      canDeleteSome() ? ensureProjects('deleting', wanted).catch(() => null) : null,
    ]).then(
      ([found, beingDeleted]) => {
        if (mine !== searchRun) return;
        lists = { term: wanted, active: found, deleting: beingDeleted };
        expanded = false;
        searchProblem = null;
        searching = false;
      },
      (error: unknown) => {
        if (mine !== searchRun) return;
        searchProblem = error;
        searching = false;
      },
    );
  }

  async function showMore(): Promise<void> {
    if (!expanded) {
      expanded = true;
      if (shownActive.length > FIRST) return;
    }
    try {
      await lists.active.loadMore();
    } catch (error) {
      showError(error, showMore);
    }
  }

  async function toggleStar(project: Project): Promise<void> {
    try {
      await setFavorite(project.id, !project.favorite);
    } catch (error) {
      const { requestId } = describeError(error);
      toasts.show({
        tone: 'error',
        message: shared.starFailed(),
        detail: requestId ? common.requestId({ id: requestId }) : undefined,
      });
    }
  }

  async function busyWith(projectId: number, action: () => Promise<void>): Promise<void> {
    pending = [...pending, projectId];
    try {
      await action();
    } finally {
      pending = pending.filter((other) => other !== projectId);
    }
  }

  function restore(project: Project): Promise<void> {
    return busyWith(project.id, async () => {
      try {
        const restored = await restoreProject(project.id);
        toasts.show({ tone: 'success', message: shared.projectRestored({ name: restored.name }) });
        await lists.active.refresh().catch(() => undefined);
      } catch (error) {
        showError(error);
      }
    });
  }

  function deleteAgain(project: Project): Promise<void> {
    return busyWith(project.id, async () => {
      try {
        await restartDeletion(project.id);
        toasts.show({ tone: 'success', message: m.deletionRestarted({ name: project.name }) });
        await lists.deleting?.refresh().catch(() => undefined);
      } catch (error) {
        showError(error);
      }
    });
  }

  async function archivedNow(): Promise<void> {
    await Promise.all([
      lists.active.refresh().catch(() => undefined),
      archived?.list.refresh().catch(() => undefined),
    ]);
  }

  async function deletedNow(): Promise<void> {
    const beingDeleted = lists.deleting;
    if (beingDeleted) {
      await beingDeleted.refresh().catch(() => undefined);
    } else if (canDeleteSome()) {
      const wanted = lists.term;
      const found = await ensureProjects('deleting', wanted).catch(() => null);
      if (found && lists.term === wanted) lists = { ...lists, deleting: found };
    }
    await lists.active.refresh().catch(() => undefined);
  }

  function actionsOf(project: Project): MenuEntry[] {
    const items: MenuEntry[] = [
      {
        type: 'action',
        label: shared.openProject(),
        icon: FolderOpen,
        onSelect: () => void onnavigate(projectHref.overview(project)),
      },
    ];
    const configure = canConfigure(project);
    if (configure) {
      items.push({
        type: 'action',
        label: shared.projectSettings(),
        icon: Settings2,
        onSelect: () => void onnavigate(projectHref.settings(project)),
      });
    }
    const removable = canDelete(project);
    if (configure || removable) items.push({ type: 'separator' });
    if (configure) {
      items.push({
        type: 'action',
        label: shared.archiveProject(),
        icon: Archive,
        onSelect: () => {
          archiving = project;
          archiveOpen = true;
        },
      });
    }
    if (removable) {
      items.push({
        type: 'action',
        label: shared.deleteProject(),
        icon: Trash2,
        danger: true,
        onSelect: () => {
          removing = project;
          deleteOpen = true;
        },
      });
    }
    return items;
  }

  const stateIcons: Record<DeletionState, LucideIcon> = {
    queued: Clock,
    running: LoaderCircle,
    failed: CircleAlert,
  };
</script>

<svelte:head><title>{shared.projects()} · trackflow</title></svelte:head>

<PageHeader title={shared.projects()}>
  {#snippet actions()}
    {#if mayCreate}
      <Button variant="primary" icon={Plus} href={resolve('/(app)/projects/new')}>
        {m.createProject()}
      </Button>
    {/if}
  {/snippet}
</PageHeader>

{#snippet facts(project: Project)}
  <span class="meta">
    {#if project.owner}<span>{m.ownedBy({ name: personName(project.owner) })}</span>{/if}
    <span class="count">{shared.teamSize({ count: project.team_size })}</span>
  </span>
{/snippet}

<div class="page">
  <search class="tools">
    <TextField
      class="find"
      type="search"
      label={m.findProjects()}
      hint={m.findProjectsHint()}
      bind:value={text}
      autocomplete="off"
      autocapitalize="off"
      spellcheck="false"
      maxlength={100}
    />
  </search>

  {#if searchProblem}
    <ErrorState error={searchProblem} level={2} onretry={() => search(term)} />
  {:else if lists.active.error && lists.active.ids.length === 0}
    <ErrorState error={lists.active.error} level={2} onretry={() => lists.active.refresh()} />
  {:else if shownActive.length === 0}
    {#if lists.term}
      <EmptyState
        icon={SearchX}
        title={m.noProjectsFound()}
        description={m.noProjectsFoundText()}
        level={2}
      />
    {:else}
      <EmptyState
        icon={FolderKanban}
        title={m.noProjects()}
        description={mayCreate ? m.noProjectsCreate() : m.noProjectsJoin()}
        level={2}
      >
        {#snippet actions()}
          {#if mayCreate}
            <Button variant="primary" icon={Plus} href={resolve('/(app)/projects/new')}>
              {m.createProject()}
            </Button>
          {/if}
        {/snippet}
      </EmptyState>
    {/if}
  {:else}
    <ul class="list" aria-label={m.projectList()} aria-busy={searching || lists.active.refreshing}>
      {#each visible as project (project.id)}
        <li class="row linked">
          <ProjectLogo
            projectKey={project.key}
            color={project.logo_color}
            src={project.logo_url ?? null}
          />
          <span class="text">
            <span class="title">
              <!-- Hrefs come from resolve(). -->
              <!-- eslint-disable-next-line svelte/no-navigation-without-resolve -->
              <a class="name" href={projectHref.overview(project)}>{project.name}</a>
              <span class="key">{project.key}</span>
            </span>
            {@render facts(project)}
          </span>
          <span class="actions">
            <IconButton
              icon={Star}
              label={project.favorite
                ? m.unstarProject({ name: project.name })
                : m.starProject({ name: project.name })}
              pressed={project.favorite}
              class={['star', { starred: project.favorite }]}
              onclick={() => toggleStar(project)}
            />
            <Menu
              label={shared.projectActions({ name: project.name })}
              items={actionsOf(project)}
              placement={{ side: 'bottom', align: 'end' }}
            >
              {#snippet trigger(props)}
                <IconButton
                  icon={Ellipsis}
                  label={shared.projectActions({ name: project.name })}
                  {...props}
                />
              {/snippet}
            </Menu>
          </span>
        </li>
      {/each}
    </ul>
    {#if moreToShow}
      <div>
        <Button loading={lists.active.loadingMore} onclick={showMore}>{m.showMore()}</Button>
      </div>
    {/if}
  {/if}

  {#if shownDeleting.length > 0}
    <section class="section" aria-labelledby="{id}-deleting">
      <h2 id="{id}-deleting" class="section-title">{m.deletingSection()}</h2>
      <p class="note">{m.deletingIntro()}</p>
      <ul class="list" aria-label={m.deletingList()}>
        {#each shownDeleting as project (project.id)}
          {@const deletion = project.deletion}
          <li class="row">
            <ProjectLogo
              projectKey={project.key}
              color={project.logo_color}
              src={project.logo_url ?? null}
            />
            <span class="text">
              <span class="title">
                <span class="name plain">{project.name}</span>
                <span class="key">{project.key}</span>
              </span>
              {#if deletion}
                {@const Icon = stateIcons[deletion.state]}
                <span class="meta">
                  <span class={['state', deletion.state]}>
                    <Icon aria-hidden="true" />{m.deletionStates[deletion.state]()}
                  </span>
                  {#if deletion.error}<span class="problem" lang="en">{deletion.error}</span>{/if}
                </span>
              {/if}
            </span>
            {#if deletion?.state === 'failed' && canDelete(project)}
              <span class="actions">
                <Button
                  icon={Trash2}
                  loading={pending.includes(project.id)}
                  aria-label={m.deleteAgainNamed({ name: project.name })}
                  onclick={() => deleteAgain(project)}
                >
                  {m.deleteAgain()}
                </Button>
              </span>
            {/if}
          </li>
        {/each}
      </ul>
    </section>
  {/if}

  <section class="section" aria-labelledby="{id}-archived">
    <h2 id="{id}-archived" class="section-title">
      <button
        type="button"
        class="disclosure"
        aria-expanded={archivedOpen}
        aria-controls="{id}-archived-body"
        onclick={() => {
          archivedOpen = !archivedOpen;
        }}
      >
        <ChevronRight aria-hidden="true" />{m.archivedSection()}
      </button>
    </h2>
    <div id="{id}-archived-body" class="archived" hidden={!archivedOpen}>
      {#if archivedOpen}
        {#if archivedProblem}
          <ErrorState error={archivedProblem} level={3} onretry={() => loadArchived(term)} />
        {:else if !archived}
          <Spinner label={common.loading()} />
        {:else if shownArchived.length === 0}
          <p class="note">{archived.term ? m.noArchivedFound() : m.noArchived()}</p>
        {:else}
          <ul class="list" aria-label={m.archivedList()} aria-busy={archived.list.refreshing}>
            {#each shownArchived as project (project.id)}
              <li class="row linked">
                <ProjectLogo
                  projectKey={project.key}
                  color={project.logo_color}
                  src={project.logo_url ?? null}
                />
                <span class="text">
                  <span class="title">
                    <!-- eslint-disable-next-line svelte/no-navigation-without-resolve -->
                    <a class="name" href={projectHref.overview(project)}>{project.name}</a>
                    <span class="key">{project.key}</span>
                  </span>
                  {@render facts(project)}
                </span>
                {#if canConfigure(project)}
                  <span class="actions">
                    <Button
                      loading={pending.includes(project.id)}
                      aria-label={m.restoreNamed({ name: project.name })}
                      onclick={() => restore(project)}
                    >
                      {m.restore()}
                    </Button>
                  </span>
                {/if}
              </li>
            {/each}
          </ul>
          {#if archived.list.hasMore}
            <div>
              <Button
                loading={archived.list.loadingMore}
                onclick={() =>
                  archived?.list.loadMore().catch((error: unknown) => showError(error))}
              >
                {m.showMore()}
              </Button>
            </div>
          {/if}
        {/if}
      {/if}
    </div>
  </section>
</div>

{#if archiving}
  <ArchiveDialog bind:open={archiveOpen} project={archiving} onarchived={archivedNow} />
{/if}
{#if removing}
  <DeleteProjectDialog bind:open={deleteOpen} project={removing} ondeleted={deletedNow} />
{/if}

<style>
  .page {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
    padding: var(--space-2) var(--space-6) var(--space-12);
  }

  .tools :global(.find) {
    max-width: 24rem;
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
    display: flex;
    align-items: center;
    gap: var(--space-3);
    min-height: calc(var(--control-md) + var(--space-4));
    padding: var(--space-1) var(--space-2) var(--space-1) var(--space-4);
  }

  .row + .row {
    border-top: 1px solid var(--color-border);
  }

  .row:first-child {
    border-start-start-radius: var(--radius-lg);
    border-start-end-radius: var(--radius-lg);
  }

  .row:last-child {
    border-end-start-radius: var(--radius-lg);
    border-end-end-radius: var(--radius-lg);
  }

  .linked:hover {
    background: var(--color-hover);
  }

  .text {
    display: flex;
    flex: 1;
    flex-wrap: wrap;
    align-items: center;
    gap: 0 var(--space-4);
    min-width: 0;
  }

  .title {
    display: flex;
    flex: 1 1 14rem;
    flex-wrap: wrap;
    align-items: center;
    gap: 0 var(--space-2);
    min-width: 0;
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

  /* The name is the link; it covers the whole row, so the row is the target (D-019). */
  a.name::after {
    content: '';
    position: absolute;
    inset: 0;
  }

  .linked:hover a.name {
    color: var(--color-link);
    text-decoration: underline;
  }

  .key {
    color: var(--color-text-muted);
    font-family: var(--font-mono);
    font-size: var(--text-sm);
  }

  .meta {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-0_5) var(--space-4);
    padding-bottom: var(--space-1);
    color: var(--color-text-muted);
    font-size: var(--text-sm);
  }

  .count {
    font-variant-numeric: tabular-nums;
  }

  /* Above the row's link, so the star and the menu stay their own targets. */
  .actions {
    position: relative;
    z-index: 1;
    display: flex;
    flex: none;
    align-items: center;
    gap: var(--space-1);
  }

  .actions :global(.star.starred svg) {
    fill: var(--color-brand);
    stroke: var(--color-text);
  }

  .section {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    margin-top: var(--space-4);
  }

  .section-title {
    font-size: var(--text-lg);
  }

  .disclosure {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
    min-height: var(--control-md);
    margin-left: calc(-1 * var(--space-1));
    padding: 0 var(--space-2) 0 var(--space-1);
    border: none;
    border-radius: var(--radius-md);
    background: none;
    color: var(--color-text);
    font: inherit;
    font-weight: var(--weight-semibold);
    cursor: pointer;
  }

  .disclosure:hover {
    background: var(--color-hover);
  }

  .disclosure :global(svg) {
    width: var(--icon-md);
    height: var(--icon-md);
    transition: transform var(--duration-fast) var(--ease-out);
  }

  .disclosure[aria-expanded='true'] :global(svg) {
    transform: rotate(90deg);
  }

  .archived {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }

  .archived[hidden] {
    display: none;
  }

  .note {
    max-width: 44rem;
    color: var(--color-text-muted);
    font-size: var(--text-sm);
  }

  .state {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
    color: var(--color-text);
    font-weight: var(--weight-medium);
  }

  .state :global(svg) {
    flex: none;
    width: var(--icon-sm);
    height: var(--icon-sm);
  }

  .state.failed {
    color: var(--color-danger);
  }

  .problem {
    overflow-wrap: anywhere;
  }

  @media (width < 48rem) {
    .page {
      padding: var(--space-2) var(--space-4) var(--space-8);
    }

    .tools :global(.find) {
      max-width: none;
    }

    .row {
      align-items: flex-start;
      padding-top: var(--space-2);
      padding-bottom: var(--space-2);
    }

    .row :global(.logo) {
      margin-top: calc((var(--control-md) - 2rem) / 2);
    }
  }
</style>
