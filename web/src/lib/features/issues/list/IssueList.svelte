<script lang="ts">
  // The issue list (QRY-053, QRY-054, QRY-069, QRY-073, QRY-074, QRY-081): the query bar, under it
  // the count, the server's notes and problems and saving the search, then the rows with «Показать
  // ещё». The address holds the query and the context; the route goes there on every search. Rows
  // are a table on wide screens and cards on phones, with the person's columns (QRY-075…QRY-078).
  import PanelLeft from '@lucide/svelte/icons/panel-left';
  import SearchX from '@lucide/svelte/icons/search-x';
  import { untrack } from 'svelte';
  import { MediaQuery } from 'svelte/reactivity';

  import type { FieldRef, Issue, IssueCount, QueryProblem } from '#lib/api/schema.gen.ts';
  import { loadQueryEngine } from '#lib/query/index.ts';
  import { preferences } from '#lib/session/index.ts';
  import { liveConnection, type ListHandle, type LiveSource } from '#lib/store/index.ts';
  import { NARROW } from '#lib/theme/breakpoints.ts';
  import { projectHref } from '#lib/features/projects/index.ts';
  import Breadcrumbs from '#lib/ui/Breadcrumbs.svelte';
  import Button from '#lib/ui/Button.svelte';
  import { copyLink } from '#lib/ui/copy.ts';
  import Dialog from '#lib/ui/Dialog.svelte';
  import EmptyState from '#lib/ui/EmptyState.svelte';
  import Menu, { type MenuEntry } from '#lib/ui/Menu.svelte';
  import Notice from '#lib/ui/Notice.svelte';
  import PageHeader from '#lib/ui/PageHeader.svelte';
  import Spinner from '#lib/ui/Spinner.svelte';
  import TextField from '#lib/ui/TextField.svelte';
  import { showError, toasts } from '#lib/ui/toast.svelte.ts';

  import ColumnsDialog from './ColumnsDialog.svelte';
  import { defaultColumns, issueFields, shownColumns, type ShownColumn } from './columns.ts';
  import IssueCard from './IssueCard.svelte';
  import IssueTable from './IssueTable.svelte';
  import {
    contextOf,
    copiedHref,
    frozenQuery,
    type ListContext,
    type ListParams,
    type ResolvedContext,
  } from './list-state.ts';
  import * as m from './messages.ts';
  import QueryBar from './QueryBar.svelte';
  import {
    countSearch,
    ensureSearch,
    queryProblems,
    saveSearch,
    searchMeta,
    updateSearchQuery,
    type SearchSpec,
  } from './search.svelte.ts';
  import { nextDirection, sortedBy, sortedQuery } from './sort.ts';
  import * as issueMessages from '../messages.ts';
  import { standInProject } from '../new/open.svelte.ts';
  import IssueSidebar from '../sidebar/IssueSidebar.svelte';
  import * as sidebarMessages from '../sidebar/messages.ts';

  interface Props {
    q: string;
    /** The address's context; `null` when it does not exist (all issues are shown). */
    context: ResolvedContext | null;
    /** The rows the route loaded for this address, or why they did not load. */
    list: ListHandle<Issue> | null;
    error?: unknown;
    onnavigate: (params: ListParams) => void;
    /** Where live changes come from; the tab's connection by default. */
    live?: LiveSource;
  }

  let {
    q,
    context: given,
    list: loaded,
    error: loadError = null,
    onnavigate,
    live,
  }: Props = $props();

  const id = $props.id();
  const context: ResolvedContext = $derived(given ?? { kind: 'all' });
  const hideResolved = $derived(preferences.value('list_hide_resolved'));
  const spec: SearchSpec = $derived({ q, context, hideResolved });

  // The route's rows first; another hide-resolved loads its own list. A first page that fails
  // leaves no list, only the error.
  let list: ListHandle<Issue> | null = $state(untrack(() => loaded));
  let failure: unknown = $state(untrack(() => loadError));
  $effect(() => {
    const wanted = spec;
    let current = true;
    ensureSearch(wanted)
      .then((found) => {
        if (!current) return;
        list = found;
        failure = null;
      })
      .catch((error: unknown) => {
        if (!current) return;
        list = null;
        failure = error;
      });
    return () => {
      current = false;
    };
  });

  function retry(): void {
    if (list) {
      void list.refresh().catch(() => undefined);
      return;
    }
    ensureSearch(spec)
      .then((found) => {
        list = found;
        failure = null;
      })
      .catch((error: unknown) => {
        failure = error;
      });
  }

  // A project's issues (UI-005, ISS-005): its crumbs, and new issues start in it.
  $effect(() => {
    if (context.kind === 'project') return standInProject(context.key);
  });

  // Live rows (QRY-086): shown rows update in place; other changes wait for «Обновить».
  $effect(() => {
    const topic = meta?.liveTopic;
    const shown = list;
    if (!topic || !shown) return;
    return shown.follow(live ?? liveConnection(), topic);
  });

  /** Grows with every «Обновить»: the count and the sidebar's counts are fetched again. */
  let revision = $state(0);
  let refreshing = $state(false);

  async function refreshList(): Promise<void> {
    if (!list) return;
    refreshing = true;
    try {
      await list.refresh();
      revision += 1;
    } catch (error) {
      showError(error);
    } finally {
      refreshing = false;
    }
  }

  let count = $state<IssueCount | null>(null);
  $effect(() => {
    // A reload («Обновить») counts again.
    const asked = { spec, revision };
    const abort = new AbortController();
    countSearch(asked.spec, abort.signal)
      .then((found) => {
        count = found;
      })
      .catch(() => {
        if (!abort.signal.aborted) count = null;
      });
    return () => {
      abort.abort();
    };
  });

  const meta = $derived(searchMeta(spec));
  const error: unknown = $derived(list ? list.error : failure);
  const refused: QueryProblem[] | null = $derived(queryProblems(error));
  const failed = $derived(error !== null && error !== undefined && refused === null);
  const notes = $derived(refused === null ? (meta?.notes ?? []) : []);
  const resolvedShown = $derived(
    !hideResolved || notes.some((n) => n.code === 'hide_resolved_ignored'),
  );
  const shownCount = $derived(count === null ? null : resolvedShown ? count.all : count.unresolved);

  let showConditions = $state(false);

  // Columns (QRY-075, QRY-078): the person's, or ID, summary and three fields of the context.
  const narrow = new MediaQuery(NARROW, false);
  const contextProject = $derived(context.kind === 'project' ? context.id : null);
  let allFields: FieldRef[] = $state([]);
  let contextFields: FieldRef[] | null = $state(null);
  $effect(() => {
    issueFields(null)
      .then((fields) => {
        allFields = fields;
      })
      .catch(() => undefined);
  });
  $effect(() => {
    const wanted = contextProject;
    contextFields = null;
    if (wanted === null) return;
    let current = true;
    issueFields(wanted)
      .then((fields) => {
        if (current) contextFields = fields;
      })
      .catch(() => undefined);
    return () => {
      current = false;
    };
  });
  const chosenColumns = $derived(preferences.value('list_columns'));
  const columnTexts = $derived(
    chosenColumns.length > 0 ? chosenColumns : defaultColumns(contextFields ?? allFields),
  );
  const columns = $derived(shownColumns(columnTexts, [...allFields, ...(contextFields ?? [])]));
  let choosing = $state(false);

  // The sidebar (QRY-082): a drawer on phones, otherwise shown as the person chose.
  let drawer = $state(false);
  const sidebarMode = $derived(preferences.value('list_sidebar'));
  const modes = $derived.by((): MenuEntry[] =>
    (
      [
        ['open', sidebarMessages.modeOpen()],
        ['auto', sidebarMessages.modeAuto()],
        ['closed', sidebarMessages.modeClosed()],
      ] as const
    ).map(([mode, label]) => ({
      type: 'radio',
      label,
      checked: sidebarMode === mode,
      onSelect: () => {
        preferences.set({ list_sidebar: mode });
      },
    })),
  );

  async function sortBy(column: ShownColumn): Promise<void> {
    if (column.sortName === null) return;
    try {
      const engine = await loadQueryEngine();
      const direction = nextDirection(sortedBy(column.column, meta?.sort ?? []));
      go({ q: sortedQuery(q, engine.analyze(q).sort, column.sortName, direction) });
    } catch (error) {
      showError(error);
    }
  }

  function go(next: Partial<ListParams>): void {
    onnavigate({ q, context: contextOf(context), ...next });
  }

  function choose(next: ListContext): void {
    showConditions = false;
    go({ context: next });
  }

  function problemText(problem: QueryProblem): string {
    const text = q.slice(problem.from, problem.to).trim();
    const meaning = m.problems[problem.code]();
    return text === '' ? meaning : m.problemAt({ text, problem: meaning });
  }

  async function copy(): Promise<void> {
    try {
      const engine = await loadQueryEngine();
      await copyLink(copiedHref(engine, q, context));
    } catch (error) {
      showError(error);
    }
  }

  // Saving (QRY-054).
  const canSave = $derived(q.trim() !== '' || context.kind === 'project' || context.kind === 'tag');
  const canUpdate = $derived(context.kind === 'search' && context.editable && q.trim() !== '');
  let saving = $state(false);
  let naming = $state(false);
  let name = $state('');
  let nameError: string | null = $state(null);

  function startSave(): void {
    name = '';
    nameError = null;
    naming = true;
  }

  async function save(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    if (name.trim() === '') {
      nameError = m.nameRequired();
      return;
    }
    saving = true;
    try {
      const engine = await loadQueryEngine();
      const query = frozenQuery(engine, q, context, true).q;
      const saved = await saveSearch({ name: name.trim(), query });
      naming = false;
      toasts.show({ tone: 'success', message: m.searchSaved({ name: name.trim() }) });
      onnavigate({ q: '', context: contextOf(saved) });
    } catch (error) {
      showError(error);
    } finally {
      saving = false;
    }
  }

  async function update(): Promise<void> {
    if (context.kind !== 'search') return;
    saving = true;
    try {
      const engine = await loadQueryEngine();
      // The search keeps its own project; only the query changes.
      const query = frozenQuery(engine, q, { ...context, projectKey: null }, true).q;
      const saved = await updateSearchQuery(context.id, query, context.version);
      toasts.show({ tone: 'success', message: m.searchUpdated({ name: context.name }) });
      onnavigate({ q: '', context: contextOf(saved) });
    } catch (error) {
      showError(error);
    } finally {
      saving = false;
    }
  }
