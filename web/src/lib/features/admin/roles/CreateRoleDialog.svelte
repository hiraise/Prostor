<script lang="ts">
  // Creating a custom role (ACC-054): a name and a description; its permissions are chosen on the
  // role's page right after. Problems are reported inside the dialog.
  import { ApiError } from '#lib/api/client.ts';
  import type { Role } from '#lib/api/schema.gen.ts';
  import * as common from '#lib/i18n/messages/common.ts';
  import { fieldErrorText, fieldErrorTexts } from '#lib/i18n/messages/errors.ts';
  import Button from '#lib/ui/Button.svelte';
  import Dialog from '#lib/ui/Dialog.svelte';
  import Notice from '#lib/ui/Notice.svelte';
  import TextField from '#lib/ui/TextField.svelte';
  import { describeError } from '#lib/ui/toast.svelte.ts';

  import * as m from './messages.ts';
  import { createRole } from './roles.ts';

  interface Props {
    open?: boolean;
    oncreated: (role: Role) => unknown;
  }

  let { open = $bindable(false), oncreated }: Props = $props();

  const formId = $props.id();
  let name = $state('');
  let description = $state('');
  let busy = $state(false);
  let problem = $state<string | null>(null);
  let errors = $state<Record<string, string>>({});
  let form: HTMLFormElement | undefined = $state();

  const own = { name: { taken: m.nameTaken } };

  // Every opening starts from a clean form.
  $effect(() => {
    if (!open) return;
    name = '';
    description = '';
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
      errors.name = fieldErrorText.required();
      queueMicrotask(focusFirstError);
      return;
    }
    busy = true;
    try {
      const role = await createRole(name.trim(), description.trim());
      open = false;
      await oncreated(role);
    } catch (error) {
      if (error instanceof ApiError && error.code === 'invalid_request') {
        errors = fieldErrorTexts(error.body?.fields ?? [], own);
        if (Object.keys(errors).length === 0) problem = describeError(error).message;
        queueMicrotask(focusFirstError);
      } else {
        problem = describeError(error).message;
      }
    } finally {
      busy = false;
    }
  }
</script>

<Dialog bind:open title={m.newRoleTitle()} dismissible={!busy}>
  <form id={formId} class="form" novalidate onsubmit={submit} bind:this={form}>
    {#if problem}<Notice tone="danger">{problem}</Notice>{/if}
    <TextField
      label={m.name()}
      bind:value={name}
      hint={m.nameHint()}
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
      {m.createRole()}
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
