<script lang="ts">
  // «Дорожки» (BRD-049…BRD-055, BRD-086): what makes the lanes — nothing, the values of a field
  // or an attribute, or parent issues (optionally only parents holding chosen values); the lanes
  // in order (drag or «Выше»/«Ниже»), adding and removing them; where «Прочее» goes; the value new
  // cards get on a parent-lane board. Each change saves the lane settings whole.
  import X from '@lucide/svelte/icons/x';

  import type {
    Board,
    BoardLanesInput,
    FieldRef,
    LaneMode,
    OtherLane,
    ProjectField,
    ValueSet,
  } from '#lib/api/schema.gen.ts';
  import { findGroups, findPeople } from '#lib/features/access/index.ts';
  import {
    ReorderList,
    ValueChip,
    ensureValueSet,
    fetchProjectFields,
    isFullSet,
  } from '#lib/features/fields/index.ts';
  import { findTags } from '#lib/features/issues/index.ts';
  import IconButton from '#lib/ui/IconButton.svelte';
  import Notice from '#lib/ui/Notice.svelte';
  import OptionList from '#lib/ui/OptionList.svelte';
  import RadioGroup from '#lib/ui/RadioGroup.svelte';
  import Select from '#lib/ui/Select.svelte';
  import { describeError } from '#lib/ui/toast.svelte.ts';

  import { lanesInput } from '../boards.ts';
  import * as m from '../messages.ts';
  import { saveBoard } from './settings.ts';

  interface Props {
    board: Board;
    busy?: boolean;
    onchanged: () => unknown;
  }

  let { board, busy = $bindable(false), onchanged }: Props = $props();

  /** Kinds of a field with a list of values (BRD-050, BRD-051). */
  const SET_KINDS = new Set(['state', 'enum', 'owned', 'version', 'build']);
  /** Kinds that make value lanes. */
  const VALUE_KINDS = new Set([...SET_KINDS, 'user', 'group', 'date', 'date_time']);
  const ATTRIBUTES = ['project', 'tags', 'created', 'updated'] as const;
  const DATES = [
    'later',
    'today',
    'yesterday',
    'this_week',
    'last_week',
    'this_month',
    'older',
    'none',
  ];

  /** What the lane keys are: values of sets, people, groups, projects, tags or date ranges. */
  type Source = 'values' | 'users' | 'groups' | 'projects' | 'tags' | 'dates';
  interface Choice {
    key: string;
    name: string;
    color: number | null;
  }

  const uid = $props.id();
  let attached = $state<{ project: number; fields: ProjectField[] }[]>([]);
  let sets = $state<ValueSet[]>([]);
  let problem = $state<string | null>(null);
  /** The chosen mode; «По значениям поля» waits for a field or an attribute before it saves. */
  let modeChoice = $derived<LaneMode>(board.lanes.mode);
  let query = $state('');
  let found = $state<Choice[]>([]);
  let searching = $state(false);
  let searchProblem = $state<string | null>(null);

  const lanes = $derived(board.lanes);
  const sourceKey = $derived(
    lanes.field ? `f:${String(lanes.field.id)}` : lanes.attribute ? `a:${lanes.attribute}` : '',
  );

  // The fields of every board project, once per change of the projects.
  $effect(() => {
    const projects = board.projects.map((one) => one.project.id).sort((a, b) => a - b);
    const controller = new AbortController();
    Promise.all(
      projects.map((project) =>
        fetchProjectFields(project, controller.signal).then((list) => ({
          project,
          fields: list.items,
        })),
      ),
    ).then(
      (list) => {
        attached = list;
      },
      () => {
        if (!controller.signal.aborted) problem = m.lanesFailed();
      },
    );
    return () => {
      controller.abort();
    };
  });

  /** Fields the mode may use: value lanes take more kinds than parent lanes. */
  const candidates = $derived.by(() => {
    const kinds = modeChoice === 'parents' ? SET_KINDS : VALUE_KINDS;
    const list: Record<number, FieldRef> = {};
    for (const { fields } of attached) {
      for (const one of fields) {
        if (kinds.has(one.field.kind) && one.field.id !== board.column_field?.id) {
          list[one.field.id] = one.field;
        }
      }
    }
    if (lanes.field && modeChoice === lanes.mode) list[lanes.field.id] = lanes.field;
    return Object.values(list).sort((a, b) => a.name.localeCompare(b.name));
  });

  const source = $derived.by((): Source | null => {
    if (lanes.mode === 'none') return null;
    const kind = lanes.field?.kind;
    if (kind) {
      if (SET_KINDS.has(kind)) return 'values';
      if (kind === 'user') return 'users';
      if (kind === 'group') return 'groups';
      return 'dates';
    }
    if (lanes.mode === 'parents') return null;
    if (lanes.attribute === 'project') return 'projects';
    if (lanes.attribute === 'tags') return 'tags';
    if (lanes.attribute === 'created' || lanes.attribute === 'updated') return 'dates';
    return null;
  });

  // The sets of a set field in the board projects: its values are the choices.
  $effect(() => {
    const field = source === 'values' ? lanes.field?.id : undefined;
    const ids: number[] = [];
    for (const { fields } of attached) {
      const set = fields.find((one) => one.field.id === field)?.set;
      if (set && !ids.includes(set.id)) ids.push(set.id);
    }
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

  // People, groups and tags are found by name.
  $effect(() => {
    const kind = source;
    if (kind !== 'users' && kind !== 'groups' && kind !== 'tags') return;
    const typed = query;
    const controller = new AbortController();
    const timer = setTimeout(
      () => {
        searching = true;
        searchProblem = null;
        const search: Promise<Choice[]> =
          kind === 'users'
            ? findPeople(typed, { signal: controller.signal, limit: 20 }).then((page) =>
                page.items.map((one) => ({
                  key: `u:${String(one.id)}`,
                  name: one.full_name,
                  color: null,
                })),
              )
            : kind === 'groups'
              ? findGroups(typed, null, 'all').then((page) =>
                  page.items.map((one) => ({
                    key: `g:${String(one.id)}`,
                    name: one.name,
                    color: null,
                  })),
                )
              : findTags(typed, controller.signal).then((tags) =>
                  tags.map((one) => ({
                    key: `t:${String(one.id)}`,
                    name: one.name,
                    color: one.color,
                  })),
                );
        search.then(
          (list) => {
            if (controller.signal.aborted) return;
            found = list;
            searching = false;
          },
          (failure: unknown) => {
            if (controller.signal.aborted) return;
            searchProblem = describeError(failure).message;
            searching = false;
          },
        );
      },
      typed ? 250 : 0,
    );
    return () => {
      clearTimeout(timer);
      controller.abort();
    };
  });

  const setValues = $derived(
    sets.flatMap((set) =>
      set.values
        .filter((one) => !one.archived)
        .map((one) => ({ key: `v:${String(one.id)}`, name: one.name, color: one.color })),
    ),
  );

  /** What the lanes can be: the chosen ones first, then what the field or search offers. */
  const choices = $derived.by((): Choice[] => {
    const offered: Choice[] =
      source === 'values'
        ? setValues
        : source === 'projects'
          ? board.projects.map((one) => ({
              key: `p:${String(one.project.id)}`,
              name: one.project.name,
              color: null,
            }))
          : source === 'dates'
            ? DATES.map((key) => ({ key: `d:${key}`, name: nameOf(`d:${key}`), color: null }))
            : found;
    const searched = source === 'users' || source === 'groups' || source === 'tags';
    const chosen = searched ? lanes.values.map(choiceOf) : [];
    const all = [...chosen, ...offered];
    return all.filter((one, index) => all.findIndex((other) => other.key === one.key) === index);
  });

  function choiceOf(key: string): Choice {
    const label = lanes.labels.find((one) => one.key === key);
    const value = setValues.find((one) => one.key === key);
    return { key, name: nameOf(key), color: label?.color ?? value?.color ?? null };
  }

  function nameOf(key: string): string {
    const [prefix = '', rest = ''] = key.split(':');
    if (prefix === 'd') return m.dateLanes[rest]?.() ?? rest;
    if (prefix === 'p') {
      const project = board.projects.find((one) => String(one.project.id) === rest);
      if (project) return project.project.name;
    }
    const label = lanes.labels.find((one) => one.key === key);
    if (label) return label.name;
    return setValues.find((one) => one.key === key)?.name ?? m.hiddenLane();
  }

  const listed = $derived(lanes.values.map((key, index) => ({ key, index })));

  async function save(change: Partial<BoardLanesInput>, paths: string[] = []): Promise<void> {
    problem = null;
    busy = true;
    try {
      const failed = await saveBoard(
        board,
        { lanes: { ...lanesInput(board.lanes), ...change } },
        paths,
      );
      if (failed) {
        problem = failed.message ?? Object.values(failed.fields).join(' ');
        modeChoice = board.lanes.mode;
        return;
      }
      await onchanged();
    } finally {
      busy = false;
    }
  }

  const FRESH = { field: null, attribute: null, values: [], new_card_value: null };

  function chooseMode(value: string): void {
    const mode = value as LaneMode;
    modeChoice = mode;
    if (mode === board.lanes.mode) return;
    // Value lanes need a field or an attribute first.
    if (mode === 'values') return;
    void save({ ...FRESH, mode }, ['lanes.mode']);
  }

  function chooseSource(value: string): void {
    if (value === sourceKey && modeChoice === lanes.mode) return;
    const [kind, rest = ''] = value.split(':');
    const mode = modeChoice;
    if (value === '' && mode === 'values') return;
    void save(
      {
        ...FRESH,
        mode,
        field: kind === 'f' ? Number(rest) : null,
        attribute: kind === 'a' ? (rest as (typeof ATTRIBUTES)[number]) : null,
      },
      ['lanes.field', 'lanes.attribute'],
    );
  }

  function setChosen(choice: Choice, on: boolean): void {
    const values = on
      ? [...lanes.values, choice.key]
      : lanes.values.filter((key) => key !== choice.key);
    void save({ values }, ['lanes.values']);
  }

  function reorder(index: number, after: number | null): void {
    const values = [...lanes.values];
    const [moved] = values.splice(index, 1);
    if (moved === undefined) return;
    const at = after === null ? 0 : values.indexOf(lanes.values[after] ?? '') + 1;
    values.splice(at, 0, moved);
    void save({ values }, ['lanes.values']);
  }

  const sourceOptions = $derived([
    {
      value: '',
      label: modeChoice === 'parents' ? m.anyParent() : m.laneSourceNone(),
    },
    ...candidates.map((field) => ({ value: `f:${String(field.id)}`, label: field.name })),
    ...(modeChoice === 'values'
      ? ATTRIBUTES.map((key) => ({ value: `a:${key}`, label: m.laneAttributes[key]() }))
      : []),
  ]);
</script>

<div class="lanes">
  {#if problem}<Notice tone="danger" ondismiss={() => (problem = null)}>{problem}</Notice>{/if}

  <RadioGroup
    legend={m.laneMode()}
    options={(['none', 'values', 'parents'] as const).map((key) => ({
      value: key,
      label: m.laneModes[key](),
      hint: m.laneModeHints[key](),
    }))}
    bind:value={() => modeChoice, (next: string) => chooseMode(next)}
  />

  {#if modeChoice !== 'none'}
    <Select
      label={modeChoice === 'parents' ? m.parentField() : m.laneSource()}
      options={sourceOptions}
      value={modeChoice === lanes.mode ? sourceKey : ''}
      disabled={busy}
      onchange={(event: Event & { currentTarget: HTMLSelectElement }) =>
        chooseSource(event.currentTarget.value)}
    />
  {/if}

  {#if modeChoice === lanes.mode && source}
    <section class="part" aria-labelledby="{uid}-list">
      <h3 id="{uid}-list">{lanes.mode === 'parents' ? m.parentValues() : m.laneValues()}</h3>
      {#if listed.length === 0}
        <p class="muted">
          {source === 'dates'
            ? m.allDateLanes()
            : lanes.mode === 'parents'
              ? m.anyParentValue()
              : m.noLaneValues()}
        </p>
      {:else}
        <ReorderList
          items={listed}
          key={(item: { key: string; index: number }) => item.index}
          label={lanes.mode === 'parents' ? m.parentValues() : m.laneValues()}
          sortable={lanes.mode === 'values'}
          {busy}
          onmove={reorder}
        >
          {#snippet row(item: { key: string; index: number })}
            {@const choice = choiceOf(item.key)}
            <div class="row">
              <span class="name">
                {#if choice.color !== null}
                  <ValueChip name={choice.name} color={choice.color} />
                {:else}
                  {choice.name}
                {/if}
              </span>
              <IconButton
                icon={X}
                size="sm"
                label={m.removeLane({ name: choice.name })}
                disabled={busy}
                onclick={() => {
                  setChosen(choice, false);
                }}
              />
            </div>
          {/snippet}
        </ReorderList>
      {/if}
      <OptionList
        label={m.chooseLanes()}
        mode="multiple"
        options={choices}
        key={(choice: Choice) => choice.key}
        checked={(choice: Choice) => lanes.values.includes(choice.key)}
        disabled={() => busy}
        onchange={setChosen}
        bind:query={
          () =>
            source === 'users' || source === 'groups' || source === 'tags' ? query : undefined,
          (next: string | undefined) => {
            query = next ?? '';
          }
        }
        searchLabel={m.findLaneValues()}
        loading={searching}
        problem={searchProblem}
      >
        {#snippet option(choice: Choice)}
          {#if choice.color !== null}
            <ValueChip name={choice.name} color={choice.color} />
          {:else}
            {choice.name}
          {/if}
        {/snippet}
      </OptionList>
    </section>
  {/if}

  {#if lanes.mode !== 'none' && modeChoice === lanes.mode}
    <RadioGroup
      legend={m.otherPlace()}
      hint={lanes.other === 'off' ? m.otherOffHint() : undefined}
      options={(['top', 'bottom', 'off'] as const).map((key) => ({
        value: key,
        label: m.otherPlaces[key](),
      }))}
      bind:value={
        () => lanes.other,
        (next: string) => {
          if (next !== lanes.other) void save({ other: next as OtherLane }, ['lanes.other']);
        }
      }
    />
  {/if}

  {#if lanes.mode === 'parents' && modeChoice === 'parents' && source === 'values'}
    <Select
      label={m.newCardValue()}
      options={[
        { value: '', label: m.newCardValueNone() },
        ...setValues.map((one) => ({ value: one.key.slice(2), label: one.name })),
      ]}
      value={lanes.new_card_value === null ? '' : String(lanes.new_card_value)}
      disabled={busy}
      onchange={(event: Event & { currentTarget: HTMLSelectElement }) => {
        const value = event.currentTarget.value;
        void save({ new_card_value: value === '' ? null : Number(value) }, [
          'lanes.new_card_value',
        ]);
      }}
    />
  {/if}
</div>

<style>
  .lanes {
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
</style>
