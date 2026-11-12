<script lang="ts">
  // Adding a field to a project (FLD-052, FLD-053): an existing field found by name or alias, with
  // the choice to share its values or copy them for the project (FLD-030); or a new field created
  // for the project. Its settings start from the field's own (FLD-019). Problems show in the dialog.
  import { untrack } from 'svelte';

  import type { AttachField, Field, FieldKind, ProjectField } from '#lib/api/schema.gen.ts';
  import * as common from '#lib/i18n/messages/common.ts';
  import { fieldErrorText } from '#lib/i18n/messages/errors.ts';
  import Button from '#lib/ui/Button.svelte';
  import Checkbox from '#lib/ui/Checkbox.svelte';
  import Dialog from '#lib/ui/Dialog.svelte';
  import Notice from '#lib/ui/Notice.svelte';
  import RadioGroup from '#lib/ui/RadioGroup.svelte';
  import Select from '#lib/ui/Select.svelte';
  import Spinner from '#lib/ui/Spinner.svelte';
  import TextField from '#lib/ui/TextField.svelte';
  import { toasts } from '#lib/ui/toast.svelte.ts';

  import { FIELD_KINDS, findFields, mayBeMulti, parseAliases, setKindOf } from './fields.ts';
  import * as m from './messages.ts';
  import { focusFirstError, refusal, refusalText, typeLabel } from './problems.ts';
  import type { ProjectFieldList } from './project-fields.svelte.ts';

  interface Props {
    open?: boolean;
    list: ProjectFieldList;
    onadded: (field: ProjectField) => unknown;
  }

  let { open = $bindable(false), list, onadded }: Props = $props();

  const formId = $props.id();
  let source = $state<'existing' | 'new'>('existing');
  let text = $state('');
  let found = $state<Field[]>([]);
  let cursor = $state<string | null>(null);
  let searched = $state(false);
  let loading = $state(false);
  let searchProblem = $state<string | null>(null);
  let chosen = $state<number | null>(null);
  let use = $state<'shared' | 'copy'>('shared');
  let name = $state('');
  let kind = $state<FieldKind>('enum');
  let multi = $state(false);
  let aliasText = $state('');
  let busy = $state(false);
  let problem = $state<string | null>(null);
  let errors = $state<Record<string, string>>({});
  let form: HTMLFormElement | undefined = $state();
  let request = 0;

  const attachedIds = $derived(new Set(list.items.map((item) => item.field.id)));
  const attachedNames = $derived(new Set(list.items.map((item) => item.field.name.toLowerCase())));
  const choice = $derived(found.find((field) => field.id === chosen));
  const kindOptions = $derived(
    FIELD_KINDS.map((value) => ({ value, label: m.kindNames[value]() })),
  );

  async function search(more: boolean): Promise<void> {
    const mine = ++request;
    loading = true;
    searchProblem = null;
    try {
      const page = await findFields(text, more ? cursor : null);
      if (mine !== request) return;
      found = more ? [...found, ...page.items] : page.items;
      cursor = page.next_cursor;
      searched = true;
    } catch (error) {
      if (mine === request) searchProblem = refusalText(error);
    } finally {
      if (mine === request) loading = false;
    }
  }

  // Every opening starts afresh.
  $effect(() => {
    if (!open) return;
    untrack(() => {
      source = 'existing';
      text = '';
      found = [];
      cursor = null;
      searched = false;
      chosen = null;
      use = 'shared';
      name = '';
      kind = 'enum';
      multi = false;
      aliasText = '';
      problem = null;
      errors = {};
    });
  });

  // Search a moment after typing stops; the first page at once.
  $effect(() => {
    if (!open || source !== 'existing') return;
    const typed = text;
    const timer = setTimeout(() => void search(false), typed ? 250 : 0);
    return () => {
      clearTimeout(timer);
    };
  });

  function unavailable(field: Field): string | null {
    if (attachedIds.has(field.id)) return m.inProject();
    if (attachedNames.has(field.name.toLowerCase())) return m.nameInProject();
    return null;
  }

  async function submit(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    problem = null;
    errors = {};
    let input: AttachField;
    if (source === 'existing') {
      if (!choice) {
        problem = m.chooseField();
        return;
      }
      input = { source: 'existing', field_id: choice.id, set: use };
    } else {
      if (!name.trim()) {
        errors.name = fieldErrorText.required();
        focusFirstError(form);
        return;
      }
      input = {
        source: 'new',
        name: name.trim(),
        kind,
        multi: mayBeMulti(kind) && multi,
        aliases: parseAliases(aliasText),
      };
    }
    busy = true;
    try {
      const added = await list.attach(input);
      open = false;
      toasts.show({
        tone: 'success',
        message: source === 'existing' ? m.attached() : m.createdInProject(),
      });
      await onadded(added);
    } catch (error) {
      const found = refusal(error, {
        own: {
          name: { taken: m.fieldNameTaken, invalid: m.fieldNameInvalid },
          field: { taken: m.nameTakenInProject },
          aliases: { invalid: m.tooManyAliases, too_long: m.tooManyAliases },
        },
        conflict: m.attachConflict,
        paths: source === 'new' ? ['name', 'aliases', 'multi'] : [],
      });
      errors = found.fields;
      problem = found.message;
      focusFirstError(form);
    } finally {
      busy = false;
    }
  }
