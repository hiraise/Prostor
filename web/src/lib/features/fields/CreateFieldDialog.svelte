<script lang="ts">
  // Creating a field of the instance (FLD-015…FLD-020): name, type, several values, aliases; for
  // types with values, a new empty set or an existing set of the same type — and only with an
  // existing set, default values from it; then what projects start with: may be empty, the empty
  // value label, private by default, and whether new projects get it. Problems show in the dialog.
  import { untrack } from 'svelte';

  import type { Field, FieldKind, NewField, ValueSetSummary } from '#lib/api/schema.gen.ts';
  import type { PersonLike } from '#lib/features/access/index.ts';
  import * as common from '#lib/i18n/messages/common.ts';
  import { fieldErrorText } from '#lib/i18n/messages/errors.ts';
  import type { EntityHandle } from '#lib/store/index.ts';
  import Button from '#lib/ui/Button.svelte';
  import Checkbox from '#lib/ui/Checkbox.svelte';
  import Dialog from '#lib/ui/Dialog.svelte';
  import Notice from '#lib/ui/Notice.svelte';
  import RadioGroup from '#lib/ui/RadioGroup.svelte';
  import Select from '#lib/ui/Select.svelte';
  import Spinner from '#lib/ui/Spinner.svelte';
  import TextField from '#lib/ui/TextField.svelte';

  import DefaultPeople from './DefaultPeople.svelte';
  import DefaultValues from './DefaultValues.svelte';
  import {
    FIELD_KINDS,
    createField,
    ensureValueSet,
    findValueSets,
    isFullSet,
    mayBeMulti,
    parseAliases,
    setKindOf,
    setValues,
    type StoredSet,
  } from './fields.ts';
  import * as m from './messages.ts';
  import { focusFirstError, refusal, refusalText } from './problems.ts';

  interface Props {
    open?: boolean;
    oncreated: (field: Field) => unknown;
  }

  let { open = $bindable(false), oncreated }: Props = $props();

  type Person = PersonLike & { id: number };

  const formId = $props.id();
  let name = $state('');
  let kind = $state<FieldKind>('enum');
  let multi = $state(false);
  let aliasText = $state('');
  let source = $state<'new' | 'existing'>('new');
  let setName = $state('');
  let sets = $state<ValueSetSummary[] | null>(null);
  let setsProblem = $state<string | null>(null);
  let setId = $state('');
  let setHandle = $state<EntityHandle<StoredSet> | null>(null);
  let chosen = $state<number[]>([]);
  let people = $state<Person[]>([]);
  let canBeEmpty = $state(true);
  let emptyLabel = $state('');
  let hidden = $state(false);
  let autoAttach = $state(false);
  let busy = $state(false);
  let problem = $state<string | null>(null);
  let errors = $state<Record<string, string>>({});
  let form: HTMLFormElement | undefined = $state();

  const setKind = $derived(setKindOf(kind));
  const set = $derived.by(() => {
    const row = setHandle?.current;
    return isFullSet(row) && String(row.id) === setId ? row : null;
  });
  const kindOptions = $derived(
    FIELD_KINDS.map((value) => ({ value, label: m.kindNames[value]() })),
  );
  const setOptions = $derived([
    { value: '', label: m.chooseSet() },
    ...(sets ?? []).map((item) => ({ value: String(item.id), label: item.name })),
  ]);

  // Every opening starts from a clean form.
  $effect(() => {
    if (!open) return;
    untrack(() => {
      name = '';
      kind = 'enum';
      multi = false;
      aliasText = '';
      source = 'new';
      setName = '';
      sets = null;
      setId = '';
      chosen = [];
      people = [];
      canBeEmpty = true;
      emptyLabel = '';
      hidden = false;
      autoAttach = false;
      problem = null;
      errors = {};
    });
  });

  // The sets of the chosen type, when an existing one is wanted.
  $effect(() => {
    const wanted = setKind;
    if (!open || source !== 'existing' || wanted === null) return;
    let live = true;
    sets = null;
    setsProblem = null;
    findValueSets(wanted).then(
      (page) => {
        if (live) sets = page.items;
      },
      (error: unknown) => {
        if (live) setsProblem = refusalText(error);
      },
    );
    return () => {
      live = false;
    };
  });

  // The values of the chosen set, to choose defaults from.
  $effect(() => {
    const id = Number(setId);
    setHandle = null;
    if (!open || source !== 'existing' || !id) return;
    let live = true;
    ensureValueSet(id).then(
      (handle) => {
        if (live) setHandle = handle;
      },
      () => undefined,
    );
    return () => {
      live = false;
    };
  });

  function changeKind(next: string): void {
    kind = FIELD_KINDS.find((item) => item === next) ?? 'enum';
    setId = '';
    chosen = [];
    people = [];
  }

  async function submit(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    problem = null;
    errors = {};
    if (!name.trim()) errors.name = fieldErrorText.required();
    if (setKind !== null && source === 'existing' && !setId) errors.set = m.chooseSet();
    if (Object.keys(errors).length > 0) {
      focusFirstError(form);
      return;
    }
    const existing = setKind !== null && source === 'existing';
    const input: NewField = {
      name: name.trim(),
      kind,
      multi: mayBeMulti(kind) && multi,
      aliases: parseAliases(aliasText),
      can_be_empty: canBeEmpty,
      empty_label: canBeEmpty ? emptyLabel.trim() : '',
      private: hidden,
      auto_attach: autoAttach,
    };
    if (setKind !== null) {
      input.set = existing
        ? { source: 'existing', set_id: Number(setId) }
        : { source: 'new', name: setName.trim() || null };
    }
    if (existing) {
      input.default_values = kind === 'user' ? people.map((person) => person.id) : chosen;
    }
    busy = true;
    try {
      const field = await createField(input);
      open = false;
      await oncreated(field);
    } catch (error) {
      const found = refusal(error, {
        own: {
          name: { taken: m.fieldNameTaken, invalid: m.fieldNameInvalid },
          aliases: { invalid: m.tooManyAliases, too_long: m.tooManyAliases },
          'set.name': { taken: m.setNameTaken },
          default_values: { invalid: m.defaultNotInSet },
        },
        paths: ['name', 'aliases', 'multi', 'empty_label', 'set', 'set.name', 'default_values'],
      });
      errors = found.fields;
      problem = found.message;
      focusFirstError(form);
    } finally {
      busy = false;
    }
  }
