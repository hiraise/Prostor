<script lang="ts">
  // The fields of the instance (FLD-015…FLD-025, FLD-070): in the instance order with type, values,
  // aliases, badges for built-in fields, fields added to new projects and fields private by
  // default, and how many projects use each. Search by name or alias and "Show only unused".
  // System administrators create fields and change the order — by dragging or by "Move up/down" in
  // each row's menu (FLD-022) — while no filter hides fields.
  import ArrowDown from '@lucide/svelte/icons/arrow-down';
  import ArrowUp from '@lucide/svelte/icons/arrow-up';
  import Ellipsis from '@lucide/svelte/icons/ellipsis';
  import KeyRound from '@lucide/svelte/icons/key-round';
  import Lock from '@lucide/svelte/icons/lock';
  import Plus from '@lucide/svelte/icons/plus';
  import SearchX from '@lucide/svelte/icons/search-x';
  import Sparkles from '@lucide/svelte/icons/sparkles';
  import { untrack } from 'svelte';
  import { resolve } from '$app/paths';

  import type { Field } from '#lib/api/schema.gen.ts';
  import { can, canSeeFields } from '#lib/session/index.ts';
  import type { ListHandle } from '#lib/store/index.ts';
  import { announce } from '#lib/ui/announce.svelte.ts';
  import Button from '#lib/ui/Button.svelte';
  import Checkbox from '#lib/ui/Checkbox.svelte';
  import EmptyState from '#lib/ui/EmptyState.svelte';
  import ErrorState from '#lib/ui/ErrorState.svelte';
  import ForbiddenState from '#lib/ui/ForbiddenState.svelte';
  import IconButton from '#lib/ui/IconButton.svelte';
  import Menu, { type MenuEntry } from '#lib/ui/Menu.svelte';
  import PageHeader from '#lib/ui/PageHeader.svelte';
  import TextField from '#lib/ui/TextField.svelte';
  import { showError, toasts } from '#lib/ui/toast.svelte.ts';

  import Badge from './Badge.svelte';
  import CreateFieldDialog from './CreateFieldDialog.svelte';
  import {
    afterMovingDown,
    afterMovingUp,
    ensureFields,
    fieldRow,
    moveField,
    reorder,
    type FieldFilters,
  } from './fields.ts';
  import * as m from './messages.ts';
  import { typeLabel } from './problems.ts';
  import ReorderList from './ReorderList.svelte';

  interface Props {
    /** The list for `filters`; `null` when the viewer may not see fields. */
    fields: ListHandle<Field> | null;
    filters?: FieldFilters;
    /** A field was created: open its page. */
    oncreated: (field: Field) => unknown;
    /** The filters changed: keep them in the address (UI-006). */
    onfilter?: (filters: FieldFilters) => void;
    /** Where "forbidden" offers to go. */
    home?: string;
  }

  let { fields, filters = { q: '', unused: false }, oncreated, onfilter, home }: Props = $props();

  /** Typing pauses this long before the list is searched. */
  const DEBOUNCE_MS = 250;

  let text = $state(untrack(() => filters.q));
  let unused = $state(untrack(() => filters.unused));
  let list = $state.raw(untrack(() => fields));
  let searching = $state(false);
  let searchError = $state<unknown>(null);
  let creating = $state(false);
  /** The order shown while a move is being saved. */
  let pending = $state<number[] | null>(null);
  let timer: ReturnType<typeof setTimeout> | undefined;
  let request = 0;

  const mayWrite = $derived(can('system.write'));
  const filtered = $derived(text.trim() !== '' || unused);
  const sortable = $derived(mayWrite && !filtered && list?.hasMore === false);
  const ids = $derived(pending ?? list?.ids ?? []);
  const items = $derived(
    ids.map((id) => fieldRow(id)).filter((field): field is Field => field !== undefined),
  );

  function current(): FieldFilters {
    return { q: text.trim(), unused };
  }

  async function search(): Promise<void> {
    clearTimeout(timer);
    const wanted = current();
    onfilter?.(wanted);
    const mine = ++request;
    searching = true;
    try {
      const found = await ensureFields(wanted);
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

  async function move(field: Field, after: number | null | undefined): Promise<void> {
    const shown = list;
    if (after === undefined || !shown) return;
    pending = reorder(shown.ids, (id) => id, field.id, after);
    try {
      await moveField(field.id, after);
      await shown.refresh();
      const order = shown.ids;
      announce(
        m.moved({ name: field.name, position: order.indexOf(field.id) + 1, total: order.length }),
      );
    } catch (error) {
      showError(error);
    } finally {
      pending = null;
    }
  }

  function menuOf(field: Field): MenuEntry[] {
    const up = afterMovingUp(ids, field.id);
    const down = afterMovingDown(ids, field.id);
    return [
      {
        type: 'action',
        label: m.moveUp(),
        icon: ArrowUp,
        disabled: up === undefined,
        onSelect: () => void move(field, up),
      },
      {
        type: 'action',
        label: m.moveDown(),
        icon: ArrowDown,
        disabled: down === undefined,
        onSelect: () => void move(field, down),
      },
    ];
  }

  async function created(field: Field): Promise<void> {
    toasts.show({ tone: 'success', message: m.fieldCreated() });
    void list?.refresh().catch(() => undefined);
    await oncreated(field);
  }
</script>

<svelte:head><title>{m.fieldsTitle()} · TaskMaster</title></svelte:head>

{#if !fields || !canSeeFields()}
  <ForbiddenState {home} />
{:else}
  <PageHeader title={m.fieldsTitle()}>
    {#snippet actions()}
      {#if mayWrite}
        <Button
          variant="primary"
          icon={Plus}
          onclick={() => {
            creating = true;
          }}
        >
          {m.createField()}
        </Button>
      {/if}
    {/snippet}
  </PageHeader>

  <div class="page">
    <p class="intro">{m.fieldsIntro()}</p>

    <search class="filters">
      <TextField
        class="find"
        type="search"
        label={m.findFields()}
        hint={m.findFieldsHint()}
        bind:value={text}
        oninput={typed}
        onkeydown={(event: KeyboardEvent) => {
          if (event.key === 'Enter') void search();
        }}
        autocomplete="off"
        maxlength={100}
      />
      <Checkbox
        class="unused"
        label={m.onlyUnused()}
        checked={unused}
        onchange={(event: Event & { currentTarget: HTMLInputElement }) => {
          unused = event.currentTarget.checked;
          void search();
        }}
      />
    </search>
    {#if mayWrite && filtered}<p class="note">{m.reorderHint()}</p>{/if}

    {#if searchError && !searching}
      <ErrorState error={searchError} level={2} onretry={search} />
    {:else if list}
      {#if list.error && list.ids.length === 0}
        <ErrorState error={list.error} level={2} onretry={() => list?.refresh()} />
      {:else if list.ids.length === 0}
        <EmptyState icon={SearchX} title={m.noFieldsMatch()} description={m.noFieldsMatchText()} />
      {:else}
        <ReorderList
          {items}
          key={(field: Field) => field.id}
          label={m.fieldList()}
          {sortable}
          busy={searching || list.refreshing || pending !== null}
          onmove={(id: number, after: number | null) => {
            const field = fieldRow(id);
            if (field) void move(field, after);
          }}
        >
          {#snippet row(field: Field)}
            <div class="field">
              <div class="line">
                <span class="title">
                  <a
                    class="name"
                    href={resolve('/(app)/admin/fields/[id]', { id: String(field.id) })}
                    aria-describedby="field-{field.id}-details"
                  >
                    {field.name}
                  </a>
                  {#if field.builtin}<Badge icon={Lock} text={m.builtIn()} />{/if}
                  {#if field.auto_attach}<Badge icon={Sparkles} text={m.autoAttachBadge()} />{/if}
                  {#if field.private}<Badge icon={KeyRound} text={m.privateByDefault()} />{/if}
                </span>
                {#if sortable}
                  <Menu label={m.fieldActions({ name: field.name })} items={menuOf(field)}>
                    {#snippet trigger(props)}
                      <IconButton
                        icon={Ellipsis}
                        label={m.fieldActions({ name: field.name })}
                        {...props}
                      />
                    {/snippet}
                  </Menu>
                {/if}
              </div>
              <div id="field-{field.id}-details" class="details">
                <p>
                  {[typeLabel(field), field.set ? m.setLine({ name: field.set.name }) : null]
                    .filter(Boolean)
                    .join(' · ')}
                </p>
                {#if field.aliases.length > 0}
                  <p>{m.aliasesLine({ list: field.aliases.join(', ') })}</p>
                {/if}
                <p class="count">
                  {field.project_count > 0
                    ? m.usedInCount({ count: field.project_count })
                    : m.notUsed()}
                </p>
              </div>
            </div>
          {/snippet}
        </ReorderList>
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

  {#if mayWrite}<CreateFieldDialog bind:open={creating} oncreated={created} />{/if}
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

  .note {
    color: var(--color-text-muted);
    font-size: var(--text-sm);
  }

  .filters {
    display: flex;
    flex-wrap: wrap;
    align-items: flex-start;
    gap: var(--space-3) var(--space-6);
  }

  .filters :global(.find) {
    flex: 1 1 16rem;
    max-width: 24rem;
  }

  .filters :global(.unused) {
    padding-top: var(--space-6);
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: var(--space-0_5);
    padding: var(--space-1) var(--space-2) var(--space-2) var(--space-3);
  }

  .line {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }

  .title {
    display: flex;
    flex: 1;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-1) var(--space-2);
    min-width: 0;
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

  .name:hover {
    color: var(--color-link);
    text-decoration: underline;
  }

  .details {
    display: flex;
    flex-direction: column;
    gap: var(--space-0_5);
    color: var(--color-text-muted);
    font-size: var(--text-sm);
    overflow-wrap: anywhere;
  }

  .count {
    font-variant-numeric: tabular-nums;
  }

  @media (width < 48rem) {
    .page {
      padding: var(--space-2) var(--space-4) var(--space-8);
    }

    .filters :global(.find) {
      max-width: none;
    }

    .filters :global(.unused) {
      padding-top: 0;
    }
  }
</style>
