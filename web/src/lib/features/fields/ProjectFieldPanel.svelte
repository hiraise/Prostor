<script lang="ts">
  // One field's settings in a project (FLD-051, FLD-055…FLD-060) and its values (FLD-029…FLD-049):
  // empty handling and its label, default values, privacy, then the value set editor. Settings are
  // saved together with the version they were read on; a change made meanwhile is reported here and
  // the person's choices stay for saving again. An archived project shows everything read-only.
  import Asterisk from '@lucide/svelte/icons/asterisk';
  import Trash2 from '@lucide/svelte/icons/trash-2';
  import { untrack } from 'svelte';
  import { resolve } from '$app/paths';

  import { ApiError } from '#lib/api/client.ts';
  import type { ProjectField } from '#lib/api/schema.gen.ts';
  import { permissions, type PersonLike } from '#lib/features/access/index.ts';
  import { canSeeFields } from '#lib/session/index.ts';
  import type { EntityHandle } from '#lib/store/index.ts';
  import Button from '#lib/ui/Button.svelte';
  import Checkbox from '#lib/ui/Checkbox.svelte';
  import ErrorState from '#lib/ui/ErrorState.svelte';
  import Notice from '#lib/ui/Notice.svelte';
  import Spinner from '#lib/ui/Spinner.svelte';
  import TextField from '#lib/ui/TextField.svelte';
  import { toasts } from '#lib/ui/toast.svelte.ts';

  import DefaultPeople from './DefaultPeople.svelte';
  import DefaultValues from './DefaultValues.svelte';
  import { ensureValueSet, isFullSet, setKindOf, setValues, type StoredSet } from './fields.ts';
  import * as m from './messages.ts';
  import { focusFirstError, refusal, typeLabel } from './problems.ts';
  import type { ProjectFieldList, ProjectFieldSettings } from './project-fields.svelte.ts';
  import ValueSetEditor from './ValueSetEditor.svelte';

  interface Props {
    field: ProjectField;
    list: ProjectFieldList;
    /** The project is archived: nothing changes. */
    readonly: boolean;
    /** Asks to remove the field from the project. */
    ondetach: () => void;
  }

  let { field, list, readonly, ondetach }: Props = $props();

  type Person = PersonLike & { id: number };

  const uid = $props.id();
  let canBeEmpty = $state(untrack(() => field.can_be_empty));
  let emptyLabel = $state(untrack(() => field.empty_label));
  let hidden = $state(untrack(() => field.private));
  let chosen = $state<number[]>(untrack(() => field.default_values.map((value) => value.id)));
  let people = $state<Person[]>(untrack(() => [...field.default_users]));
  let busy = $state(false);
  let conflict = $state(false);
  let problem = $state<string | null>(null);
  let errors = $state<Record<string, string>>({});
  let form: HTMLFormElement | undefined = $state();
  let setHandle = $state<EntityHandle<StoredSet> | null>(null);
  let setError = $state<unknown>(null);

  const kind = $derived(field.field.kind);
  const setKind = $derived(setKindOf(kind));
  const setId = $derived(field.set?.id ?? null);
  const set = $derived.by(() => {
    const row = setHandle?.current;
    return isFullSet(row) ? row : null;
  });
  const defaults = $derived(kind === 'user' ? people.map((person) => person.id) : chosen);
  const savedDefaults = $derived(
    kind === 'user'
      ? field.default_users.map((person) => person.id)
      : field.default_values.map((value) => value.id),
  );
  const required = $derived(!canBeEmpty && (setKind === null || defaults.length === 0));
  const privateHint = $derived(
    m.privateHint({
      read: permissions['issue.private.read'].name(),
      update: permissions['issue.private.update'].name(),
    }),
  );

  function sameIds(a: readonly number[], b: readonly number[]): boolean {
    return a.length === b.length && a.every((id, index) => b[index] === id);
  }

  function settings(): ProjectFieldSettings {
    const changes: ProjectFieldSettings = {};
    if (canBeEmpty !== field.can_be_empty) changes.can_be_empty = canBeEmpty;
    if (canBeEmpty && emptyLabel.trim() !== field.empty_label) {
      changes.empty_label = emptyLabel.trim();
    }
    if (hidden !== field.private) changes.private = hidden;
    if (setKind !== null && !sameIds(defaults, savedDefaults)) changes.default_values = defaults;
    return changes;
  }

  const changed = $derived(Object.keys(settings()).length > 0);

  // The values of the field's set; again after the project gets its own copy (FLD-030).
  $effect(() => {
    const id = setId;
    setHandle = null;
    setError = null;
    if (id === null) return;
    let live = true;
    ensureValueSet(id).then(
      (handle) => {
        if (live) setHandle = handle;
      },
      (error: unknown) => {
        if (live) setError = error;
      },
    );
    return () => {
      live = false;
    };
  });

  async function submit(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    const changes = settings();
    if (Object.keys(changes).length === 0) return;
    busy = true;
    conflict = false;
    problem = null;
    errors = {};
    try {
      const saved = await list.save(field.field.id, changes);
      emptyLabel = saved.empty_label;
      toasts.show({ tone: 'success', message: m.settingsSaved() });
    } catch (error) {
      if (error instanceof ApiError && error.code === 'conflict') {
        conflict = true;
        await list.load().catch(() => undefined);
      } else {
        const found = refusal(error, {
          own: { default_values: { invalid: m.defaultNotInSet } },
          paths: ['empty_label', 'default_values'],
        });
        errors = found.fields;
        problem = found.message;
        focusFirstError(form);
      }
    } finally {
      busy = false;
    }
  }

  async function fork(): Promise<void> {
    await list.forkSet(field.field.id);
  }
