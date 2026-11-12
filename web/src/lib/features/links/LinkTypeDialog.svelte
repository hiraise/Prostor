<script lang="ts">
  // Creating a link type (LNK-019) and changing one (LNK-020) in one dialog: the internal name, the
  // direction, and the words each side uses. An undirected type has one name for both sides, so
  // the target side is hidden. Editing warns that new words replace the old ones in searches and
  // commands, and that dropping the direction drops the target side. Field errors from the server
  // show under their inputs; a change made meanwhile is reported here, and saving again keeps
  // this person's version.
  import { untrack } from 'svelte';

  import { ApiError } from '#lib/api/client.ts';
  import type { LinkDirection, LinkType } from '#lib/api/schema.gen.ts';
  import * as common from '#lib/i18n/messages/common.ts';
  import { fieldErrorText, fieldErrorTexts } from '#lib/i18n/messages/errors.ts';
  import { ConflictError } from '#lib/store/index.ts';
  import Button from '#lib/ui/Button.svelte';
  import Dialog from '#lib/ui/Dialog.svelte';
  import Notice from '#lib/ui/Notice.svelte';
  import RadioGroup, { type RadioOption } from '#lib/ui/RadioGroup.svelte';
  import TextField from '#lib/ui/TextField.svelte';
  import { describeError, toasts } from '#lib/ui/toast.svelte.ts';

  import {
    createLinkType,
    DIRECTIONS,
    updateLinkType,
    type LinkTypeChanges,
  } from './link-types.ts';
  import * as m from './messages.ts';

  interface Props {
    open?: boolean;
    /** The type to change; `null` creates a new one. */
    type?: LinkType | null;
    /** A type was created or saved. */
    ondone?: (type: LinkType) => unknown;
  }

  let { open = $bindable(false), type = null, ondone }: Props = $props();

  /** The longest name the server takes. */
  const MAX_NAME = 50;

  const formId = $props.id();
  let name = $state('');
  let source = $state('');
  let target = $state('');
  let direction = $state<LinkDirection>('directed');
  let busy = $state(false);
  let problem = $state<string | null>(null);
  let conflict = $state(false);
  let errors = $state<Record<string, string>>({});
  let form: HTMLFormElement | undefined = $state();
  /** What the inputs were filled from; after a conflict untouched inputs take the server's values. */
  let base = { name: '', source: '', target: '', direction: 'directed' as LinkDirection };

  const options = $derived<RadioOption[]>(
    DIRECTIONS.map((value) => ({
      value,
      label: m.directionNames[value](),
      hint: m.directionHints[value](),
    })),
  );
  const undirected = $derived(direction === 'undirected');
  const renamed = $derived(
    type !== null &&
      (name.trim() !== type.name ||
        source.trim() !== type.source_name ||
        (!undirected && type.target_name !== null && target.trim() !== type.target_name)),
  );
  const dropsTarget = $derived(type !== null && undirected && type.target_name !== null);

  $effect(() => {
    if (!open) return;
    untrack(() => {
      name = type?.name ?? '';
      source = type?.source_name ?? '';
      target = type?.target_name ?? '';
      direction = type?.direction ?? 'directed';
      base = { name, source, target, direction };
      problem = null;
      conflict = false;
      errors = {};
    });
  });

  function focusFirstError(): void {
    queueMicrotask(() => {
      form?.querySelector<HTMLElement>('[aria-invalid="true"]')?.focus();
    });
  }

  /** Takes the server's values into the inputs this person did not touch. */
  function adopt(server: LinkType): void {
    const theirs = {
      name: server.name,
      source: server.source_name,
      target: server.target_name ?? '',
      direction: server.direction,
    };
    if (name === base.name) name = theirs.name;
    if (source === base.source) source = theirs.source;
    if (target === base.target) target = theirs.target;
    if (direction === base.direction) direction = theirs.direction;
    base = theirs;
  }

  /** Problems the form finds before asking the server. */
  function check(): Record<string, string> {
    const found: Record<string, string> = {};
    if (!name.trim()) found.name = fieldErrorText.required();
    if (!source.trim()) found.source_name = fieldErrorText.required();
    if (!undirected) {
      if (!target.trim()) found.target_name = fieldErrorText.required();
      else if (target.trim().toLowerCase() === source.trim().toLowerCase()) {
        found.target_name = m.sameSides();
      }
    }
    return found;
  }

  /** What changed against the type, for a patch. */
  function changes(current: LinkType): LinkTypeChanges {
    const result: LinkTypeChanges = {};
    if (name.trim() !== current.name) result.name = name.trim();
    if (source.trim() !== current.source_name) result.source_name = source.trim();
    if (direction !== current.direction) result.direction = direction;
    if (undirected) {
      if (current.target_name !== null) result.target_name = null;
    } else if (target.trim() !== current.target_name) {
      result.target_name = target.trim();
    }
    return result;
  }

  async function submit(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    problem = null;
    conflict = false;
    errors = check();
    if (Object.keys(errors).length > 0) {
      focusFirstError();
      return;
    }
    busy = true;
    try {
      let done: LinkType;
      if (type === null) {
        done = await createLinkType({
          name: name.trim(),
          source_name: source.trim(),
          target_name: undirected ? undefined : target.trim(),
          direction,
        });
        toasts.show({ tone: 'success', message: m.created() });
      } else {
        const changed = changes(type);
        if (Object.keys(changed).length === 0) {
          open = false;
          return;
        }
        done = await updateLinkType(type.id, changed);
        toasts.show({ tone: 'success', message: m.saved() });
      }
      open = false;
      await ondone?.(done);
    } catch (error) {
      if (error instanceof ConflictError) {
        conflict = true;
        adopt(error.server as LinkType);
      } else if (error instanceof ApiError && error.code === 'invalid_request') {
        const nameTexts = { invalid: m.nameInvalid };
        errors = fieldErrorTexts(error.body?.fields ?? [], {
          name: { ...nameTexts, taken: m.typeNameTaken },
          source_name: { ...nameTexts, taken: m.sideNameTaken },
          target_name: { ...nameTexts, taken: m.sideNameTaken },
        });
        if (Object.keys(errors).length === 0) problem = describeError(error).message;
        focusFirstError();
      } else {
        problem = describeError(error).message;
      }
    } finally {
      busy = false;
    }
  }
