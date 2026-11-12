<script lang="ts">
  // «Колонки» (BRD-042…BRD-046, §3.7): the field whose values are the columns, the columns in
  // order (drag or «Выше»/«Ниже»), their limits, renaming a value and adding a column from a
  // value of the field or from a new value. A new or renamed value changes the set in every
  // project using it, so both ask first and name those projects (D-013).
  import EllipsisVertical from '@lucide/svelte/icons/ellipsis-vertical';

  import type { Board, BoardColumn, ProjectField, UsedIn, ValueSet } from '#lib/api/schema.gen.ts';
  import {
    ReorderList,
    ValueChip,
    ensureValueSet,
    fetchProjectFields,
    isFullSet,
    refusalText,
  } from '#lib/features/fields/index.ts';
  import Button from '#lib/ui/Button.svelte';
  import Dialog from '#lib/ui/Dialog.svelte';
  import IconButton from '#lib/ui/IconButton.svelte';
  import Menu, { type MenuEntry } from '#lib/ui/Menu.svelte';
  import Notice from '#lib/ui/Notice.svelte';
  import Select from '#lib/ui/Select.svelte';
  import TextField from '#lib/ui/TextField.svelte';
  import { toasts } from '#lib/ui/toast.svelte.ts';

  import { addColumn, deleteColumn, updateColumn } from '../boards.ts';
  import * as m from '../messages.ts';
  import { saveBoard } from './settings.ts';

  interface Props {
    board: Board;
    busy?: boolean;
    onchanged: () => unknown;
  }

  let { board, busy = $bindable(false), onchanged }: Props = $props();

  /** Kinds whose single value can be a column (BRD-042). */
  const COLUMN_KINDS = new Set(['state', 'enum', 'owned', 'version', 'build']);
  const NAME_MAX = 100;

  const uid = $props.id();
  /** Fields of the board projects, by project. */
  let attached = $state<{ project: number; fields: ProjectField[] }[]>([]);
  let sets = $state<ValueSet[]>([]);
  let problem = $state<string | null>(null);
  /** The chosen field; follows the stored board. */
  let fieldChoice = $derived(board.column_field ? String(board.column_field.id) : '');
  let pendingField = $state<string | null>(null);
  let addValue = $state('');
  let newName = $state('');
  let newNameError = $state<string | null>(null);
  let confirmNew = $state<string | null>(null);
  let renaming = $state<BoardColumn | null>(null);
  let renameText = $state('');
  let renameError = $state<string | null>(null);

  // The fields of every board project, once per change of the projects.
  $effect(() => {
    const projects = board.projects.map((one) => one.project.id).sort((a, b) => a - b);
    const controller = new AbortController();
    Promise.all(
      projects.map((project) =>
        fetchProjectFields(project, controller.signal).then((found) => ({
          project,
          fields: found.items,
        })),
      ),
    ).then(
      (found) => {
        attached = found;
      },
      () => {
        if (!controller.signal.aborted) problem = m.fieldsFailed();
      },
    );
    return () => {
      controller.abort();
    };
  });

  const candidates = $derived.by(() => {
    const found: Record<number, ProjectField['field']> = {};
    for (const { fields } of attached) {
      for (const one of fields) {
        if (COLUMN_KINDS.has(one.field.kind) && !one.field.multi) found[one.field.id] = one.field;
      }
    }
    if (board.column_field) found[board.column_field.id] = board.column_field;
    return Object.values(found).sort((a, b) => a.name.localeCompare(b.name));
  });

  /** The sets the column field uses in the board projects, the first project's first. */
  const setIds = $derived.by(() => {
    const field = board.column_field?.id;
    const ids: number[] = [];
    for (const { fields } of attached) {
      const set = fields.find((one) => one.field.id === field)?.set;
      if (set && !ids.includes(set.id)) ids.push(set.id);
    }
    return ids;
  });

  $effect(() => {
    const ids = setIds;
    let stale = false;
    Promise.all(ids.map((id) => ensureValueSet(id))).then(
      (handles) => {
        if (stale) return;
        sets = handles
          .map((handle) => handle.current)
          .filter((set): set is ValueSet => isFullSet(set));
      },
      () => undefined,
    );
    return () => {
      stale = true;
    };
  });

  const offered = $derived.by(() => {
    const used = new Set(board.columns.map((column) => column.value.id));
    const found: { value: string; label: string }[] = [];
    for (const set of sets) {
      for (const value of set.values) {
        const key = String(value.id);
        if (value.archived || used.has(value.id) || found.some((one) => one.value === key))
          continue;
        found.push({ value: key, label: value.name });
      }
    }
    return found;
  });

  function usedIn(set: ValueSet | undefined): string {
    const used: UsedIn = set?.used_in ?? { items: [], hidden: 0 };
    return used.items.map((one) => one.name).join(', ');
  }

  const newSet = $derived(sets.find((set) => set.id === setIds[0]));
  const renameSet = $derived(
    renaming
      ? sets.find((set) => set.values.some((one) => one.id === renaming?.value.id))
      : undefined,
  );

  async function run(action: () => Promise<unknown>): Promise<boolean> {
    problem = null;
    busy = true;
    try {
      await action();
      await onchanged();
      return true;
    } catch (error) {
      problem = refusalText(error);
      return false;
    } finally {
      busy = false;
    }
  }

  function chooseField(value: string): void {
    if (value === (board.column_field ? String(board.column_field.id) : '')) return;
    fieldChoice = value;
    if (board.columns.length > 0) {
      pendingField = value;
      return;
    }
    void changeField(value);
  }

  async function changeField(value: string): Promise<void> {
    pendingField = null;
    busy = true;
    try {
      const found = await saveBoard(board, { column_field: value === '' ? null : Number(value) });
      if (found) {
        problem = found.message ?? Object.values(found.fields).join(' ');
        fieldChoice = board.column_field ? String(board.column_field.id) : '';
        return;
      }
      await onchanged();
    } finally {
      busy = false;
    }
  }

  function move(id: number, after: number | null): void {
    void run(() => updateColumn(board.id, id, { after }));
  }

  function neighbour(column: BoardColumn, step: -1 | 1): number | null | undefined {
    const ids = board.columns.map((one) => one.id);
    const index = ids.indexOf(column.id);
    if (step === -1) return index <= 0 ? undefined : index === 1 ? null : ids[index - 2];
    return index >= ids.length - 1 ? undefined : ids[index + 1];
  }

  function actions(column: BoardColumn): MenuEntry[] {
    const up = neighbour(column, -1);
    const down = neighbour(column, 1);
    return [
      {
        type: 'action',
        label: m.moveUp(),
        disabled: up === undefined,
        onSelect: () => {
          if (up !== undefined) move(column.id, up);
        },
      },
      {
        type: 'action',
        label: m.moveDown(),
        disabled: down === undefined,
        onSelect: () => {
          if (down !== undefined) move(column.id, down);
        },
      },
      {
        type: 'action',
        label: m.rename(),
        onSelect: () => {
          renaming = column;
          renameText = column.value.name;
          renameError = null;
        },
      },
      { type: 'separator' },
      {
        type: 'action',
        label: m.removeColumn(),
        danger: true,
        onSelect: () => {
          void run(() => deleteColumn(board.id, column.id)).then((done) => {
            if (done) toasts.show({ message: m.columnRemoved({ name: column.value.name }) });
          });
        },
      },
    ];
  }

  function limit(text: string): number | null | false {
    if (text.trim() === '') return null;
    const value = Number(text);
    return Number.isInteger(value) && value >= 0 ? value : false;
  }

  function setLimit(column: BoardColumn, which: 'wip_min' | 'wip_max', text: string): void {
    const value = limit(text);
    const other = which === 'wip_min' ? column.wip_max : column.wip_min;
    const [min, max] = which === 'wip_min' ? [value, other] : [other, value];
    if (
      value === false ||
      (min !== null && max !== null && min !== false && max !== false && min > max)
    ) {
      problem = m.limitsInvalid();
      return;
    }
    if (value === column[which]) return;
    void run(() => updateColumn(board.id, column.id, { [which]: value }));
  }

  function addExisting(event: SubmitEvent): void {
    event.preventDefault();
    if (addValue === '') return;
    const value = Number(addValue);
    void run(() => addColumn(board.id, { value })).then((done) => {
      if (done) addValue = '';
    });
  }

  function askNew(event: SubmitEvent): void {
    event.preventDefault();
    const typed = newName.trim();
    newNameError = typed ? null : m.nameRequired();
    if (typed) confirmNew = typed;
  }

  function addNew(): void {
    const typed = confirmNew;
    confirmNew = null;
    if (!typed) return;
    void run(() => addColumn(board.id, { new_value: typed })).then((done) => {
      if (done) newName = '';
    });
  }

  function rename(event: SubmitEvent): void {
    event.preventDefault();
    const column = renaming;
    const typed = renameText.trim();
    if (!column) return;
    if (!typed) {
      renameError = m.nameRequired();
      return;
    }
    renaming = null;
    if (typed !== column.value.name)
      void run(() => updateColumn(board.id, column.id, { name: typed }));
  }
