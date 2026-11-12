<script lang="ts">
  // The issue list's sidebar (QRY-082): «Черновики» (only when there are some, ISS-011),
  // «Проекты», «Теги», «Сохранённые поиски», each with its issue count and each folding. On wide
  // screens it is always shown, shown on pointing or focus (`auto`), or hidden — the person's
  // choice, as is its width. On phones it is a drawer from the leading edge. Tags and saved
  // searches have «Изменить», «Общий доступ…» and «Удалить» as their rights allow (ISS-036).
  import ChevronDown from '@lucide/svelte/icons/chevron-down';
  import ChevronRight from '@lucide/svelte/icons/chevron-right';
  import Ellipsis from '@lucide/svelte/icons/ellipsis';
  import PanelLeft from '@lucide/svelte/icons/panel-left';
  import Trash2 from '@lucide/svelte/icons/trash-2';
  import { untrack, type Snippet } from 'svelte';
  import { MediaQuery } from 'svelte/reactivity';

  import type { CountItem, SavedSearch, SidebarSection, Tag } from '#lib/api/schema.gen.ts';
  import { preferences } from '#lib/session/index.ts';
  import { NARROW } from '#lib/theme/breakpoints.ts';
  import Button from '#lib/ui/Button.svelte';
  import Dialog from '#lib/ui/Dialog.svelte';
  import IconButton from '#lib/ui/IconButton.svelte';
  import Menu, { type MenuEntry } from '#lib/ui/Menu.svelte';
  import Resizer from '#lib/ui/Resizer.svelte';
  import { showError, toasts } from '#lib/ui/toast.svelte.ts';

  import { listHref, type ListContext, type ResolvedContext } from '../list/list-state.ts';
  import { newIssueDialog, openNewIssue } from '../new/open.svelte.ts';
  import * as m from './messages.ts';
  import SearchDialog from './SearchDialog.svelte';
  import SharingDialog from './SharingDialog.svelte';
  import {
    deleteAllDrafts,
    deleteDraft,
    deleteSearch,
    deleteTag,
    loadCounts,
    loadSidebar,
    shownCount,
    tagUsage,
    countKey,
    type CountKey,
    type Shareable,
    type SidebarData,
  } from './sidebar.ts';
  import TagDialog from './TagDialog.svelte';

  interface Props {
    context: ResolvedContext;
    hideResolved: boolean;
    /** The phone drawer is open. */
    drawer?: boolean;
    /** Grows when the list is reloaded after live changes: the counts are fetched again. */
    revision?: number;
    /** The shown context was deleted here: the list goes elsewhere. */
    onchoose: (context: ListContext) => void;
  }

  let {
    context,
    hideResolved,
    drawer = $bindable(false),
    revision = 0,
    onchoose,
  }: Props = $props();

  /** Width limits, rem × 10 (the preference's range). */
  const MIN = 160;
  const MAX = 400;

  const id = $props.id();
  const narrow = new MediaQuery(NARROW, false);
  const mode = $derived(preferences.value('list_sidebar'));
  const folded = $derived(preferences.value('list_sidebar_folded'));
  let dragged = $state<number | null>(null);
  const width = $derived(dragged ?? preferences.value('list_sidebar_width'));
  /** `auto` mode shown by a tap on its button (touch has no pointing). */
  let peek = $state(false);

  let data = $state<SidebarData | null>(null);
  let counts = $state<Map<CountKey, CountItem>>(new Map());
  let failed = $state(false);

  async function reload(): Promise<void> {
    try {
      const next = await loadSidebar();
      data = next;
      failed = false;
      counts = await loadCounts(next);
    } catch {
      if (data === null) failed = true;
    }
  }

  $effect(() => {
    void reload();
  });

  // A search saved from the list is shown at once: an unknown shown search reloads the lists once.
  let soughtSearch: number | null = null;
  $effect(() => {
    if (context.kind !== 'search' || data === null || context.id === soughtSearch) return;
    const wanted = context.id;
    if (data.searches.some((search) => search.id === wanted)) return;
    soughtSearch = wanted;
    void untrack(() => reload());
  });

  // Live counts: the list's «Обновить» recounts what the sidebar shows.
  let countedRevision = untrack(() => revision);
  $effect(() => {
    const wanted = revision;
    if (wanted === countedRevision) return;
    countedRevision = wanted;
    const shown = untrack(() => data);
    if (shown === null) return;
    loadCounts(shown)
      .then((next) => {
        counts = next;
      })
      .catch(() => undefined);
  });

  // A closed new-issue form may have kept or deleted a draft.
  let formWasOpen = false;
  $effect(() => {
    const open = newIssueDialog.open;
    if (formWasOpen && !open) void reload();
    formWasOpen = open;
  });

  function count(key: CountKey): number | null {
    return shownCount(counts.get(key), hideResolved);
  }

  function isCurrent(kind: ResolvedContext['kind'], itemId: number): boolean {
    return context.kind === kind && context.kind !== 'all' && context.id === itemId;
  }

  function toggle(section: SidebarSection): void {
    const next = folded.includes(section)
      ? folded.filter((s) => s !== section)
      : [...folded, section];
    preferences.set({ list_sidebar_folded: next });
  }

  function resize(value: number, done: boolean): void {
    if (!done) {
      dragged = value;
      return;
    }
    dragged = null;
    preferences.set({ list_sidebar_width: value });
  }

  function chosen(): void {
    drawer = false;
    peek = false;
  }

  // ---- Drafts (ISS-011) ----

  let confirmingDrafts = $state(false);
  let deletingDrafts = $state(false);

  function summaryOf(summary: string): string {
    return summary.trim() || m.noSummary();
  }

  async function removeDraft(draft: number): Promise<void> {
    try {
      await deleteDraft(draft);
      if (data) data.drafts = data.drafts.filter((d) => d.id !== draft);
      toasts.show({ tone: 'success', message: m.draftDeleted() });
    } catch (error) {
      showError(error);
    }
  }

  async function removeAllDrafts(): Promise<void> {
    deletingDrafts = true;
    try {
      await deleteAllDrafts();
      if (data) data.drafts = [];
      confirmingDrafts = false;
      toasts.show({ tone: 'success', message: m.draftsDeleted() });
    } catch (error) {
      showError(error);
    } finally {
      deletingDrafts = false;
    }
  }

  // ---- Tags and saved searches ----

  let editingTag = $state<Tag | null>(null);
  let tagOpen = $state(false);
  let editingSearch = $state<SavedSearch | null>(null);
  let searchOpen = $state(false);
  let sharing = $state<Shareable | null>(null);
  let sharingOpen = $state(false);
  let removing = $state<
    { kind: 'tag'; tag: Tag; usage: number | null } | { kind: 'search'; search: SavedSearch } | null
  >(null);
  let removeOpen = $state(false);
  let removeBusy = $state(false);

  function tagActions(tag: Tag): MenuEntry[] {
    const items: MenuEntry[] = [];
    if (tag.abilities.edit) {
      items.push({
        type: 'action',
        label: m.edit(),
        onSelect: () => {
          editingTag = tag;
          tagOpen = true;
        },
      });
    }
    if (tag.abilities.share) {
      items.push({
        type: 'action',
        label: m.share(),
        onSelect: () => {
          sharing = { kind: 'tag', tag };
          sharingOpen = true;
        },
      });
    }
    if (tag.abilities.delete) {
      items.push({
        type: 'action',
        label: m.remove(),
        icon: Trash2,
        danger: true,
        onSelect: () => {
          removing = { kind: 'tag', tag, usage: null };
          removeOpen = true;
          tagUsage(tag.id).then(
            (usage) => {
              if (removing?.kind === 'tag' && removing.tag.id === tag.id) {
                removing = { ...removing, usage };
              }
            },
            () => undefined,
          );
        },
      });
    }
    return items;
  }

  function searchActions(search: SavedSearch): MenuEntry[] {
    const items: MenuEntry[] = [];
    if (search.abilities.edit) {
      items.push({
        type: 'action',
        label: m.edit(),
        onSelect: () => {
          editingSearch = search;
          searchOpen = true;
        },
      });
    }
    if (search.abilities.share) {
      items.push({
        type: 'action',
        label: m.share(),
        onSelect: () => {
          sharing = { kind: 'search', search };
          sharingOpen = true;
        },
      });
    }
    if (search.abilities.delete) {
      items.push({
        type: 'action',
        label: m.remove(),
        icon: Trash2,
        danger: true,
        onSelect: () => {
          removing = { kind: 'search', search };
          removeOpen = true;
        },
      });
    }
    return items;
  }

  async function confirmRemove(): Promise<void> {
    const target = removing;
    if (!target) return;
    removeBusy = true;
    try {
      if (target.kind === 'tag') {
        await deleteTag(target.tag.id);
        toasts.show({ tone: 'success', message: m.tagDeleted() });
      } else {
        await deleteSearch(target.search.id);
        toasts.show({ tone: 'success', message: m.searchDeleted() });
      }
      removeOpen = false;
      const shownId = target.kind === 'tag' ? target.tag.id : target.search.id;
      if (isCurrent(target.kind, shownId)) onchoose({ kind: 'all' });
      await reload();
    } catch (error) {
      showError(error);
    } finally {
      removeBusy = false;
    }
  }

  const removeTitle = $derived(
    removing === null
      ? ''
      : removing.kind === 'tag'
        ? m.deleteTagTitle({ name: removing.tag.name })
        : m.deleteSearchTitle({ name: removing.search.name }),
  );
