<script lang="ts">
  // «Карточка» (BRD-059…BRD-062, BRD-086): the fields cards show (at most ten, in order, each in
  // full or as its first letter), what colors the cards (nothing, the project, the value of a
  // field) and the estimation fields. Only fields of the board projects the person reads are
  // offered (FLD-062). Each change saves the card settings whole.
  import X from '@lucide/svelte/icons/x';

  import type {
    Board,
    BoardCardsInput,
    CardColor,
    FieldRef,
    ProjectField,
  } from '#lib/api/schema.gen.ts';
  import { ReorderList, fetchProjectFields, fieldMessages } from '#lib/features/fields/index.ts';
  import Checkbox from '#lib/ui/Checkbox.svelte';
  import IconButton from '#lib/ui/IconButton.svelte';
  import Notice from '#lib/ui/Notice.svelte';
  import RadioGroup from '#lib/ui/RadioGroup.svelte';
  import Select from '#lib/ui/Select.svelte';

  import { cardsInput } from '../boards.ts';
  import * as m from '../messages.ts';
  import { saveBoard } from './settings.ts';

  interface Props {
    board: Board;
    busy?: boolean;
    onchanged: () => unknown;
  }

  let { board, busy = $bindable(false), onchanged }: Props = $props();

  /** The most fields a card shows (BRD-059). */
  const FIELDS_MAX = 10;
  /** Kinds whose values have colors (BRD-060). */
  const COLOR_KINDS = new Set(['state', 'enum', 'owned', 'version', 'build']);
  /** Kinds an estimation may be (BRD-062). */
  const ESTIMATION_KINDS = new Set(['integer', 'float', 'duration']);

  const uid = $props.id();
  let attached = $state<ProjectField[][]>([]);
  let problem = $state<string | null>(null);
  /** The chosen coloring; «По полю» waits for a field before it saves. */
  let colorChoice = $derived<CardColor>(board.cards.color);

  const cards = $derived(board.cards);

  // The fields of every board project, once per change of the projects.
  $effect(() => {
    const projects = board.projects.map((one) => one.project.id).sort((a, b) => a - b);
    const controller = new AbortController();
    Promise.all(
      projects.map((project) =>
        fetchProjectFields(project, controller.signal).then((list) => list.items),
      ),
    ).then(
      (list) => {
        attached = list;
      },
      () => {
        if (!controller.signal.aborted) problem = m.cardsFailed();
      },
    );
    return () => {
      controller.abort();
    };
  });

  /** Fields of the board projects by name; text fields are never on cards. */
  const offered = $derived.by(() => {
    const list: Record<number, FieldRef> = {};
    for (const fields of attached) {
      for (const one of fields) {
        if (one.field.kind !== 'text') list[one.field.id] = one.field;
      }
    }
    return Object.values(list).sort((a, b) => a.name.localeCompare(b.name));
  });

  const addable = $derived(
    offered.filter(
      (field) =>
        field.id !== board.column_field?.id && !cards.fields.some((one) => one.id === field.id),
    ),
  );

  /** A choice of one field among `kinds`, the current one kept even if no longer offered. */
  function fieldOptions(kinds: Set<string>, current: FieldRef | null, none: string) {
    const list = offered.filter((field) => kinds.has(field.kind));
    if (current && !list.some((one) => one.id === current.id)) list.unshift(current);
    return [
      { value: '', label: none },
      ...list.map((field) => ({ value: String(field.id), label: field.name })),
    ];
  }

  const colorOptions = [
    { value: '', label: fieldMessages.noColor() },
    ...fieldMessages.colorNames.map((label, scheme) => ({
      value: String(scheme),
      label: label(),
    })),
  ];

  async function save(change: Partial<BoardCardsInput>, paths: string[]): Promise<void> {
    problem = null;
    busy = true;
    try {
      const failed = await saveBoard(board, { cards: { ...cardsInput(cards), ...change } }, paths);
      if (failed) {
        problem = failed.message ?? Object.values(failed.fields).join(' ');
        colorChoice = board.cards.color;
        return;
      }
      await onchanged();
    } finally {
      busy = false;
    }
  }

  function idOf(value: string): number | null {
    return value === '' ? null : Number(value);
  }

  function addField(value: string): void {
    const id = idOf(value);
    if (id === null) return;
    void save({ fields: [...cards.fields.map((one) => one.id), id] }, ['cards.fields']);
  }

  function removeField(id: number): void {
    void save(
      {
        fields: cards.fields.map((one) => one.id).filter((one) => one !== id),
        initials: cards.initials.filter((one) => one !== id),
      },
      ['cards.fields'],
    );
  }

  function reorder(id: number, after: number | null): void {
    const ids = cards.fields.map((one) => one.id).filter((one) => one !== id);
    ids.splice(after === null ? 0 : ids.indexOf(after) + 1, 0, id);
    void save({ fields: ids }, ['cards.fields']);
  }

  function setInitial(id: number, on: boolean): void {
    const initials = on ? [...cards.initials, id] : cards.initials.filter((one) => one !== id);
    void save({ initials }, ['cards.initials']);
  }

  function chooseColor(value: string): void {
    const color = value as CardColor;
    colorChoice = color;
    if (color === cards.color || color === 'field') return;
    void save({ color, color_field: null }, ['cards.color']);
  }

  function setProjectColor(project: number, value: string): void {
    void save({ project_colors: [{ project, color: idOf(value) }] }, ['cards.project_colors']);
  }
