<script lang="ts">
  // Adding a value or changing one (FLD-032, FLD-043…FLD-049): name, description and color for
  // every kind; resolved for states, released with release and start dates for versions, the build
  // date for builds, and an owner for values with owners. Problems show under their inputs.
  import { untrack } from 'svelte';

  import type { AccountSummary, NewValue, ValueSet } from '#lib/api/schema.gen.ts';
  import { PersonName, type PersonLike } from '#lib/features/access/index.ts';
  import * as common from '#lib/i18n/messages/common.ts';
  import { fieldErrorText } from '#lib/i18n/messages/errors.ts';
  import Button from '#lib/ui/Button.svelte';
  import Checkbox from '#lib/ui/Checkbox.svelte';
  import Dialog from '#lib/ui/Dialog.svelte';
  import Notice from '#lib/ui/Notice.svelte';
  import TextField from '#lib/ui/TextField.svelte';
  import { toasts } from '#lib/ui/toast.svelte.ts';

  import ColorPicker from './ColorPicker.svelte';
  import { addValue, editValue, type SetValue, type ValueChanges } from './fields.ts';
  import * as m from './messages.ts';
  import PeopleDialog from './PeopleDialog.svelte';
  import { focusFirstError, refusal } from './problems.ts';

  interface Props {
    open?: boolean;
    set: ValueSet;
    /** The value to change; `null` adds one. */
    value?: SetValue | null;
    /** A new value goes right after this one; `null` puts it at the end. */
    after?: SetValue | null;
    onsaved?: () => unknown;
  }

  let { open = $bindable(false), set, value = null, after = null, onsaved }: Props = $props();

  type Owner = PersonLike & { id: number };

  const formId = $props.id();
  let name = $state('');
  let description = $state('');
  let colorScheme = $state<number | null>(null);
  let resolved = $state(false);
  let released = $state(false);
  let releaseDate = $state('');
  let startDate = $state('');
  let assembledOn = $state('');
  let owner = $state<Owner | null>(null);
  let archived = $state(false);
  let choosingOwner = $state(false);
  let busy = $state(false);
  let problem = $state<string | null>(null);
  let errors = $state<Record<string, string>>({});
  let form: HTMLFormElement | undefined = $state();

  const kind = $derived(set.kind);
  const editing = $derived(value !== null);
  const title = $derived(value ? m.editValueTitle({ name: value.name }) : m.newValueTitle());

  // Every opening starts from the value (or an empty form).
  $effect(() => {
    if (!open) return;
    untrack(() => {
      name = value?.name ?? '';
      description = value?.description ?? '';
      colorScheme = value?.color ?? null;
      resolved = value?.resolved ?? false;
      released = value?.released ?? false;
      releaseDate = value?.release_date ?? '';
      startDate = value?.start_date ?? '';
      assembledOn = value?.assembled_on ?? '';
      owner = value?.owner ?? null;
      archived = value?.archived ?? false;
      problem = null;
      errors = {};
    });
  });

  const date = (text: string): string | null => text || null;

  /** What the form holds, in the properties this kind of value has. */
  function entered(): ValueChanges {
    const shown: ValueChanges = {
      name: name.trim(),
      description: description.trim(),
      color: colorScheme,
    };
    if (kind === 'state') shown.resolved = resolved;
    if (kind === 'version') {
      shown.released = released;
      shown.release_date = date(releaseDate);
      shown.start_date = date(startDate);
    }
    if (kind === 'build') shown.assembled_on = date(assembledOn);
    if (editing) shown.archived = archived;
    return shown;
  }

  /** The properties that differ from the value being changed. */
  function changesOf(current: SetValue, shown: ValueChanges): ValueChanges {
    const changes: Record<string, unknown> = {};
    for (const [key, next] of Object.entries(shown) as [keyof ValueChanges, unknown][]) {
      if (current[key] !== next) changes[key] = next;
    }
    if (kind === 'owned' && (current.owner?.id ?? null) !== (owner?.id ?? null)) {
      changes.owner = owner;
    }
    return changes;
  }

  async function submit(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    problem = null;
    errors = {};
    if (!name.trim()) {
      errors.name = fieldErrorText.required();
      focusFirstError(form);
      return;
    }
    const shown = entered();
    busy = true;
    try {
      if (value) {
        const changes = changesOf(value, shown);
        if (Object.keys(changes).length > 0) {
          await editValue(set.id, value, changes);
          toasts.show({ tone: 'success', message: m.valueSaved() });
        }
      } else {
        const input: NewValue = {
          name: shown.name ?? '',
          description: shown.description,
          color: shown.color,
          resolved: shown.resolved,
          released: shown.released,
          release_date: shown.release_date,
          start_date: shown.start_date,
          assembled_on: shown.assembled_on,
          ...(kind === 'owned' ? { owner_id: owner?.id ?? null } : {}),
          ...(after ? { after: after.id } : {}),
        };
        await addValue(set.id, input);
        toasts.show({ tone: 'success', message: m.valueAdded() });
      }
      open = false;
      await onsaved?.();
    } catch (error) {
      const found = refusal(error, {
        own: {
          name: {
            taken: m.valueNameTaken,
            too_long: name.trim().length <= 100 ? m.setFull : fieldErrorText.too_long,
          },
        },
        paths: ['name', 'description', 'color', 'release_date', 'start_date', 'assembled_on'],
      });
      errors = found.fields;
      problem = found.message;
      focusFirstError(form);
    } finally {
      busy = false;
    }
  }

  function chooseOwner(chosen: AccountSummary[]): void {
    owner = chosen[0] ?? owner;
  }