</script>

<div class="layout">
  <IssueSidebar
    {context}
    {hideResolved}
    {revision}
    bind:drawer
    onchoose={(next: ListContext) => {
      choose(next);
    }}
  />
  <div class="main">
    {#if context.kind === 'project'}
      <Breadcrumbs
        label={issueMessages.breadcrumbs()}
        items={[{ label: context.name, href: projectHref.overview(context) }]}
      />
    {/if}
    <PageHeader title={m.title()}>
      {#snippet actions()}
        {#if narrow.current}
          <Button
            icon={PanelLeft}
            variant="secondary"
            aria-haspopup="dialog"
            onclick={() => {
              drawer = true;
            }}
          >
            {sidebarMessages.sidebar()}
          </Button>
        {:else}
          <Menu
            label={sidebarMessages.sidebarMode()}
            items={modes}
            placement={{ side: 'bottom', align: 'end' }}
          >
            {#snippet trigger(props)}
              <Button icon={PanelLeft} variant="ghost" {...props}
                >{sidebarMessages.sidebar()}</Button
              >
            {/snippet}
          </Menu>
        {/if}
      {/snippet}
    </PageHeader>

    <div class="page">
      <QueryBar
        {q}
        {context}
        {hideResolved}
        problems={refused ?? notes}
        describedby={refused !== null || notes.length > 0 ? `${id}-problems` : undefined}
        onsearch={(text: string) => {
          go({ q: text });
        }}
        oncontext={choose}
        onhideresolved={(hide: boolean) => {
          preferences.set({ list_hide_resolved: hide });
        }}
        oncopylink={() => void copy()}
        oncolumns={() => {
          choosing = true;
        }}
      />

      {#if given === null}
        <Notice tone="warning">{m.contextMissing()}</Notice>
      {/if}

      {#if context.kind === 'search'}
        <div class="conditions">
          <Button
            variant="ghost"
            size="sm"
            aria-expanded={showConditions}
            aria-controls="{id}-conditions"
            onclick={() => {
              showConditions = !showConditions;
            }}
          >
            {showConditions ? m.hideConditions() : m.showConditions()}
          </Button>
          <p id="{id}-conditions" class="saved-query" hidden={!showConditions}>
            <span class="sr-only">{m.conditionsOf({ name: context.name })}</span>
            <code>{context.query}</code>
          </p>
        </div>
      {/if}

      <div class="status">
        <p class="count" aria-live="polite">
          {#if shownCount !== null && refused === null}{m.count({ count: shownCount })}{/if}
        </p>
        {#if canSave || canUpdate}
          <div class="save">
            {#if canUpdate}
              <Button variant="secondary" size="sm" loading={saving} onclick={() => void update()}>
                {m.updateSearch()}
              </Button>
            {/if}
            {#if canSave}
              <Button variant="ghost" size="sm" onclick={startSave}>{m.saveSearch()}</Button>
            {/if}
          </div>
        {/if}
      </div>

      {#if refused !== null || notes.length > 0}
        <div id="{id}-problems">
          {#if refused !== null}
            <Notice tone="danger">
              <p class="problems-title">{m.problemsTitle()}</p>
              <ul class="problems">
                {#each refused as problem, i (i)}<li>{problemText(problem)}</li>{/each}
              </ul>
            </Notice>
          {:else}
            <Notice tone="info">
              <ul class="problems">
                {#each notes as note, i (i)}<li>{problemText(note)}</li>{/each}
              </ul>
            </Notice>
          {/if}
        </div>
      {/if}

      {#if failed}
        <Notice tone="danger">
          {m.loadFailed()}
          <Button variant="secondary" size="sm" onclick={retry}>
            {m.retry()}
          </Button>
        </Notice>
      {/if}

      {#if refused === null && list?.stale}
        <Notice tone="info">
          {m.listChanged()}
          <Button
            variant="secondary"
            size="sm"
            loading={refreshing}
            onclick={() => void refreshList()}
          >
            {m.refreshList()}
          </Button>
        </Notice>
      {/if}

      {#if refused === null && list}
        {#if list.ids.length > 0}
          {#if narrow.current}
            <ul class="rows" aria-label={m.rows()} aria-busy={list.refreshing}>
              {#each list.items as issue (issue.id)}
                <li class="row"><IssueCard {issue} {columns} /></li>
              {/each}
            </ul>
          {:else}
            <IssueTable
              items={list.items}
              {columns}
              sort={meta?.sort ?? []}
              busy={list.refreshing}
              onsort={(column: ShownColumn) => void sortBy(column)}
            />
          {/if}
          {#if list.hasMore}
            <div class="more">
              <Button
                variant="secondary"
                loading={list.loadingMore}
                onclick={() => void list?.loadMore().catch(() => undefined)}
              >
                {m.showMore()}
              </Button>
            </div>
          {/if}
        {:else if list.refreshing}
          <p class="loading"><Spinner /></p>
        {:else if !failed}
          <EmptyState title={m.nothingFound()} icon={SearchX} level={2}>
            <ul class="hints">
              <li>{m.hintFewer()}</li>
              <li>{m.hintValues()}</li>
              {#if !resolvedShown}<li>{m.hintResolved()}</li>{/if}
            </ul>
            {#snippet actions()}
              {#if !resolvedShown}
                <Button
                  variant="secondary"
                  onclick={() => {
                    preferences.set({ list_hide_resolved: false });
                  }}
                >
                  {m.showResolved()}
                </Button>
              {/if}
            {/snippet}
          </EmptyState>
        {/if}
      {/if}
    </div>
  </div>
</div>

<Dialog
  bind:open={naming}
  title={m.saveSearch()}
  size="sm"
  dismissible={!saving}
  onclose={() => {
    naming = false;
  }}
>
  <form id="{id}-save" onsubmit={(event) => void save(event)}>
    <TextField label={m.searchName()} bind:value={name} error={nameError} maxlength={100} />
  </form>
  {#snippet footer()}
    <Button
      variant="ghost"
      disabled={saving}
      onclick={() => {
        naming = false;
      }}
    >
      {m.cancel()}
    </Button>
    <Button variant="primary" type="submit" form="{id}-save" loading={saving}>{m.save()}</Button>
  {/snippet}
</Dialog>

<ColumnsDialog
  bind:open={choosing}
  columns={columns.map((shown) => shown.text)}
  known={allFields}
  project={contextProject}
  onsave={(next: string[] | null) => {
    preferences.set({ list_columns: next });
    choosing = false;
  }}
  onclose={() => {
    choosing = false;
  }}
/>

<style>
  .layout {
    display: flex;
    align-items: flex-start;
    min-height: 100%;
  }

  .main {
    flex: 1;
    min-width: 0;
  }

  .page {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    padding: 0 var(--space-6) var(--space-12);
  }

  .conditions {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: var(--space-1);
  }

  .saved-query code {
    font-family: var(--font-mono);
    font-size: var(--text-sm);
    overflow-wrap: anywhere;
  }

  .status {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2);
    min-height: var(--control-sm);
  }

  .count {
    color: var(--color-text-muted);
    font-size: var(--text-sm);
  }

  .save {
    display: flex;
    gap: var(--space-2);
  }

  .problems-title {
    font-weight: var(--weight-medium);
  }

  .problems {
    padding-inline-start: var(--space-4);
    list-style: disc;
  }

  .rows {
    border: 1px solid var(--color-border);
    border-radius: var(--radius-lg);
    background: var(--color-surface);
  }

  .row + .row {
    border-top: 1px solid var(--color-border);
  }

  .more,
  .loading {
    display: flex;
    justify-content: center;
  }

  .hints {
    color: var(--color-text-muted);
    text-align: start;
  }

  @media (width < 48rem) {
    .page {
      padding: 0 var(--space-4) var(--space-8);
    }
  }
</style>
