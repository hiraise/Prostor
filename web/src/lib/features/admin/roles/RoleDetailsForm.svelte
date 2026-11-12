<script lang="ts">
  // A custom role's name and description (ACC-054), saved by an explicit button. The edit is sent
  // with the role's version; a change made meanwhile by someone else to the same fields is shown
  // here, and saving again keeps this person's values.
  import { untrack } from 'svelte';

  import { ApiError } from '#lib/api/client.ts';
  import type { Role } from '#lib/api/schema.gen.ts';
  import { fieldErrorText, fieldErrorTexts } from '#lib/i18n/messages/errors.ts';
  import { ConflictError } from '#lib/store/index.ts';
  import Button from '#lib/ui/Button.svelte';
  import Notice from '#lib/ui/Notice.svelte';
  import TextField from '#lib/ui/TextField.svelte';
  import { showError, toasts } from '#lib/ui/toast.svelte.ts';

  import * as m from './messages.ts';
  import { saveRoleDetails } from './roles.ts';

  let { role }: { role: Role } = $props();

  // What the fields were filled from; the server's newer values replace untouched fields only.
  let base = $state(untrack(() => ({ name: role.name, description: role.description })));
  let name = $state(untrack(() => role.name));
  let description = $state(untrack(() => role.description));
  let busy = $state(false);
  let conflict = $state(false);
  let errors = $state<Record<string, string>>({});
  let form: HTMLFormElement | undefined = $state();

  const own = { name: { taken: m.nameTaken } };
  const changed = $derived(name.trim() !== role.name || description.trim() !== role.description);

  $effect(() => {
    const server = { name: role.name, description: role.description };
    untrack(() => {
      if (!busy) adopt(server);
    });
  });

  function adopt(server: { name: string; description: string }): void {
    if (name === base.name) name = server.name;
    if (description === base.description) description = server.description;
    base = server;
  }

  function focusFirstError(): void {
    form?.querySelector<HTMLElement>('[aria-invalid="true"]')?.focus();
  }

  async function submit(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    conflict = false;
    errors = {};
    if (!name.trim()) {
      errors.name = fieldErrorText.required();
      queueMicrotask(focusFirstError);
      return;
    }
    if (!changed) return;
    busy = true;
    try {
      const saved = await saveRoleDetails(role.id, {
        name: name.trim(),
        description: description.trim(),
      });
      name = saved.name;
      description = saved.description;
      base = { name: saved.name, description: saved.description };
      toasts.show({ tone: 'success', message: m.roleSaved() });
    } catch (error) {
      // The fields keep what was typed; the role shows the server's values.
      base = { name: role.name, description: role.description };
      if (error instanceof ConflictError) {
        conflict = true;
      } else if (error instanceof ApiError && error.code === 'invalid_request') {
        errors = fieldErrorTexts(error.body?.fields ?? [], own);
        queueMicrotask(focusFirstError);
      } else {
        showError(error);
      }
    } finally {
      busy = false;
    }
  }
</script>

<form class="form" novalidate onsubmit={submit} bind:this={form}>
  {#if conflict}<Notice tone="warning">{m.editConflict()}</Notice>{/if}
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
    maxlength={500}
    autocomplete="off"
  />
  <div class="actions">
    <Button type="submit" variant="primary" loading={busy} disabled={!changed && !busy}>
      {m.saveChanges()}
    </Button>
  </div>
</form>

<style>
  .form {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
    max-width: 32rem;
  }

  @media (width < 48rem) {
    .actions :global(button) {
      width: 100%;
    }
  }
</style>