</script>

<Dialog bind:open {title} dismissible={!busy}>
  <form id={formId} class="form" novalidate onsubmit={submit} bind:this={form}>
    {#if problem}<Notice tone="danger">{problem}</Notice>{/if}
    {#if !editing}
      <p class="position">
        {after ? m.positionAfter({ name: after.name }) : m.positionEnd()}
      </p>
    {/if}
    <TextField
      label={m.name()}
      bind:value={name}
      error={errors.name}
      maxlength={100}
      autocomplete="off"
      required
    />
    <TextField
      label={m.description()}
      bind:value={description}
      hint={m.descriptionHint()}
      error={errors.description}
      maxlength={1000}
      autocomplete="off"
    />
    <ColorPicker bind:value={colorScheme} />
    {#if errors.color}<p class="error">{errors.color}</p>{/if}

    {#if kind === 'state'}
      <Checkbox label={m.resolved()} hint={m.resolvedHint()} bind:checked={resolved} />
    {:else if kind === 'version'}
      <Checkbox label={m.released()} hint={m.releasedHint()} bind:checked={released} />
      <div class="dates">
        <TextField
          type="date"
          label={m.releaseDate()}
          bind:value={releaseDate}
          error={errors.release_date}
        />
        <TextField
          type="date"
          label={m.startDate()}
          bind:value={startDate}
          error={errors.start_date}
        />
      </div>
    {:else if kind === 'build'}
      <TextField
        type="date"
        label={m.assembledOn()}
        bind:value={assembledOn}
        error={errors.assembled_on}
      />
    {:else if kind === 'owned'}
      <fieldset class="owner">
        <legend>{m.owner()}</legend>
        <p class="hint">{m.ownerHint()}</p>
        <div class="owner-row">
          {#if owner}
            <PersonName person={owner} withUsername />
          {:else}
            <span class="none">{m.noOwner()}</span>
          {/if}
        </div>
        <div class="owner-actions">
          <Button
            onclick={() => {
              choosingOwner = true;
            }}
          >
            {owner ? m.changeOwner() : m.chooseOwner()}
          </Button>
          {#if owner}
            <Button
              variant="ghost"
              onclick={() => {
                owner = null;
              }}
            >
              {m.removeOwner()}
            </Button>
          {/if}
        </div>
      </fieldset>
    {/if}

    {#if editing}
      <Checkbox label={m.archived()} hint={m.archivedHint()} bind:checked={archived} />
    {/if}
  </form>

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
      {editing ? m.saveChanges() : m.addValue()}
    </Button>
  {/snippet}
</Dialog>

{#if kind === 'owned'}
  <PeopleDialog
    bind:open={choosingOwner}
    title={m.chooseOwner()}
    label={m.owner()}
    mode="single"
    action={m.choose()}
    onconfirm={chooseOwner}
  />
{/if}

<style>
  .form {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }

  .position,
  .hint,
  .none {
    color: var(--color-text-muted);
    font-size: var(--text-sm);
  }

  .dates {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(11rem, 1fr));
    gap: var(--space-3);
  }

  .owner {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    min-width: 0;
    margin: 0;
    padding: 0;
    border: none;
  }

  legend {
    margin-bottom: var(--space-1);
    padding: 0;
    font-weight: var(--weight-medium);
  }

  .owner-row {
    display: flex;
    align-items: center;
    min-height: var(--control-md);
  }

  .owner-actions {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
  }

  .error {
    color: var(--color-danger);
    font-size: var(--text-sm);
  }
</style>
