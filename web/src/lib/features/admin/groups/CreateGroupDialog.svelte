<script lang="ts">
  // Creating a group (ACC-023): a unique name, a description and auto-join. The group starts with
  // no members and no roles. Problems are reported inside the dialog.
  import { ApiError } from '#lib/api/client.ts';
  import type { Group } from '#lib/api/schema.gen.ts';
  import * as common from '#lib/i18n/messages/common.ts';
  import { fieldErrorText, fieldErrorTexts } from '#lib/i18n/messages/errors.ts';
  import Button from '#lib/ui/Button.svelte';
  import Checkbox from '#lib/ui/Checkbox.svelte';
  import Dialog from '#lib/ui/Dialog.svelte';
  import Notice from '#lib/ui/Notice.svelte';
  import TextField from '#lib/ui/TextField.svelte';
  import { describeError, toasts } from '#lib/ui/toast.svelte.ts';

  import { createGroup } from './groups.ts';
  import * as m from './messages.ts';

  interface Props {
    open?: boolean;
    oncreated: (group: Group) => unknown;
  }

  let { open = $bindable(false), oncreated }: Props = $props();

  const formId = $props.id();
  let name = $state('');
  let description = $state('');
  let autoJoin = $state(false);
  let busy = $state(false);
  let problem = $state<string | null>(null);
  let errors = $state<Record<string, string>>({});
  let form: HTMLFormElement | undefined = $state();

  const own = {
    name: { taken: m.nameTaken },
  };

  // Every opening starts from a clean form.
  $effect(() => {
    if (!open) return;
    name = '';
    description = '';
    autoJoin = false;
    problem = null;
    errors = {};
  });

  function focusFirstError(): void {
    form?.querySelector<HTMLElement>('[aria-invalid="true"]')?.focus();
  }

  async function submit(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    problem = null;
    errors = {};
    if (!name.trim()) {
      errors = { name: fieldErrorText.required() };
      queueMicrotask(focusFirstError);
      return;
    }
    busy = true;
    try {
      const group = await createGroup({
        name: name.trim(),
        description: description.trim(),
        auto_join: autoJoin,
      });
      open = false;
      toasts.show({ tone: 'success', message: m.groupCreated() });
      await oncreated(group);
    } catch (error) {
      if (error instanceof ApiError && error.code === 'invalid_request') {
        errors = fieldErrorTexts(error.body?.fields ?? [], own);
        queueMicrotask(focusFirstError);
      } else {
        problem = describeError(error).message;
      }
    } finally {
      busy = false;
    }
  }
</script>

<Dialog bind:open title={m.newGroupTitle()} dismissible={!busy}>
  <form id={formId} class="form" novalidate onsubmit={submit} bind:this={form}>
    {#if problem}<Notice tone="danger">{problem}</Notice>{/if}
    <TextField
      label={m.groupName()}
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
      maxlength={500}
      autocomplete="off"
    />
    <Checkbox label={m.autoJoin()} hint={m.autoJoinHint()} bind:checked={autoJoin} />
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
      {m.createGroup()}
    </Button>
  {/snippet}
</Dialog>

<style>
  .form {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }
</style>