</script>

{#snippet countOf(shown: number | null)}
  {#if shown !== null}<span class="count" aria-hidden="true">{shown}</span>{/if}
{/snippet}

{#snippet link(
  target: ListContext,
  current: boolean,
  label: string,
  key: CountKey,
  color?: number | null,
)}
  {@const href = listHref({ q: '', context: target })}
  {@const shown = count(key)}
  <!-- listHref resolves the address. -->
  <!-- eslint-disable svelte/no-navigation-without-resolve -->
  <a
    {href}
    class="link"
    aria-label={shown === null ? label : `${label}, ${m.count({ count: shown })}`}
    aria-current={current ? 'page' : undefined}
    onclick={chosen}
  >
    {#if color !== undefined}
      <span
        class={['dot', { plain: color === null || color < 0 || color > 11 }]}
        style:--dot={color === null ? undefined : `var(--value-${String(color)}-fg)`}
        aria-hidden="true"
      ></span>
    {/if}
    <span class="name">{label}</span>
    {@render countOf(shown)}
  </a>
  <!-- eslint-enable svelte/no-navigation-without-resolve -->
{/snippet}

{#snippet actions(name: string, items: MenuEntry[])}
  {#if items.length > 0}
    <Menu label={m.actionsOf({ name })} {items} placement={{ side: 'bottom', align: 'end' }}>
      {#snippet trigger(props)}
        <IconButton icon={Ellipsis} label={m.actionsOf({ name })} size="sm" {...props} />
      {/snippet}
    </Menu>
  {/if}
{/snippet}

{#snippet group(section: SidebarSection, title: string, body: Snippet)}
  {@const open = !folded.includes(section)}
  <section class="group" aria-labelledby="{id}-{section}">
    <h2 class="heading">
      <button
        type="button"
        id="{id}-{section}"
        class="fold"
        aria-expanded={open}
        aria-controls="{id}-{section}-items"
        onclick={() => {
          toggle(section);
        }}
      >
        {#if open}<ChevronDown size={16} aria-hidden="true" />{:else}<ChevronRight
            size={16}
            aria-hidden="true"
          />{/if}
        {title}
      </button>
    </h2>
    <div id="{id}-{section}-items" hidden={!open}>{@render body()}</div>
  </section>
{/snippet}

{#snippet draftsBody()}
  <ul class="items">
    {#each data?.drafts ?? [] as draft (draft.id)}
      {@const summary = summaryOf(draft.summary)}
      <li class="item">
        <button
          type="button"
          class="link"
          aria-label={m.openDraft({ summary })}
          onclick={() => {
            chosen();
            openNewIssue({ draft: draft.id });
          }}
        >
          <span class="name">{summary}</span>
        </button>
        <IconButton
          icon={Trash2}
          label={m.deleteDraft({ summary })}
          size="sm"
          onclick={() => void removeDraft(draft.id)}
        />
      </li>
    {/each}
  </ul>
  <div class="all">
    <Button
      variant="ghost"
      size="sm"
      onclick={() => {
        confirmingDrafts = true;
      }}
    >
      {m.deleteAllDrafts()}
    </Button>
  </div>
{/snippet}

{#snippet projectsBody()}
  {#if (data?.projects.length ?? 0) === 0}
    <p class="none">{m.noProjects()}</p>
  {:else}
    <ul class="items">
      {#each data?.projects ?? [] as project (project.id)}
        <li class="item">
          {@render link(
            { kind: 'project', key: project.key },
            isCurrent('project', project.id),
            project.name,
            countKey('project', project.id),
          )}
        </li>
      {/each}
    </ul>
  {/if}
{/snippet}

{#snippet tagsBody()}
  {#if (data?.tags.length ?? 0) === 0}
    <p class="none">{m.noTags()}</p>
  {:else}
    <ul class="items">
      {#each data?.tags ?? [] as tag (tag.id)}
        <li class="item">
          {@render link(
            { kind: 'tag', id: tag.id },
            isCurrent('tag', tag.id),
            tag.name,
            countKey('tag', tag.id),
            tag.color,
          )}
          {@render actions(tag.name, tagActions(tag))}
        </li>
      {/each}
    </ul>
  {/if}
{/snippet}

{#snippet searchesBody()}
  {#if (data?.searches.length ?? 0) === 0}
    <p class="none">{m.noSearches()}</p>
  {:else}
    <ul class="items">
      {#each data?.searches ?? [] as search (search.id)}
        <li class="item">
          {@render link(
            { kind: 'search', id: search.id },
            isCurrent('search', search.id),
            search.name,
            countKey('search', search.id),
          )}
          {@render actions(search.name, searchActions(search))}
        </li>
      {/each}
    </ul>
  {/if}
{/snippet}

{#snippet content()}
  {#if failed}
    <div class="failed">
      <p>{m.loadFailed()}</p>
      <Button variant="secondary" size="sm" onclick={() => void reload()}>{m.retry()}</Button>
    </div>
  {:else if data}
    {#if data.drafts.length > 0}{@render group('drafts', m.drafts(), draftsBody)}{/if}
    {@render group('projects', m.projects(), projectsBody)}
    {@render group('tags', m.tags(), tagsBody)}
    {@render group('searches', m.searches(), searchesBody)}
  {/if}
{/snippet}

{#if narrow.current}
  <Dialog bind:open={drawer} side="start" title={m.sidebar()}>
    <nav class="drawer" aria-label={m.sidebar()}>{@render content()}</nav>
  </Dialog>
{:else if mode !== 'closed'}
  <div
    class={['sidebar', mode, { peek }]}
    style:--sidebar-width="{width / 10}rem"
    onmouseleave={() => {
      peek = false;
    }}
    role="presentation"
  >
    {#if mode === 'auto'}
      <div class="strip">
        <IconButton
          icon={PanelLeft}
          label={m.sidebar()}
          aria-expanded={peek}
          onclick={() => {
            peek = !peek;
          }}
        />
      </div>
    {/if}
    <nav class="panel" aria-label={m.sidebar()}>{@render content()}</nav>
    {#if mode === 'open'}
      <div class="handle">
        <Resizer
          value={width}
          min={MIN}
          max={MAX}
          side="start"
          label={m.sidebarWidth()}
          onchange={resize}
        />
      </div>
    {/if}
  </div>
{/if}

<Dialog
  bind:open={confirmingDrafts}
  title={m.deleteAllTitle()}
  size="sm"
  dismissible={!deletingDrafts}
>
  <p>{m.deleteAllText({ count: data?.drafts.length ?? 0 })}</p>
  {#snippet footer()}
    <Button
      variant="ghost"
      disabled={deletingDrafts}
      onclick={() => {
        confirmingDrafts = false;
      }}>{m.cancel()}</Button
    >
    <Button variant="danger" loading={deletingDrafts} onclick={() => void removeAllDrafts()}>
      {m.deleteAllDrafts()}
    </Button>
  {/snippet}
</Dialog>

<Dialog bind:open={removeOpen} title={removeTitle} size="sm" dismissible={!removeBusy}>
  {#if removing?.kind === 'tag'}
    <p aria-live="polite">{removing.usage === null ? '' : m.tagUsage({ count: removing.usage })}</p>
  {:else if removing?.kind === 'search'}
    <p>{m.deleteSearchText()}</p>
  {/if}
  {#snippet footer()}
    <Button
      variant="ghost"
      disabled={removeBusy}
      onclick={() => {
        removeOpen = false;
      }}>{m.cancel()}</Button
    >
    <Button variant="danger" loading={removeBusy} onclick={() => void confirmRemove()}>
      {m.remove()}
    </Button>
  {/snippet}
</Dialog>

<TagDialog bind:open={tagOpen} tag={editingTag} onsaved={reload} />
<SearchDialog bind:open={searchOpen} search={editingSearch} onsaved={reload} />
<SharingDialog bind:open={sharingOpen} item={sharing} onsaved={reload} />

<style>
  .sidebar {
    position: relative;
    display: flex;
    flex: none;
    align-self: stretch;
  }

  .sidebar.open {
    width: var(--sidebar-width);
  }

  .panel {
    width: 100%;
    max-height: calc(100dvh - var(--space-8));
    overflow-y: auto;
    padding: var(--space-3) var(--space-2) var(--space-6);
    border-inline-end: 1px solid var(--color-border);
    background: var(--color-surface-sunken);
  }

  .open .panel {
    position: sticky;
    top: 0;
  }

  .handle {
    display: flex;
    position: absolute;
    inset-block: 0;
    inset-inline-end: 0;
    translate: 50% 0;
    z-index: 1;
  }

  /* Shown on pointing, on focus inside, or after a tap on its button. */
  .sidebar.auto {
    width: calc(var(--control-md) + var(--space-2));
  }

  .strip {
    padding: var(--space-2) var(--space-1);
  }

  .auto .panel {
    position: absolute;
    inset-block-start: 0;
    inset-inline-start: 100%;
    z-index: 10;
    width: var(--sidebar-width);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-lg);
    box-shadow: var(--shadow-lg);
    visibility: hidden;
    opacity: 0;
    transition:
      opacity 160ms ease-out,
      visibility 160ms;
  }

  .auto:hover .panel,
  .auto:focus-within .panel,
  .auto.peek .panel {
    visibility: visible;
    opacity: 1;
  }

  .group + .group {
    margin-top: var(--space-3);
  }

  .heading {
    font-size: var(--text-sm);
  }

  .fold {
    display: flex;
    align-items: center;
    gap: var(--space-1);
    width: 100%;
    min-height: var(--control-sm);
    padding: 0 var(--space-2);
    border: 0;
    border-radius: var(--radius-md);
    background: none;
    color: var(--color-text-muted);
    font-weight: var(--weight-medium);
    cursor: pointer;
  }

  .fold:hover {
    background: var(--color-hover);
  }

  .item {
    display: flex;
    align-items: center;
    gap: var(--space-1);
  }

  .link {
    display: flex;
    flex: 1;
    align-items: center;
    gap: var(--space-2);
    min-width: 0;
    min-height: var(--control-sm);
    padding: 0 var(--space-2) 0 var(--space-6);
    border: 0;
    border-radius: var(--radius-md);
    background: none;
    color: var(--color-text);
    font: inherit;
    text-align: start;
    text-decoration: none;
    cursor: pointer;
  }

  .link:hover {
    background: var(--color-hover);
  }

  .link[aria-current='page'] {
    background: var(--color-selected);
    font-weight: var(--weight-medium);
  }

  .name {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .count {
    color: var(--color-text-muted);
    font-size: var(--text-sm);
    font-variant-numeric: tabular-nums;
  }

  .dot {
    flex: none;
    width: 0.625rem;
    height: 0.625rem;
    border-radius: 50%;
    background: var(--dot);
  }

  .dot.plain {
    border: 1px solid var(--color-border-control);
    background: none;
  }

  .none {
    padding: var(--space-1) var(--space-2) var(--space-1) var(--space-6);
    color: var(--color-text-muted);
    font-size: var(--text-sm);
  }

  .all {
    padding-inline-start: var(--space-4);
  }

  .failed {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: var(--space-2);
    padding: var(--space-2);
    font-size: var(--text-sm);
  }

  /* Touch targets on phones (design §11). */
  .drawer .link,
  .drawer .fold {
    min-height: 2.75rem;
  }

  @media (prefers-reduced-motion: reduce) {
    .auto .panel {
      transition: none;
    }
  }
</style>
