<script lang="ts">
  // The issue list (QRY-053, QRY-054, QRY-069, QRY-073, QRY-074, QRY-081): the query bar, under it
  // the count, the server's notes and problems and saving the search, then the rows with «Показать
  // ещё». The address holds the query and the context; the route goes there on every search. Rows
  // are a table on wide screens and cards on phones, with the person's columns (QRY-075…QRY-078).
  import PanelLeft from '@lucide/svelte/icons/panel-left';
  import SearchX from '@lucide/svelte/icons/search-x';
  import { goto } from '$app/navigation';
  import { onMount, untrack } from 'svelte';
  import { MediaQuery, SvelteMap } from 'svelte/reactivity';

  import type { FieldRef, Issue, IssueCount, QueryProblem } from '#lib/api/schema.gen.ts';
  import { loadQueryEngine } from '#lib/query/index.ts';
  import {
    claim,
    enter,
    leave,
    setFocusedEntity,
    type FocusedEntity,
  } from '#lib/shortcuts/registry.svelte.ts';
  import { preferences } from '#lib/session/index.ts';
  import { liveConnection, type ListHandle, type LiveSource } from '#lib/store/index.ts';
  import { NARROW } from '#lib/theme/breakpoints.ts';
  import { announce } from '#lib/ui/announce.svelte.ts';
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

  import { assignToMe, setStar } from '../actions.ts';
  import CommandDialog from '../commands/CommandDialog.svelte';
  import { recentCommands } from '../commands/recent.svelte.ts';
  import { issueHref } from '../issues.ts';
  import ColumnsDialog from './ColumnsDialog.svelte';
  import { defaultColumns, issueFields, shownColumns, type ShownColumn } from './columns.ts';
  import { createRowKeys } from './keyboard.ts';
  import IssueCard from './IssueCard.svelte';
  import IssueTable from './IssueTable.svelte';
  import {
    contextOf,
    copiedHref,
    frozenQuery,
    ListFocus,
    ListSelection,
    type ListContext,
    type ListParams,
    type ResolvedContext,
  } from './list-state.svelte.ts';
  import * as m from './messages.ts';
  import QueryBar from './QueryBar.svelte';
  import SelectionBar from './SelectionBar.svelte';
  import SelectionPicker from './SelectionPicker.svelte';
  import { exportIssues, type ExportFormat } from './export.ts';
  import {
    countSearch,
    ensureSearch,
    params,
    PAGE,
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

  // Applying commands (step 2.1 §6, CMD-026; step 2.2 §8.2): the query's worth from the header's
  // «Применить команду ко всем…», or the selection from the toolbar — explicit IDs while a page's
  // worth is chosen, the bulk job once everything the query matches is.
  let commanding = $state(false);
  /** What an open dialog applies to. */
  let commandTarget = $state<'query' | 'selection'>('query');
  /** The line the dialog opens with; `undefined` shows the last applied one. */
  let commandPreset = $state<string | undefined>(undefined);

  function openCommands(on: 'query' | 'selection', preset?: string): void {
    commandTarget = on;
    commandPreset = preset;
    commanding = true;
  }

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

  // ---- The list on the keyboard (step 2.2 §6, QRY-085, UI-022) ----

  /** The rows live under this element; the roving focus finds a row's element by its issue ID. */
  let pageEl: HTMLElement | undefined = $state();
  const focus = new ListFocus();

  /** Moves the roving focus (§6): the row's element takes the DOM focus — which scrolls it into
   * view — and the move is announced politely. */
  function focusRow(id: number | null): void {
    focus.current = id;
    if (id === null) return;
    const row = list?.items.find((issue) => issue.id === id);
    pageEl?.querySelector<HTMLElement>(`[data-row="${String(id)}"]`)?.focus();
    if (row) {
      announce(m.focusedRow({ id: row.readable_id ?? String(row.id), summary: row.summary }));
    }
  }

  /** `,` and `.` (§6): one page of rows back or forward; the step past what is loaded brings the
   * next page in and lands on its first row. */
  function turnPage(step: 1 | -1): void {
    const shown = list?.items ?? [];
    if (shown.length === 0) return;
    const at = shown.findIndex((row) => row.id === focus.current);
    const next = Math.floor((at === -1 ? 0 : at) / PAGE) + step;
    if (next < 0) return;
    const target = next * PAGE;
    const landed = shown[target];
    if (landed) {
      focusRow(landed.id);
      return;
    }
    if (!list?.hasMore) return;
    void list
      .loadMore()
      .then(() => {
        const loaded = list?.items[target];
        // The row renders with the loaded page; its element takes the focus one frame later.
        if (loaded) requestAnimationFrame(() => focusRow(loaded.id));
      })
      .catch(() => undefined);
  }

  /** `b` (§6): the sidebar hides or shows; on a phone it is the drawer. */
  function toggleSidebar(): void {
    if (narrow.current) {
      drawer = !drawer;
      return;
    }
    preferences.set({ list_sidebar: sidebarMode === 'closed' ? 'open' : 'closed' });
  }

  /** The rows' shared keys (§6): the actions are the ones `RowMenu` and `RowStar` call. */
  const rowKeys = createRowKeys({
    rows: () => list?.items ?? [],
    focus: focusRow,
    open: (row: Issue, newTab: boolean) => {
      if (newTab) window.open(issueHref(row), '_blank');
      // eslint-disable-next-line svelte/no-navigation-without-resolve -- issueHref resolves it.
      else void goto(issueHref(row));
    },
    // eslint-disable-next-line svelte/no-navigation-without-resolve -- issueHref resolves it.
    edit: (row: Issue) => void goto(`${issueHref(row)}?edit`),
    star: (row: Issue) => {
      setStar(row, !row.starred).catch((error: unknown) => {
        showError(error);
      });
    },
    assignMe: assignToMe,
    sidebar: toggleSidebar,
    page: turnPage,
    select: {
      toggle: toggleSelection,
      extend: extendSelection,
      page: choosePage,
    },
  });

  // The list's context (§6): the framework's global keys act here — the copy trio works on the
  // focused row — and the overlay lists the list's keys while this page is shown. The lifecycle,
  // not an effect: `enter` mutates the registry's reactive stack, and an effect that touches it
  // would never settle. The command combo (§8.2) is claimed for the selection; with none chosen
  // it opens the header's flow over the query, and bare letters wait for a selection.
  onMount(() => {
    enter('list');
    setFocusedEntity(() => entityOf(focus.current));
    const releaseCommand = claim('command', (preset?: string) => {
      if (preset !== undefined && selection.empty) return false;
      // With nothing chosen the combo still opens — over the query, as the header's entry does.
      openCommands(selection.empty ? 'query' : 'selection', preset);
    });
    const releaseRepeat = claim('command.repeat', () => {
      if (selection.empty || recentCommands.last() === null) return false;
      openCommands('selection', recentCommands.last() ?? undefined);
    });
    return () => {
      leave('list');
      setFocusedEntity(null);
      releaseCommand();
      releaseRepeat();
    };
  });

  // ---- The selection and its toolbar (step 2.2 §8, QRY-083, QRY-084) ----

  const selection = new ListSelection();
  /** The phone's selection mode (§8.1): «Выбрать» in the list's settings menu turns it on. */
  let choosingRows = $state(false);

  const pageIds = $derived((list?.items ?? []).map((issue) => issue.id));
  /** The rows the list has shown, by ID: a chosen row keeps its issue for the command dialog even
   * after a refresh takes it off the page (§8.1 — the selection survives by IDs). A `SvelteMap`,
   * not `$state`: Svelte's deep proxy does not take collections. */
  const known = new SvelteMap<number, Issue>();
  $effect(() => {
    for (const issue of list?.items ?? []) known.set(issue.id, issue);
  });
  const chosenIssues = $derived.by(() =>
    selection.idsOf().flatMap((id) => {
      const issue = known.get(id);
      return issue === undefined ? [] : [issue];
    }),
  );
  const chosenCount = $derived(
    selection.allMatching ? (shownCount ?? selection.size) : selection.size,
  );
  /** The offer of §8.1: more issues match than the page shows. */
  const matchingOffer = $derived(
    !selection.allMatching && shownCount !== null && shownCount > pageIds.length
      ? shownCount
      : null,
  );
  /** «Удалить» needs the delete right on something chosen; the server checks anyway (§8.2). */
  const canDeleteChosen = $derived(selection.allMatching || chosenIssues.some((i) => i.can.delete));

  // The toolbar's pickers (step 2.2 §8.3, QRY-084): which of the three is open, and the right
  // their flows need — the same rule as «Удалить»'s, for the update right. The kind stays while
  // the dialog closes, so its title does not flash to another picker's.
  let picking = $state(false);
  let pickerKind = $state<'tag' | 'assignee' | 'link'>('tag');
  const canChangeChosen = $derived(selection.allMatching || chosenIssues.some((i) => i.can.update));
  /** The assignee field's name as the person sees it; the picker's line names the field (§8.3). */
  const assigneeFieldName = $derived(
    allFields.find((field) => field.builtin === 'assignee')?.name ?? null,
  );

  function openPicker(kind: 'tag' | 'assignee' | 'link'): void {
    pickerKind = kind;
    picking = true;
  }

  /** The count of the new state, politely (§8.1) — every change of the selection passes here. */
  function announceSelection(): void {
    announce(m.selectedCount({ count: chosenCount }));
  }

  function toggleSelection(row: Issue): void {
    selection.click(row.id, pageIds);
    announceSelection();
  }

  /** `Shift+↑/↓` (§8.1): the focus moves, the range reaches the row it lands on. */
  function extendSelection(row: Issue, step: 1 | -1): void {
    const shown = list?.items ?? [];
    const at = shown.findIndex((issue) => issue.id === row.id);
    const landed = shown[at + step];
    if (!landed) return;
    focusRow(landed.id);
    selection.keyExtend(row.id, landed.id, pageIds);
    announceSelection();
  }

  /** `Ctrl/⌘+A` (§8.1): the page's issues chosen; the toolbar then offers all matching. */
  function choosePage(): void {
    selection.selectPage(true, pageIds);
    announceSelection();
  }

  function takeAllMatching(): void {
    selection.selectAllMatching();
    announceSelection();
  }

  /** «Снять выделение» — on a phone it also leaves the selection mode (§8.1). */
  function clearChosen(): void {
    selection.clear();
    choosingRows = false;
    announceSelection();
  }

  // The toolbar's export (§9, QRY-084): the chosen issues, or — when everything matches — the
  // query as the list runs it. Fewer rows in the file than matches is the cap or invisible
  // issues; the note says so, the counts come from the answer's headers.
  let exporting = $state<ExportFormat | null>(null);

  async function exportChosen(format: ExportFormat): Promise<void> {
    exporting = format;
    try {
      const done = await exportIssues(
        selection.allMatching
          ? { query: { ...params(spec), hide_resolved: spec.hideResolved || undefined }, format }
          : { ids: selection.idsOf(), format },
      );
      if (done.exported !== null && done.total !== null && done.exported < done.total) {
        toasts.show({
          tone: 'info',
          message: m.exportTruncated({ exported: done.exported, total: done.total }),
        });
      }
    } catch (error) {
      showError(error);
    } finally {
      exporting = null;
    }
  }

  function toggleChoosing(): void {
    if (choosingRows) selection.clear();
    choosingRows = !choosingRows;
  }

  function entityOf(id: number | null): FocusedEntity | null {
    const row = id === null ? null : (list?.items.find((issue) => issue.id === id) ?? null);
    if (row === null) return null;
    return { id: row.readable_id ?? String(row.id), href: issueHref(row), summary: row.summary };
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

    <div class="page" bind:this={pageEl}>
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
        applyAll={shownCount === null ? null : { count: shownCount }}
        onapplyall={() => {
          openCommands('query');
        }}
        onselect={narrow.current ? toggleChoosing : undefined}
        selecting={choosingRows}
        ontolist={() => {
          // Tab enters the list at its first row (§6); without rows the key moves on.
          const first = list?.items[0];
          if (!first) return false;
          focusRow(first.id);
          return true;
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

      <!-- The toolbar of the selection (§8.2): a bar over the rows, a bottom sheet on a phone. -->
      {#if !selection.empty}
        <SelectionBar
          count={chosenCount}
          allMatching={selection.allMatching}
          matching={matchingOffer}
          canDelete={canDeleteChosen}
          canChange={canChangeChosen}
          oncommands={() => openCommands('selection')}
          onlink={() => openPicker('link')}
          ontag={() => openPicker('tag')}
          onassignee={() => openPicker('assignee')}
          onexport={(format: ExportFormat) => void exportChosen(format)}
          {exporting}
          ondelete={() => openCommands('selection', 'delete')}
          onclear={clearChosen}
          onallmatching={takeAllMatching}
        />
      {/if}

      {#if refused === null && list}
        {#if list.ids.length > 0}
          {#if narrow.current}
            <ul class="rows" aria-label={m.rows()} aria-busy={list.refreshing}>
              {#each list.items as issue, index (issue.id)}
                {@const tab =
                  focus.current === null
                    ? index === 0
                      ? 0
                      : -1
                    : focus.current === issue.id
                      ? 0
                      : -1}
                <!-- A card is the row of the keyboard (§6): roving tabindex carries the list's
                     keys to an attached keyboard, as the table's grid does on a wide screen. In
                     the selection mode of §8.1 its checkbox comes before the card. -->
                <!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
                <li
                  class="row"
                  class:choosing={choosingRows}
                  data-row={issue.id}
                  tabindex={tab}
                  onfocusin={() => {
                    focus.current = issue.id;
                  }}
                  onkeydown={(event: KeyboardEvent) => {
                    rowKeys(event, issue);
                  }}
                >
                  {#if choosingRows}
                    <input
                      type="checkbox"
                      class="choose"
                      checked={selection.has(issue.id)}
                      aria-label={m.selectRow({ id: issue.readable_id ?? String(issue.id) })}
                      onclick={(event: MouseEvent) => {
                        selection.click(issue.id, pageIds, event.shiftKey);
                        announceSelection();
                      }}
                    />
                  {/if}
                  <IssueCard {issue} {columns} />
                </li>
              {/each}
            </ul>
          {:else}
            <IssueTable
              items={list.items}
              {columns}
              sort={meta?.sort ?? []}
              busy={list.refreshing}
              onsort={(column: ShownColumn) => void sortBy(column)}
              {focus}
              onrowkeydown={rowKeys}
              {selection}
              onselected={announceSelection}
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
      <!-- The phone's fixed sheet (§8.1): the list ends with room for it, so its last rows and
           «Показать ещё» stay reachable above the sheet. -->
      {#if !selection.empty && narrow.current}
        <div class="sheet-space" aria-hidden="true"></div>
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

<CommandDialog
  bind:open={commanding}
  preset={commandPreset}
  issues={commandTarget === 'selection' && !selection.allMatching ? chosenIssues : []}
  bulk={commandTarget === 'query' || selection.allMatching
    ? {
        query: q,
        project: contextProject,
        hideResolved,
        count: shownCount ?? 0,
      }
    : null}
  onclose={() => {
    commanding = false;
  }}
/>

<!-- The toolbar's pickers (§8.3): a choice becomes the line the dialog above previews. -->
<SelectionPicker
  bind:open={picking}
  mode={pickerKind}
  assigneeName={assigneeFieldName}
  onchosen={(line: string) => {
    openCommands('selection', line);
  }}
/>

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

  /* The selection mode of §8.1: the card's checkbox before it, on the row's top line. */
  .row.choosing {
    display: flex;
    align-items: flex-start;
    gap: var(--space-2);
    padding-inline-start: var(--space-2);
  }

  .row.choosing .choose {
    /* A 44 px target the thumb reaches (D-019); the card keeps its own padding. */
    min-width: 44px;
    min-height: 44px;
    margin: var(--space-1) 0 0;
    accent-color: var(--color-ink);
  }

  .more,
  .loading {
    display: flex;
    justify-content: center;
  }

  /* The sheet publishes its height on the body; the spacer matches it (§8.1). */
  .sheet-space {
    height: var(--selection-sheet, 0px);
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
