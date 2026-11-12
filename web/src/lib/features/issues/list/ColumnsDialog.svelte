<script lang="ts">
  // Choosing the list's columns (QRY-078): the shown ones in order — dragged with a mouse, moved
  // with the arrow buttons, removed — and a column to add: a built-in attribute or a field of the
  // readable projects, narrowed to one project by a select. Saved into preference `list_columns`;
  // «Как по умолчанию» clears it. The summary stays: it is the row's link.
  import ArrowDown from '@lucide/svelte/icons/arrow-down';
  import ArrowUp from '@lucide/svelte/icons/arrow-up';
  import X from '@lucide/svelte/icons/x';

  import type { FieldRef, Project } from '#lib/api/schema.gen.ts';
  import { ReorderList } from '#lib/features/fields/index.ts';
  import { findProjects } from '#lib/features/projects/index.ts';
  import Button from '#lib/ui/Button.svelte';
  import Dialog from '#lib/ui/Dialog.svelte';
  import IconButton from '#lib/ui/IconButton.svelte';
  import Notice from '#lib/ui/Notice.svelte';
  import Select from '#lib/ui/Select.svelte';

  import {
    BUILTIN_COLUMNS,
    columnFields,
    issueFields,
    shownColumns,
    type ShownColumn,
  } from './columns.ts';
  import * as m from './messages.ts';

  interface Props {
    open?: boolean;
    /** The columns shown now. */
    columns: readonly string[];
    /** Fields of every readable project: names of field columns. */
    known: readonly FieldRef[];
    /** The project to offer fields of first (the context's). */
    project: number | null;
    /** `null`: back to the default columns. */
    onsave: (columns: string[] | null) => void;
    onclose: () => void;
  }

  let { open = $bindable(false), columns, known, project, onsave, onclose }: Props = $props();

  const id = $props.id();
  let chosen: string[] = $state([]);
  let projects: Project[] = $state([]);
  let narrowed = $state('');
  let offered: FieldRef[] = $state([]);
  let failed = $state(false);
  let adding = $state('');

  // Each opening starts from the columns shown now and the context's project.
  $effect(() => {
    if (!open) return;
    chosen = [...columns];
    narrowed = project === null ? '' : String(project);
    findProjects('')
      .then((page) => {
        projects = page.items;
      })
      .catch(() => {
        projects = [];
      });
  });

  $effect(() => {
    if (!open) return;
    const wanted = narrowed === '' ? null : Number(narrowed);
    let current = true;
    failed = false;
    issueFields(wanted)
      .then((fields) => {
        if (current) offered = columnFields(fields);
      })
      .catch(() => {
        if (current) {
          offered = [];
          failed = true;
        }
      });
    return () => {
      current = false;
    };
  });

  const names = $derived([...known, ...offered]);
  const items: ShownColumn[] = $derived(shownColumns(chosen, names));
  const addable = $derived([
    ...BUILTIN_COLUMNS.filter((name) => !chosen.includes(name)).map((name) => ({
      value: name,
      label: m.columnNames[name](),
    })),
    ...offered
      .filter((field) => !chosen.includes(`field:${String(field.id)}`))
      .map((field) => ({ value: `field:${String(field.id)}`, label: field.name })),
  ]);

  $effect(() => {
    if (!addable.some((option) => option.value === adding)) adding = addable[0]?.value ?? '';
  });

  function place(text: string, index: number): void {
    const rest = chosen.filter((other) => other !== text);
    rest.splice(Math.max(0, Math.min(index, rest.length)), 0, text);
    chosen = rest;
  }

  function moveAfter(key: number, after: number | null): void {
    const moving = items.find((item) => item.key === key);
    if (!moving) return;
    const rest = items.filter((item) => item.key !== key);
    const index = after === null ? 0 : rest.findIndex((item) => item.key === after) + 1;
    place(moving.text, index);
  }

  function shift(item: ShownColumn, by: number): void {
    place(item.text, items.indexOf(item) + by);
  }

  function add(): void {
    if (adding === '' || chosen.includes(adding)) return;
    chosen = [...items.map((item) => item.text), adding];
  }

  const projectOptions = $derived([
    { value: '', label: m.allProjects() },
    ...projects.map((p) => ({ value: String(p.id), label: p.name })),
  ]);
</script>

<Dialog bind:open title={m.columnsTitle()} size="md" {onclose}>
  <div class="body">
    <ReorderList
      {items}
      key={(item: ShownColumn) => item.key}
      label={m.shownColumns()}
      sortable
      onmove={moveAfter}
    >
      {#snippet row(item: ShownColumn, index: number)}
        <div class="column">
          <span class="name">{item.name}</span>
          <IconButton
            icon={ArrowUp}
            label={m.moveUp({ name: item.name })}
            disabled={index === 0}
            onclick={() => {
              shift(item, -1);
            }}
          />
          <IconButton
            icon={ArrowDown}
            label={m.moveDown({ name: item.name })}
            disabled={index === items.length - 1}
            onclick={() => {
              shift(item, 1);
            }}
          />
          <IconButton
            icon={X}
            label={m.removeColumn({ name: item.name })}
            disabled={item.text === 'summary'}
            onclick={() => {
              chosen = items.map((other) => other.text).filter((text) => text !== item.text);
            }}
          />
        </div>
      {/snippet}
    </ReorderList>

    <form
      id="{id}-add"
      class="add"
      onsubmit={(event) => {
        event.preventDefault();
        add();
      }}
    >
      <Select label={m.fieldsOf()} options={projectOptions} bind:value={narrowed} />
      {#if failed}<Notice tone="danger">{m.fieldsFailed()}</Notice>{/if}
      {#if addable.length > 0}
        <div class="pick">
          <Select label={m.addColumn()} options={addable} bind:value={adding} />
          <Button type="submit" variant="secondary">{m.add()}</Button>
        </div>
      {:else}
        <p class="none">{m.nothingToAdd()}</p>
      {/if}
    </form>
  </div>
  {#snippet footer()}
    <Button
      variant="ghost"
      onclick={() => {
        onsave(null);
      }}
    >
      {m.resetColumns()}
    </Button>
    <span class="gap"></span>
    <Button variant="ghost" onclick={onclose}>{m.cancel()}</Button>
    <Button
      variant="primary"
      onclick={() => {
        onsave(items.map((item) => item.text));
      }}
    >
      {m.save()}
    </Button>
  {/snippet}
</Dialog>

<style>
  .body {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }

  .column {
    display: flex;
    align-items: center;
    gap: var(--space-1);
    padding: var(--space-1) var(--space-2);
  }

  .name {
    flex: 1;
    min-width: 0;
    overflow-wrap: anywhere;
  }

  .add {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }

  .pick {
    display: flex;
    align-items: flex-end;
    gap: var(--space-2);
  }

  .pick > :global(:first-child) {
    flex: 1;
  }

  .none {
    color: var(--color-text-muted);
    font-size: var(--text-sm);
  }

  .gap {
    flex: 1;
  }
</style>