</script>

<div class="panel">
  <div class="about">
    <p class="type">{typeLabel(field.field)}</p>
    {#if field.aliases.length > 0}
      <p>{m.aliasesLine({ list: field.aliases.join(', ') })}</p>
    {/if}
    <p>
      {m.sharedProperties()}
      {#if canSeeFields()}
        <a href={resolve('/(app)/admin/fields/[id]', { id: String(field.field.id) })}
          >{m.openInAdministration()}</a
        >
      {/if}
    </p>
  </div>

  <section class="section" aria-labelledby="{uid}-settings">
    <h3 id="{uid}-settings">{m.settingsInProject()}</h3>
    {#if conflict}<Notice tone="warning">{m.settingsConflict()}</Notice>{/if}
    {#if problem}<Notice tone="danger">{problem}</Notice>{/if}
    <form class="form" novalidate onsubmit={submit} bind:this={form}>
      <fieldset class="plain" disabled={readonly}>
        <Checkbox label={m.canBeEmpty()} hint={m.canBeEmptyHint()} bind:checked={canBeEmpty} />
        <TextField
          label={m.emptyLabel()}
          hint={canBeEmpty ? m.emptyLabelHint() : m.emptyLabelOff()}
          bind:value={emptyLabel}
          error={errors.empty_label}
          disabled={readonly || !canBeEmpty}
          maxlength={50}
          autocomplete="off"
        />
        {#if kind === 'user'}
          <DefaultPeople
            multi={field.field.multi}
            bind:people
            disabled={readonly}
            error={errors.default_values}
          />
        {:else if setKind !== null && set}
          <DefaultValues
            values={setValues(set)}
            multi={field.field.multi}
            bind:chosen
            disabled={readonly}
          />
          {#if errors.default_values}<p class="error">{errors.default_values}</p>{/if}
        {:else if setKind !== null && !setError}
          <Spinner label={m.defaultsLoading()} />
        {/if}
        <Checkbox label={m.privateField()} hint={privateHint} bind:checked={hidden} />
        {#if required}
          <p class="required"><Asterisk aria-hidden="true" />{m.requiredNote()}</p>
        {/if}
      </fieldset>
      {#if !readonly}
        <div class="actions">
          <Button type="submit" variant="primary" loading={busy} disabled={!changed && !busy}>
            {m.saveChanges()}
          </Button>
        </div>
      {/if}
    </form>
  </section>

  {#if kind === 'group'}
    <Notice>{m.groupFieldNote()}</Notice>
  {:else if setKind === null}
    <p class="note">{m.simpleFieldNote()}</p>
  {:else if set}
    <ValueSetEditor
      {set}
      level={3}
      showName
      {readonly}
      onfork={fork}
      onchanged={() => {
        list.refresh();
      }}
    />
  {:else if setError}
    <ErrorState
      error={setError}
      level={3}
      onretry={async () => {
        if (setId === null) return;
        setError = null;
        try {
          setHandle = await ensureValueSet(setId);
        } catch (error) {
          setError = error;
        }
      }}
    />
  {/if}

  {#if !readonly}
    <div class="detach">
      <Button variant="danger" icon={Trash2} onclick={ondetach}>{m.detach()}</Button>
    </div>
  {/if}
</div>

<style>
  .panel {
    display: flex;
    flex-direction: column;
    gap: var(--space-6);
  }

  .about {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    color: var(--color-text-muted);
    font-size: var(--text-sm);
  }

  .about .type {
    color: var(--color-text);
    font-size: var(--text-md);
    font-weight: var(--weight-medium);
  }

  .about a {
    color: var(--color-link);
  }

  .section {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }

  h3 {
    font-size: var(--text-lg);
  }

  .form {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }

  .plain {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
    min-width: 0;
    margin: 0;
    padding: 0;
    border: none;
  }

  .required {
    display: flex;
    align-items: flex-start;
    gap: var(--space-1_5);
    color: var(--color-text-muted);
    font-size: var(--text-sm);
  }

  .required :global(svg) {
    flex: none;
    width: var(--icon-sm);
    height: var(--icon-sm);
    margin-top: 0.125rem;
  }

  .error {
    color: var(--color-danger);
    font-size: var(--text-sm);
  }

  .note {
    color: var(--color-text-muted);
    font-size: var(--text-sm);
  }

  .detach {
    padding-top: var(--space-4);
    border-top: 1px solid var(--color-border);
  }

  @media (width < 48rem) {
    .actions :global(button),
    .detach :global(button) {
      width: 100%;
    }
  }
</style>
