<script lang="ts">
  // What a project starts with when it adds the field (FLD-019) and whether new projects get it
  // (FLD-020): may be empty, the empty value label, private by default, default values. Projects
  // using the field already keep their own settings. Only system administrators change these.
  import { untrack } from 'svelte';

  import type { Field, UserRef, ValueRef } from '#lib/api/schema.gen.ts';
  import type { PersonLike } from '#lib/features/access/index.ts';
  import { can } from '#lib/session/index.ts';
  import { ConflictError, type EntityHandle } from '#lib/store/index.ts';
  import Button from '#lib/ui/Button.svelte';
  import Checkbox from '#lib/ui/Checkbox.svelte';
  import Notice from '#lib/ui/Notice.svelte';
  import Spinner from '#lib/ui/Spinner.svelte';
  import TextField from '#lib/ui/TextField.svelte';
  import { toasts } from '#lib/ui/toast.svelte.ts';

  import DefaultPeople from './DefaultPeople.svelte';
  import DefaultValues from './DefaultValues.svelte';
  import {
    ensureValueSet,
    isFullSet,
    setValues,
    updateField,
    type FieldChanges,
    type StoredSet,
  } from './fields.ts';
  import * as m from './messages.ts';
  import { focusFirstError, refusal } from './problems.ts';

  let { field }: { field: Field } = $props();

  type Person = PersonLike & { id: number };

  const uid = $props.id();
  let autoAttach = $state(untrack(() => field.auto_attach));
  let canBeEmpty = $state(untrack(() => field.can_be_empty));
  let emptyLabel = $state(untrack(() => field.empty_label));
  let hidden = $state(untrack(() => field.private));
  let chosen = $state<number[]>(untrack(() => field.default_values.map((value) => value.id)));
  let people = $state<Person[]>(untrack(() => [...field.default_users]));
  let setHandle = $state<EntityHandle<StoredSet> | null>(null);
  let busy = $state(false);
  let conflict = $state(false);
  let problem = $state<string | null>(null);
  let errors = $state<Record<string, string>>({});
  let form: HTMLFormElement | undefined = $state();

  const mayEdit = $derived(can('system.write'));
  const setId = $derived(field.set?.id ?? null);
  const set = $derived.by(() => {
    const row = setHandle?.current;
    return isFullSet(row) ? row : null;
  });

  // The values of the field's set, to choose defaults from.
  $effect(() => {
    const id = setId;
    setHandle = null;
    if (id === null || field.kind === 'user') return;
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

  function sameIds(a: readonly number[], b: readonly number[]): boolean {
    return a.length === b.length && a.every((id, index) => b[index] === id);
  }

  function userRef(person: Person): UserRef {
    return {
      id: person.id,
      full_name: person.full_name,
      username: person.username,
      avatar_color: person.avatar_color,
      avatar_url: person.avatar_url ?? null,
      kind: person.kind ?? 'person',
      pseudonym: person.pseudonym ?? null,
      disabled: false,
    };
  }

  function changes(): FieldChanges {
    const found: FieldChanges = {};
    if (autoAttach !== field.auto_attach) found.auto_attach = autoAttach;
    if (canBeEmpty !== field.can_be_empty) found.can_be_empty = canBeEmpty;
    const label = canBeEmpty ? emptyLabel.trim() : '';
    if (label !== field.empty_label) found.empty_label = label;
    if (hidden !== field.private) found.private = hidden;
    if (field.kind === 'user') {
      const before = field.default_users.map((person) => person.id);
      if (
        !sameIds(
          people.map((person) => person.id),
          before,
        )
      ) {
        found.default_users = people.map(userRef);
      }
    } else if (set) {
      const before = field.default_values.map((value) => value.id);
      if (!sameIds(chosen, before)) {
        const values = setValues(set);
        found.default_values = chosen
          .map((id) => values.find((value) => value.id === id))
          .filter((value): value is NonNullable<typeof value> => value !== undefined)
          .map((value): ValueRef => ({
            id: value.id,
            name: value.name,
            color: value.color,
            archived: value.archived,
          }));
      }
    }
    return found;
  }

  const changed = $derived(Object.keys(changes()).length > 0);

  async function submit(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    const found = changes();
    if (Object.keys(found).length === 0) return;
    busy = true;
    conflict = false;
    problem = null;
    errors = {};
    try {
      const saved = await updateField(field.id, found);
      emptyLabel = saved.empty_label;
      toasts.show({ tone: 'success', message: m.fieldSaved() });
    } catch (error) {
      if (error instanceof ConflictError) {
        conflict = true;
      } else {
        const refused = refusal(error, {
          own: { default_values: { invalid: m.defaultNotInSet } },
          paths: ['empty_label', 'default_values'],
        });
        errors = refused.fields;
        problem = refused.message;
        focusFirstError(form);
      }
    } finally {
      busy = false;
    }
  }
</script>

<section class="section" aria-labelledby="{uid}-title">
  <h2 id="{uid}-title">{m.startingSettings()}</h2>
  <p class="note">{m.startingHint()}</p>
  {#if conflict}<Notice tone="warning">{m.fieldConflict()}</Notice>{/if}
  {#if problem}<Notice tone="danger">{problem}</Notice>{/if}
  <form class="form" novalidate onsubmit={submit} bind:this={form}>
    <fieldset class="plain" disabled={!mayEdit}>
      <Checkbox label={m.autoAttach()} hint={m.autoAttachHint()} bind:checked={autoAttach} />
      <Checkbox label={m.canBeEmpty()} hint={m.canBeEmptyHint()} bind:checked={canBeEmpty} />
      <TextField
        label={m.emptyLabel()}
        hint={canBeEmpty ? m.emptyLabelHint() : m.emptyLabelOff()}
        bind:value={emptyLabel}
        error={errors.empty_label}
        disabled={!mayEdit || !canBeEmpty}
        maxlength={50}
        autocomplete="off"
      />
      <Checkbox label={m.privateByDefault()} bind:checked={hidden} />
      {#if field.kind === 'user' && field.set}
        <DefaultPeople
          multi={field.multi}
          bind:people
          disabled={!mayEdit}
          error={errors.default_values}
        />
      {:else if field.set && set}
        <DefaultValues
          values={setValues(set)}
          multi={field.multi}
          bind:chosen
          disabled={!mayEdit}
        />
        {#if errors.default_values}<p class="error">{errors.default_values}</p>{/if}
      {:else if field.set}
        <Spinner label={m.defaultsLoading()} />
      {/if}
    </fieldset>
    {#if mayEdit}
      <div class="actions">
        <Button type="submit" variant="primary" loading={busy} disabled={!changed && !busy}>
          {m.saveChanges()}
        </Button>
      </div>
    {/if}
  </form>
</section>

<style>
  .section {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    max-width: 36rem;
  }

  h2 {
    font-size: var(--text-lg);
  }

  .note {
    color: var(--color-text-muted);
    font-size: var(--text-sm);
  }

  .error {
    color: var(--color-danger);
    font-size: var(--text-sm);
  }

  .form,
  .plain {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }

  .plain {
    min-width: 0;
    margin: 0;
    padding: 0;
    border: none;
  }

  @media (width < 48rem) {
    .actions :global(button) {
      width: 100%;
    }
  }
</style>