</script>

<Dialog bind:open title={m.addField()} size="lg" dismissible={!busy}>
  {#if open}
    <form id={formId} class="form" novalidate onsubmit={submit} bind:this={form}>
      {#if problem}<Notice tone="danger">{problem}</Notice>{/if}
      <RadioGroup
        legend={m.whatToAdd()}
        options={[
          { value: 'existing', label: m.existingField(), hint: m.existingFieldHint() },
          { value: 'new', label: m.newField(), hint: m.newFieldHint() },
        ]}
        bind:value={
          () => source,
          (next: string) => {
            source = next === 'new' ? 'new' : 'existing';
            problem = null;
          }
        }
      />

      {#if source === 'existing'}
        <TextField
          label={m.findFields()}
          type="search"
          hint={m.findFieldsHint()}
          bind:value={text}
          autocomplete="off"
          spellcheck="false"
        />
        <fieldset class="results" aria-busy={loading}>
          <legend class="legend">{m.fieldsLegend()}</legend>
          {#if searchProblem}
            <p class="note danger">{searchProblem}</p>
          {:else if !searched}
            <p class="note"><Spinner label={common.loading()} /></p>
          {:else if found.length === 0}
            <p class="note">{m.noFieldsFound()}</p>
          {/if}
          <ul class="list">
            {#each found as field (field.id)}
              {@const reason = unavailable(field)}
              <li>
                <label class={['option', { off: reason }]}>
                  <input
                    type="radio"
                    name="{formId}-field"
                    value={field.id}
                    checked={chosen === field.id}
                    disabled={reason !== null}
                    onchange={() => {
                      chosen = field.id;
                    }}
                  />
                  <span class="field-name">{field.name}</span>
                  <span class="field-type">{typeLabel(field)}</span>
                  {#if reason}<span class="field-type">{reason}</span>{/if}
                </label>
              </li>
            {/each}
          </ul>
          {#if cursor}
            <div>
              <Button variant="ghost" {loading} onclick={() => search(true)}>
                {m.showMore()}
              </Button>
            </div>
          {/if}
        </fieldset>
        {#if choice && setKindOf(choice.kind) !== null && choice.set}
          <RadioGroup
            legend={m.setUse()}
            options={[
              {
                value: 'shared',
                label: `${m.shareValues()} (${choice.set.name})`,
                hint: m.shareValuesHint(),
              },
              { value: 'copy', label: m.copyValues(), hint: m.copyValuesHint() },
            ]}
            bind:value={
              () => use,
              (next: string) => {
                use = next === 'copy' ? 'copy' : 'shared';
              }
            }
          />
        {/if}
      {:else}
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
          bind:value={
            () => kind,
            (next: string) => {
              kind = FIELD_KINDS.find((item) => item === next) ?? 'enum';
            }
          }
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
      {/if}
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
      {source === 'existing' ? m.addField() : m.createField()}
    </Button>
  {/snippet}
</Dialog>

<style>
  .form {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }

  .results {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    min-width: 0;
    margin: 0;
    padding: 0;
    border: none;
  }

  .legend {
    margin-bottom: var(--space-1);
    padding: 0;
    font-weight: var(--weight-medium);
  }

  .list {
    display: flex;
    flex-direction: column;
    max-height: 18rem;
    margin: 0;
    padding: 0;
    overflow-y: auto;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    list-style: none;
  }

  .list:empty {
    display: none;
  }

  .option {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-1) var(--space-3);
    min-height: var(--control-md);
    padding: var(--space-1_5) var(--space-3);
    cursor: pointer;
  }

  .option:hover {
    background: var(--color-hover);
  }

  .option.off {
    cursor: default;
  }

  li + li .option {
    border-top: 1px solid var(--color-border);
  }

  input {
    width: 1rem;
    height: 1rem;
    margin: 0;
    accent-color: var(--color-ink);
  }

  @media (pointer: coarse) {
    input {
      width: 1.25rem;
      height: 1.25rem;
    }
  }

  .field-name {
    font-weight: var(--weight-medium);
    overflow-wrap: anywhere;
  }

  .field-type,
  .note {
    color: var(--color-text-muted);
    font-size: var(--text-sm);
  }

  .danger {
    color: var(--color-danger);
  }
</style>