</script>

<Dialog bind:open title={m.newFieldTitle()} size="lg" dismissible={!busy}>
  {#if open}
    <form id={formId} class="form" novalidate onsubmit={submit} bind:this={form}>
      {#if problem}<Notice tone="danger">{problem}</Notice>{/if}
      <TextField
        label={m.name()}
        bind:value={name}
        hint={m.fieldNameHint()}
        error={errors.name}
        maxlength={100}
        autocomplete="off"
        required
      />
      <Select
        label={m.fieldType()}
        options={kindOptions}
        error={errors.multi}
        bind:value={() => kind, changeKind}
      />
      {#if mayBeMulti(kind)}
        <Checkbox label={m.severalValues()} hint={m.severalValuesHint()} bind:checked={multi} />
      {/if}
      <TextField
        label={m.aliases()}
        bind:value={aliasText}
        hint={m.aliasesHint()}
        error={errors.aliases}
        autocomplete="off"
      />

      {#if setKind !== null}
        <RadioGroup
          legend={m.valuesSource()}
          options={[
            { value: 'new', label: m.newSet(), hint: m.newSetHint() },
            { value: 'existing', label: m.existingSet(), hint: m.existingSetHint() },
          ]}
          bind:value={
            () => source,
            (next: string) => {
              source = next === 'existing' ? 'existing' : 'new';
            }
          }
        />
        {#if source === 'new'}
          <TextField
            label={m.setName()}
            bind:value={setName}
            hint={m.setNameHint()}
            error={errors['set.name']}
            maxlength={100}
            autocomplete="off"
          />
        {:else if setsProblem}
          <Notice tone="danger">{setsProblem}</Notice>
        {:else if sets === null}
          <Spinner label={common.loading()} />
        {:else if sets.length === 0}
          <p class="note">{m.noSetsOfKind()}</p>
        {:else}
          <Select
            label={m.set()}
            options={setOptions}
            bind:value={setId}
            error={errors.set}
            required
          />
          {#if set && kind === 'user'}
            <DefaultPeople
              multi={mayBeMulti(kind) && multi}
              bind:people
              error={errors.default_values}
            />
          {:else if set}
            <DefaultValues values={setValues(set)} multi={mayBeMulti(kind) && multi} bind:chosen />
            {#if errors.default_values}<p class="error">{errors.default_values}</p>{/if}
          {/if}
        {/if}
      {/if}

      <section class="starting" aria-labelledby="{formId}-starting">
        <h3 id="{formId}-starting">{m.startingSettings()}</h3>
        <p class="note">{m.startingHint()}</p>
        <Checkbox label={m.canBeEmpty()} hint={m.canBeEmptyHint()} bind:checked={canBeEmpty} />
        <TextField
          label={m.emptyLabel()}
          hint={canBeEmpty ? m.emptyLabelHint() : m.emptyLabelOff()}
          bind:value={emptyLabel}
          error={errors.empty_label}
          disabled={!canBeEmpty}
          maxlength={50}
          autocomplete="off"
        />
        <Checkbox label={m.privateByDefault()} bind:checked={hidden} />
        <Checkbox label={m.autoAttach()} hint={m.autoAttachHint()} bind:checked={autoAttach} />
      </section>
    </form>
  {/if}

  {#snippet footer()}
    <Button
      disabled={busy}
      onclick={() => {
        open = false;
      }}
    >
      {common.cancel()}
    </Button>
    <Button type="submit" form={formId} variant="primary" loading={busy}>
      {m.createField()}
    </Button>
  {/snippet}
</Dialog>

<style>
  .form {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }

  .starting {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    padding-top: var(--space-4);
    border-top: 1px solid var(--color-border);
  }

  h3 {
    font-size: var(--text-md);
  }

  .note {
    color: var(--color-text-muted);
    font-size: var(--text-sm);
  }

  .error {
    color: var(--color-danger);
    font-size: var(--text-sm);
  }
</style>
