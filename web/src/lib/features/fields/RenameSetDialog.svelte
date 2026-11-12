<script lang="ts">
  // Renaming a value set: saved with the version it was read on; a rename made meanwhile is
  // reported here, and saving again keeps this person's name.
  import { untrack } from 'svelte';

  import type { ValueSet } from '#lib/api/schema.gen.ts';
  import * as common from '#lib/i18n/messages/common.ts';
  import { fieldErrorText } from '#lib/i18n/messages/errors.ts';
  import { ConflictError } from '#lib/store/index.ts';
  import Button from '#lib/ui/Button.svelte';
  import Dialog from '#lib/ui/Dialog.svelte';
  import Notice from '#lib/ui/Notice.svelte';
  import TextField from '#lib/ui/TextField.svelte';
  import { toasts } from '#lib/ui/toast.svelte.ts';

  import { renameValueSet } from './fields.ts';
  import * as m from './messages.ts';
  import { focusFirstError, refusal } from './problems.ts';

  let { open = $bindable(false), set }: { open?: boolean; set: ValueSet } = $props();

  const formId = $props.id();
  let name = $state('');
  let busy = $state(false);
  let problem = $state<string | null>(null);
  let conflict = $state(false);
  let errors = $state<Record<string, string>>({});
  let form: HTMLFormElement | undefined = $state();

  $effect(() => {
    if (!open) return;
    untrack(() => {
      name = set.name;
      problem = null;
      conflict = false;
      errors = {};
    });
  });

  async function submit(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    problem = null;
    conflict = false;
    errors = {};
    if (!name.trim()) {
      errors.name = fieldErrorText.required();
      focusFirstError(form);
      return;
    }
    if (name.trim() === set.name) {
      open = false;
      return;
    }
    busy = true;
    try {
      await renameValueSet(set.id, name.trim());
      open = false;
      toasts.show({ tone: 'success', message: m.setRenamed() });
    } catch (error) {
      if (error instanceof ConflictError) {
        conflict = true;
      } else {
        const found = refusal(error, { own: { name: { taken: m.setNameTaken } }, paths: ['name'] });
        errors = found.fields;
        problem = found.message;
        focusFirstError(form);
      }
    } finally {
      busy = false;
    }
  }
</script>

<Dialog bind:open title={m.renameSetTitle()} size="sm" dismissible={!busy}>
  <form id={formId} class="form" novalidate onsubmit={submit} bind:this={form}>
    {#if conflict}<Notice tone="warning">{m.renameConflict()}</Notice>{/if}
    {#if problem}<Notice tone="danger">{problem}</Notice>{/if}
    <TextField
      label={m.name()}
      bind:value={name}
      error={errors.name}
      maxlength={100}
      autocomplete="off"
      required
    />
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
    <Button type="submit" form={formId} variant="primary" loading={busy}>{m.save()}</Button>
  {/snippet}
</Dialog>

<style>
  .form {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }
</style>