</script>

<Dialog
  bind:open
  title={type ? m.editTitle({ name: type.name }) : m.newTitle()}
  size="sm"
  dismissible={!busy}
>
  <form id={formId} class="form" novalidate onsubmit={submit} bind:this={form}>
    {#if conflict}<Notice tone="warning">{m.editConflict()}</Notice>{/if}
    {#if problem}<Notice tone="danger">{problem}</Notice>{/if}
    <TextField
      label={m.name()}
      hint={m.nameHint()}
      bind:value={name}
      error={errors.name}
      maxlength={MAX_NAME}
      autocomplete="off"
      required
    />
    <RadioGroup legend={m.direction()} {options} bind:value={direction} />
    <TextField
      label={undirected ? m.sharedName() : m.sourceName()}
      hint={undirected ? m.sharedHint() : m.sourceHint()}
      bind:value={source}
      error={errors.source_name}
      maxlength={MAX_NAME}
      autocomplete="off"
      required
    />
    {#if !undirected}
      <TextField
        label={m.targetName()}
        hint={m.targetHint()}
        bind:value={target}
        error={errors.target_name}
        maxlength={MAX_NAME}
        autocomplete="off"
        required
      />
    {/if}
    {#if dropsTarget && type?.target_name}
      <Notice tone="warning">{m.undirectedWarning({ name: type.target_name })}</Notice>
    {/if}
    {#if renamed}<Notice tone="info">{m.renameWarning()}</Notice>{/if}
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
      {type ? m.save() : m.create()}
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