</script>

<div class="columns">
  {#if problem}<Notice tone="danger" ondismiss={() => (problem = null)}>{problem}</Notice>{/if}

  <Select
    label={m.columnField()}
    hint={m.columnFieldHint()}
    options={[
      { value: '', label: m.noField() },
      ...candidates.map((field) => ({ value: String(field.id), label: field.name })),
    ]}
    value={fieldChoice}
    disabled={busy}
    onchange={(event: Event & { currentTarget: HTMLSelectElement }) =>
      chooseField(event.currentTarget.value)}
  />

  {#if board.column_field}
    <section class="part" aria-labelledby="{uid}-list">
      <h3 id="{uid}-list">{m.columnList()}</h3>
      {#if board.columns.length === 0}
        <p class="muted">{m.noColumns()}</p>
      {:else}
        <ReorderList
          items={board.columns}
          key={(column: BoardColumn) => column.id}
          label={m.columnList()}
          sortable
          {busy}
          onmove={move}
        >
          {#snippet row(column: BoardColumn)}
            <div class="row">
              <span class="name"
                ><ValueChip name={column.value.name} color={column.value.color} /></span
              >
              <label class="limit">
                <span aria-hidden="true">{m.minShort()}</span>
                <input
                  type="number"
                  min="0"
                  inputmode="numeric"
                  aria-label={m.minLabel({ name: column.value.name })}
                  value={column.wip_min ?? ''}
                  onchange={(event) => setLimit(column, 'wip_min', event.currentTarget.value)}
                />
              </label>
              <label class="limit">
                <span aria-hidden="true">{m.maxShort()}</span>
                <input
                  type="number"
                  min="0"
                  inputmode="numeric"
                  aria-label={m.maxLabel({ name: column.value.name })}
                  value={column.wip_max ?? ''}
                  onchange={(event) => setLimit(column, 'wip_max', event.currentTarget.value)}
                />
              </label>
              <Menu
                label={m.columnActions({ name: column.value.name })}
                items={actions(column)}
                placement={{ side: 'bottom', align: 'end' }}
              >
                {#snippet trigger(props)}
                  <IconButton
                    icon={EllipsisVertical}
                    size="sm"
                    label={m.columnActions({ name: column.value.name })}
                    {...props}
                  />
                {/snippet}
              </Menu>
            </div>
          {/snippet}
        </ReorderList>
      {/if}
    </section>

    <section class="part" aria-labelledby="{uid}-add">
      <h3 id="{uid}-add">{m.addColumn()}</h3>
      {#if offered.length > 0}
        <form class="add" novalidate onsubmit={addExisting}>
          <Select
            label={m.existingValue()}
            options={[{ value: '', label: '—' }, ...offered]}
            bind:value={addValue}
          />
          <Button type="submit" size="sm" disabled={busy || addValue === ''}>{m.add()}</Button>
        </form>
      {:else if sets.length > 0}
        <p class="muted">{m.allValuesUsed()}</p>
      {/if}
      <form class="add" novalidate onsubmit={askNew}>
        <TextField
          label={m.newValue()}
          bind:value={newName}
          maxlength={NAME_MAX}
          autocomplete="off"
          error={newNameError}
        />
        <Button type="submit" size="sm" disabled={busy}>{m.add()}</Button>
      </form>
    </section>
  {/if}
</div>

<Dialog
  open={pendingField !== null}
  title={m.changeFieldTitle()}
  size="sm"
  onclose={() => {
    pendingField = null;
    fieldChoice = board.column_field ? String(board.column_field.id) : '';
  }}
>
  <p>{m.changeFieldText({ count: board.columns.length })}</p>
  {#snippet footer()}
    <Button variant="ghost" onclick={() => (pendingField = null)}>{m.cancel()}</Button>
    <Button variant="danger" onclick={() => void changeField(pendingField ?? '')}
      >{m.changeField()}</Button
    >
  {/snippet}
</Dialog>

<Dialog
  open={confirmNew !== null}
  title={m.newValueTitle({ name: confirmNew ?? '' })}
  size="sm"
  onclose={() => (confirmNew = null)}
>
  {#if newSet}
    <p>{m.newValueUsedIn({ projects: usedIn(newSet), hidden: newSet.used_in.hidden })}</p>
  {/if}
  {#snippet footer()}
    <Button variant="ghost" onclick={() => (confirmNew = null)}>{m.cancel()}</Button>
    <Button variant="primary" onclick={addNew}>{m.add()}</Button>
  {/snippet}
</Dialog>

<Dialog
  open={renaming !== null}
  title={m.renameTitle({ name: renaming?.value.name ?? '' })}
  size="sm"
  onclose={() => (renaming = null)}
>
  <form id="{uid}-rename" class="part" novalidate onsubmit={rename}>
    <TextField
      label={m.newName()}
      bind:value={renameText}
      maxlength={NAME_MAX}
      autocomplete="off"
      error={renameError}
    />
    {#if renameSet}
      <p class="muted">
        {m.setUsedIn({ projects: usedIn(renameSet), hidden: renameSet.used_in.hidden })}
      </p>
    {/if}
  </form>
  {#snippet footer()}
    <Button variant="ghost" onclick={() => (renaming = null)}>{m.cancel()}</Button>
    <Button variant="primary" type="submit" form="{uid}-rename">{m.rename()}</Button>
  {/snippet}
</Dialog>

<style>
  .columns {
    display: flex;
    flex-direction: column;
    gap: var(--space-6);
  }

  .part {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }

  h3 {
    font-size: var(--text-md);
  }

  .muted {
    color: var(--color-text-muted);
  }

  .row {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    min-width: 0;
  }

  .name {
    flex: 1;
    min-width: 0;
  }

  .limit {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
    color: var(--color-text-muted);
    font-size: var(--text-xs);
  }

  .limit input {
    width: 3.5rem;
    min-height: var(--control-sm);
    padding: 0 var(--space-1);
    border: 1px solid var(--color-border-control);
    border-radius: var(--radius-sm);
    background: var(--color-surface);
    color: var(--color-text);
    font: inherit;
    font-size: var(--text-sm);
  }

  .add {
    display: flex;
    flex-wrap: wrap;
    align-items: flex-end;
    gap: var(--space-2);
  }

  .add > :global(:first-child) {
    flex: 1;
    min-width: 12rem;
  }
</style>