</script>

<div class="card-tab">
  {#if problem}<Notice tone="danger" ondismiss={() => (problem = null)}>{problem}</Notice>{/if}

  <section class="part" aria-labelledby="{uid}-fields">
    <h3 id="{uid}-fields">{m.cardFields()}</h3>
    <p class="muted">{m.cardFieldsHint()}</p>
    {#if cards.fields.length === 0}
      <p class="muted">{m.noCardFields()}</p>
    {:else}
      <ReorderList
        items={cards.fields}
        key={(field: FieldRef) => field.id}
        label={m.cardFields()}
        sortable
        {busy}
        onmove={reorder}
      >
        {#snippet row(field: FieldRef)}
          <div class="row">
            <span class="name">{field.name}</span>
            <Checkbox
              label={m.initialOnly()}
              checked={cards.initials.includes(field.id)}
              disabled={busy}
              onchange={(event: Event & { currentTarget: HTMLInputElement }) => {
                setInitial(field.id, event.currentTarget.checked);
              }}
            />
            <IconButton
              icon={X}
              size="sm"
              label={m.removeCardField({ name: field.name })}
              disabled={busy}
              onclick={() => {
                removeField(field.id);
              }}
            />
          </div>
        {/snippet}
      </ReorderList>
    {/if}
    {#if cards.fields.length >= FIELDS_MAX}
      <p class="muted">{m.cardFieldsFull()}</p>
    {:else}
      <Select
        label={m.addCardField()}
        options={[
          { value: '', label: m.chooseField() },
          ...addable.map((field) => ({ value: String(field.id), label: field.name })),
        ]}
        value=""
        disabled={busy || addable.length === 0}
        onchange={(event: Event & { currentTarget: HTMLSelectElement }) => {
          addField(event.currentTarget.value);
        }}
      />
    {/if}
  </section>

  <RadioGroup
    legend={m.cardColor()}
    options={(['none', 'project', 'field'] as const).map((key) => ({
      value: key,
      label: m.cardColors[key](),
      hint: m.cardColorHints[key](),
    }))}
    bind:value={() => colorChoice, (next: string) => chooseColor(next)}
  />

  {#if colorChoice === 'field'}
    <Select
      label={m.colorField()}
      options={fieldOptions(COLOR_KINDS, cards.color_field, m.chooseField())}
      value={cards.color === 'field' ? String(cards.color_field?.id ?? '') : ''}
      disabled={busy}
      onchange={(event: Event & { currentTarget: HTMLSelectElement }) => {
        const id = idOf(event.currentTarget.value);
        if (id !== null) void save({ color: 'field', color_field: id }, ['cards.color_field']);
      }}
    />
  {:else if colorChoice === 'project' && cards.color === 'project'}
    <div class="part">
      {#each board.projects as one (one.project.id)}
        <Select
          label={m.projectColor({ name: one.project.name })}
          options={colorOptions}
          value={one.color === null ? '' : String(one.color)}
          disabled={busy}
          onchange={(event: Event & { currentTarget: HTMLSelectElement }) => {
            setProjectColor(one.project.id, event.currentTarget.value);
          }}
        />
      {/each}
    </div>
  {/if}

  <section class="part" aria-labelledby="{uid}-estimation">
    <h3 id="{uid}-estimation">{m.estimationField()}</h3>
    <Select
      label={m.estimationField()}
      hint={m.estimationHint()}
      options={fieldOptions(ESTIMATION_KINDS, cards.estimation, m.noEstimation())}
      value={String(cards.estimation?.id ?? '')}
      disabled={busy}
      onchange={(event: Event & { currentTarget: HTMLSelectElement }) => {
        void save({ estimation: idOf(event.currentTarget.value) }, ['cards.estimation']);
      }}
    />
    <Select
      label={m.originalEstimationField()}
      options={fieldOptions(ESTIMATION_KINDS, cards.original_estimation, m.noEstimation())}
      value={String(cards.original_estimation?.id ?? '')}
      disabled={busy}
      onchange={(event: Event & { currentTarget: HTMLSelectElement }) => {
        void save({ original_estimation: idOf(event.currentTarget.value) }, [
          'cards.original_estimation',
        ]);
      }}
    />
  </section>
</div>

<style>
  .card-tab {
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
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-2);
    min-width: 0;
  }

  .name {
    flex: 1;
    min-width: 0;
  }
</style>
