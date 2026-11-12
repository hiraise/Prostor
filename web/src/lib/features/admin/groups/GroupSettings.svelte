<script lang="ts">
  // A group's settings (ACC-024): name, description and auto-join, saved on the version shown. A
  // built-in group keeps its name and auto-join (ACC-021). Deleting a group with a replacement
  // (ACC-032) starts here.
  import Trash2 from '@lucide/svelte/icons/trash-2';
  import { untrack } from 'svelte';

  import { ApiError } from '#lib/api/client.ts';
  import type { Group } from '#lib/api/schema.gen.ts';
  import { fieldErrorText, fieldErrorTexts } from '#lib/i18n/messages/errors.ts';
  import { ConflictError } from '#lib/store/index.ts';
  import Button from '#lib/ui/Button.svelte';
  import Checkbox from '#lib/ui/Checkbox.svelte';
  import Notice from '#lib/ui/Notice.svelte';
  import TextField from '#lib/ui/TextField.svelte';
  import { toasts } from '#lib/ui/toast.svelte.ts';

  import { problemOf, updateGroup, type GroupSettings } from './groups.ts';
  import * as m from './messages.ts';

  interface Props {
    group: Group;
    /** Opens the deletion of this group. */
    ondelete: () => void;
  }

  let { group, ondelete }: Props = $props();

  const id = $props.id();
  // The form starts from the group as it is and keeps the person's edits until they save.
  let name = $state(untrack(() => group.name));
  let description = $state(untrack(() => group.description));
  let autoJoin = $state(untrack(() => group.auto_join));
  let busy = $state(false);
  let errors = $state<Record<string, string>>({});
  let problem = $state<string | null>(null);
  let conflict = $state(false);
  let form: HTMLFormElement | undefined = $state();

  const own = { name: { taken: m.nameTaken } };

  const changes = $derived.by(() => {
    const found: GroupSettings = {};
    if (!group.builtin && name.trim() !== group.name) found.name = name.trim();
    if (description.trim() !== group.description) found.description = description.trim();
    if (!group.builtin && autoJoin !== group.auto_join) found.auto_join = autoJoin;
    return found;
  });
  const dirty = $derived(Object.keys(changes).length > 0);

  function showSaved(): void {
    name = group.name;
    description = group.description;
    autoJoin = group.auto_join;
  }

  function focusFirstError(): void {
    form?.querySelector<HTMLElement>('[aria-invalid="true"]')?.focus();
  }

  async function submit(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    errors = {};
    problem = null;
    conflict = false;
    if (!group.builtin && !name.trim()) {
      errors = { name: fieldErrorText.required() };
      queueMicrotask(focusFirstError);
      return;
    }
    if (!dirty) return;
    busy = true;
    try {
      await updateGroup(group.id, { ...changes });
      showSaved();
      toasts.show({ tone: 'success', message: m.settingsSaved() });
    } catch (error) {
      if (error instanceof ConflictError) {
        // The row shows the other person's values now.
        showSaved();
        conflict = true;
      } else if (error instanceof ApiError && error.code === 'invalid_request') {
        errors = fieldErrorTexts(error.body?.fields ?? [], own);
        queueMicrotask(focusFirstError);
      } else {
        problem = problemOf(error).message;
      }
    } finally {
      busy = false;
    }
  }
</script>

<section class="settings" aria-labelledby="{id}-title">
  <h2 id="{id}-title">{m.settingsTitle()}</h2>

  <form class="form" novalidate onsubmit={submit} bind:this={form}>
    {#if conflict}<Notice tone="warning">{m.changedMeanwhile()}</Notice>{/if}
    {#if problem}<Notice tone="danger">{problem}</Notice>{/if}
    {#if group.builtin}
      <p class="note">{m.builtinName()}</p>
    {:else}
      <TextField
        label={m.groupName()}
        bind:value={name}
        error={errors.name}
        maxlength={100}
        autocomplete="off"
        required
      />
    {/if}
    <TextField
      label={m.description()}
      bind:value={description}
      hint={m.descriptionHint()}
      error={errors.description}
      maxlength={500}
      autocomplete="off"
    />
    {#if group.builtin}
      <Checkbox label={m.autoJoin()} hint={m.builtinAutoJoinHint()} checked disabled />
    {:else}
      <Checkbox label={m.autoJoin()} hint={m.autoJoinHint()} bind:checked={autoJoin} />
    {/if}
    <div class="actions">
      <Button type="submit" variant="primary" loading={busy} disabled={!dirty}>
        {m.saveChanges()}
      </Button>
    </div>
  </form>

  {#if !group.builtin}
    <div class="danger-zone" aria-labelledby="{id}-delete" role="group">
      <h3 id="{id}-delete">{m.deleteGroupTitle()}</h3>
      <p class="note">{m.deleteGroupText()}</p>
      <div>
        <Button variant="danger" icon={Trash2} onclick={ondelete}>
          {m.deleteGroups({ count: 1 })}
        </Button>
      </div>
    </div>
  {/if}
</section>

<style>
  .settings {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }

  h2 {
    font-size: var(--text-lg);
  }

  h3 {
    font-size: var(--text-md);
    font-weight: var(--weight-semibold);
  }

  .form {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
    max-width: 32rem;
  }

  .note {
    max-width: 44rem;
    color: var(--color-text-muted);
    font-size: var(--text-sm);
  }

  .danger-zone {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    max-width: 32rem;
    padding-top: var(--space-4);
    border-top: 1px solid var(--color-border);
  }

  @media (width < 48rem) {
    .actions :global(button) {
      width: 100%;
    }
  }
</style>
