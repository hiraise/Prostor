<script lang="ts">
  // «Изменить» of a tag (ISS-036): its name and color. The owner and the edit audience change
  // them; sharing has its own dialog.
  import type { Tag } from '#lib/api/schema.gen.ts';
  import { ColorPicker, focusFirstError, refusal } from '#lib/features/fields/index.ts';
  import { SourceSubscription } from '#lib/features/notifications/index.ts';
  import Button from '#lib/ui/Button.svelte';
  import Dialog from '#lib/ui/Dialog.svelte';
  import Notice from '#lib/ui/Notice.svelte';
  import TextField from '#lib/ui/TextField.svelte';
  import { toasts } from '#lib/ui/toast.svelte.ts';

  import * as m from './messages.ts';
  import { updateTag } from './sidebar.ts';

  interface Props {
    open?: boolean;
    tag: Tag | null;
    onsaved: () => unknown;
  }

  let { open = $bindable(false), tag, onsaved }: Props = $props();

  const id = $props.id();
  let form: HTMLFormElement | undefined = $state();
  let name = $state('');
  let color = $state<number | null>(null);
  let errors = $state<Record<string, string>>({});
  let problem = $state<string | null>(null);
  let busy = $state(false);

  $effect(() => {
    if (!open || !tag) return;
    name = tag.name;
    color = tag.color;
    errors = {};
    problem = null;
  });

  async function submit(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    if (!tag) return;
    errors = {};
    problem = null;
    if (name.trim() === '') {
      errors = { name: m.nameRequired() };
      focusFirstError(form);
      return;
    }
    const changes: { name?: string; color?: number | null } = {};
    if (name.trim() !== tag.name) changes.name = name.trim();
    if (color !== tag.color) changes.color = color;
    busy = true;
    try {
      if (Object.keys(changes).length > 0) {
        await updateTag(tag, changes);
        toasts.show({ tone: 'success', message: m.tagSaved() });
      }
      open = false;
      await onsaved();
    } catch (error) {
      const found = refusal(error, {
        own: { name: { taken: m.nameTaken } },
        conflict: m.conflict,
        paths: ['name', 'color'],
      });
      errors = found.fields;
      problem = found.message;
      focusFirstError(form);
    } finally {
      busy = false;
    }
  }
</script>

<Dialog bind:open title={m.editTag()} size="sm" dismissible={!busy}>
  <form id="{id}-form" class="form" bind:this={form} onsubmit={(event) => void submit(event)}>
    {#if problem}<Notice tone="danger">{problem}</Notice>{/if}
    <TextField label={m.name()} bind:value={name} error={errors.name ?? null} maxlength={100} />
    <ColorPicker bind:value={color} />
    <SourceSubscription open={open && tag !== null} source="tag" sourceId={tag?.id ?? 0} />
  </form>
  {#snippet footer()}
    <Button
      variant="ghost"
      disabled={busy}
      onclick={() => {
        open = false;
      }}
    >
      {m.cancel()}
    </Button>
    <Button variant="primary" type="submit" form="{id}-form" loading={busy}>{m.save()}</Button>
  {/snippet}
</Dialog>

<style>
  .form {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }
</style>
