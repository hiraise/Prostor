<script lang="ts">
  // A field's name and aliases (FLD-016…FLD-018): shared by every project using it. They change with
  // `system.write`, or by someone who configures every project using a field that new projects do
  // not get; the server decides. Saved with the version they were read on.
  import { untrack } from 'svelte';

  import { ApiError } from '#lib/api/client.ts';
  import type { Field } from '#lib/api/schema.gen.ts';
  import { fieldErrorText } from '#lib/i18n/messages/errors.ts';
  import { can, canAnywhere } from '#lib/session/index.ts';
  import { ConflictError } from '#lib/store/index.ts';
  import Button from '#lib/ui/Button.svelte';
  import Notice from '#lib/ui/Notice.svelte';
  import TextField from '#lib/ui/TextField.svelte';
  import { toasts } from '#lib/ui/toast.svelte.ts';

  import { parseAliases, updateField } from './fields.ts';
  import * as m from './messages.ts';
  import { focusFirstError, refusal } from './problems.ts';

  let { field }: { field: Field } = $props();

  const uid = $props.id();
  let name = $state(untrack(() => field.name));
  let aliasText = $state(untrack(() => field.aliases.join(', ')));
  let busy = $state(false);
  let conflict = $state(false);
  let problem = $state<string | null>(null);
  let errors = $state<Record<string, string>>({});
  let form: HTMLFormElement | undefined = $state();

  const mayEdit = $derived(
    can('system.write') || (canAnywhere('project.update') && !field.auto_attach),
  );
  const aliases = $derived(parseAliases(aliasText));
  const changed = $derived(
    name.trim() !== field.name || aliases.join('\n') !== field.aliases.join('\n'),
  );

  async function submit(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    conflict = false;
    problem = null;
    errors = {};
    if (!name.trim()) {
      errors.name = fieldErrorText.required();
      focusFirstError(form);
      return;
    }
    if (!changed) return;
    busy = true;
    try {
      const saved = await updateField(field.id, { name: name.trim(), aliases });
      name = saved.name;
      aliasText = saved.aliases.join(', ');
      toasts.show({ tone: 'success', message: m.fieldSaved() });
    } catch (error) {
      if (error instanceof ConflictError) {
        conflict = true;
      } else if (error instanceof ApiError && error.code === 'conflict') {
        conflict = true;
      } else {
        const found = refusal(error, {
          own: {
            name: { taken: m.fieldNameTaken, invalid: m.fieldNameInvalid },
            aliases: { invalid: m.tooManyAliases, too_long: m.tooManyAliases },
          },
          paths: ['name', 'aliases'],
        });
        errors = found.fields;
        problem = found.message;
        focusFirstError(form);
      }
    } finally {
      busy = false;
    }
  }
</script>

<section class="section" aria-labelledby="{uid}-title">
  <h2 id="{uid}-title">{m.nameAndAliases()}</h2>
  <p class="note">{m.sharedHint()}</p>
  {#if !mayEdit && field.auto_attach}<p class="note">{m.autoAttachedRename()}</p>{/if}
  {#if conflict}<Notice tone="warning">{m.fieldConflict()}</Notice>{/if}
  {#if problem}<Notice tone="danger">{problem}</Notice>{/if}
  <form class="form" novalidate onsubmit={submit} bind:this={form}>
    <fieldset class="plain" disabled={!mayEdit}>
      <TextField
        label={m.name()}
        bind:value={name}
        hint={m.fieldNameHint()}
        error={errors.name}
        maxlength={100}
        autocomplete="off"
        required
      />
      <TextField
        label={m.aliases()}
        bind:value={aliasText}
        hint={m.aliasesHint()}
        error={errors.aliases}
        autocomplete="off"
      />
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
