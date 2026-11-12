<script lang="ts">
  // Creating a value set (FLD-029): a name and a type; values are added on the set's page.
  import type { SetKind, ValueSet } from '#lib/api/schema.gen.ts';
  import * as common from '#lib/i18n/messages/common.ts';
  import { fieldErrorText } from '#lib/i18n/messages/errors.ts';
  import Button from '#lib/ui/Button.svelte';
  import Dialog from '#lib/ui/Dialog.svelte';
  import Notice from '#lib/ui/Notice.svelte';
  import Select from '#lib/ui/Select.svelte';
  import TextField from '#lib/ui/TextField.svelte';

  import { SET_KINDS, createValueSet } from './fields.ts';
  import * as m from './messages.ts';
  import { focusFirstError, refusal } from './problems.ts';

  interface Props {
    open?: boolean;
    oncreated: (set: ValueSet) => unknown;
  }

  let { open = $bindable(false), oncreated }: Props = $props();

  const formId = $props.id();
  let name = $state('');
  let kind = $state<SetKind>('enum');
  let busy = $state(false);
  let problem = $state<string | null>(null);
  let errors = $state<Record<string, string>>({});
  let form: HTMLFormElement | undefined = $state();

  const kindOptions = $derived(SET_KINDS.map((value) => ({ value, label: m.kindNames[value]() })));

  // Every opening starts from a clean form.
  $effect(() => {
    if (!open) return;
    name = '';
    kind = 'enum';
    problem = null;
    errors = {};
  });

  async function submit(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    problem = null;
    errors = {};
    if (!name.trim()) {
      errors.name = fieldErrorText.required();
      focusFirstError(form);
      return;
    }
    busy = true;
    try {
      const set = await createValueSet(name.trim(), kind);
      open = false;
      await oncreated(set);
    } catch (error) {
      const found = refusal(error, {
        own: { name: { taken: m.setNameTaken } },
        paths: ['name', 'kind'],
      });
      errors = found.fields;
      problem = found.message;
      focusFirstError(form);
    } finally {
      busy = false;
    }
  }
</script>

<Dialog bind:open title={m.newSetTitle()} dismissible={!busy}>
  <form id={formId} class="form" novalidate onsubmit={submit} bind:this={form}>
    {#if problem}<Notice tone="danger">{problem}</Notice>{/if}
    <TextField
      label={m.name()}
      bind:value={name}
      error={errors.name}
      maxlength={100}
      autocomplete="off"
      required
    />
    <Select
      label={m.fieldType()}
      options={kindOptions}
      error={errors.kind}
      bind:value={
        () => kind,
        (next: string) => {
          kind = SET_KINDS.find((item) => item === next) ?? 'enum';
        }
      }
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
    <Button type="submit" form={formId} variant="primary" loading={busy}>{m.createSet()}</Button>
  {/snippet}
</Dialog>

<style>
  .form {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }
</style>
