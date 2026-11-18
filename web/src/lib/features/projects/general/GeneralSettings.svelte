<script lang="ts">
  // A project's general settings (PRJ-037…PRJ-039): the owner and creation date, the logo
  // (PRJ-007), then the name, the key with its former keys (PRJ-005), the description (Markdown in
  // the basic profile, with its preview, PRJ-006) and the default audience of new issues and
  // articles. Saved on the version shown; a clash with someone else's change is said where the edit
  // happened. Below: archive or restore (PRJ-061, PRJ-063) and delete (PRJ-064). An archived
  // project is read-only here: only restore and delete are offered. The owner changes on the
  // people page (PRJ-053).
  import Archive from '@lucide/svelte/icons/archive';
  import ArchiveRestore from '@lucide/svelte/icons/archive-restore';
  import Trash2 from '@lucide/svelte/icons/trash-2';
  import { untrack } from 'svelte';

  import { ApiError } from '#lib/api/client.ts';
  import type { GroupRef, Project } from '#lib/api/schema.gen.ts';
  import { groupName, PersonName } from '#lib/features/access/index.ts';
  import { MarkdownEditor } from '#lib/features/content/index.ts';
  import { errorText, fieldErrorText } from '#lib/i18n/messages/errors.ts';
  import { ConflictError } from '#lib/store/index.ts';
  import Time from '#lib/time/Time.svelte';
  import Button from '#lib/ui/Button.svelte';
  import Notice from '#lib/ui/Notice.svelte';
  import PageHeader from '#lib/ui/PageHeader.svelte';
  import TextField from '#lib/ui/TextField.svelte';
  import { describeError, showError, toasts } from '#lib/ui/toast.svelte.ts';

  import ArchiveDialog from '../ArchiveDialog.svelte';
  import DeleteProjectDialog from '../DeleteProjectDialog.svelte';
  import * as shared from '../messages.ts';
  import { projectFieldTexts } from '../problems.ts';
  import {
    canConfigure,
    canDelete,
    checkProject,
    restoreProject,
    updateProject,
    type ProjectSettings,
  } from '../projects.ts';
  import AudienceDialog from './AudienceDialog.svelte';
  import LogoForm from './LogoForm.svelte';
  import * as m from './messages.ts';

  interface Props {
    project: Project;
    /** The project is being deleted; the route leaves for the list. */
    ondeleted: () => unknown;
    /** The key changed; the route moves to the address with the new key. */
    onkeychanged?: (project: Project) => unknown;
  }

  let { project, ondeleted, onkeychanged }: Props = $props();

  /** Typing pauses this long before a new name or key is checked. */
  const CHECK_MS = 300;
  /** Fields the form shows problems under; others are reported above it. */
  const FIELDS = ['name', 'key', 'description'];

  const id = $props.id();
  // The form starts from the project as it is and keeps the person's edits until they save.
  let name = $state(untrack(() => project.name));
  let key = $state(untrack(() => project.key));
  let description = $state(untrack(() => project.description));
  let audience = $state.raw<GroupRef | null>(untrack(() => project.default_audience));
  let busy = $state(false);
  let errors = $state<Record<string, string>>({});
  let live = $state<Record<string, string>>({});
  let problem = $state<string | null>(null);
  let conflict = $state(false);
  let choosing = $state(false);
  let archiving = $state(false);
  let deleting = $state(false);
  let restoring = $state(false);
  let form: HTMLFormElement | undefined = $state();

  const archived = $derived(project.archived_at !== null);
  const editable = $derived(!archived && canConfigure(project));
  const typedKey = $derived(key.trim().toUpperCase());
  const changes = $derived.by(() => {
    const found: ProjectSettings = {};
    if (name.trim() !== project.name) found.name = name.trim();
    if (typedKey !== project.key) found.key = typedKey;
    if (description.trim() !== project.description) found.description = description.trim();
    if ((audience?.id ?? null) !== (project.default_audience?.id ?? null)) {
      found.default_audience_id = audience?.id ?? null;
    }
    return found;
  });
  const dirty = $derived(Object.keys(changes).length > 0);

  // A new name or key is checked a moment after typing stops (PRJ-018). The project's own name
  // and keys, current and former, are its own to use.
  $effect(() => {
    if (!editable) return;
    const typedName = name.trim();
    const checkName =
      typedName && typedName.toLowerCase() !== project.name.toLowerCase() ? typedName : undefined;
    const checkKey =
      typedKey && typedKey !== project.key && !project.former_keys.includes(typedKey)
        ? typedKey
        : undefined;
    if (!checkName && !checkKey) {
      live = {};
      return;
    }
    const controller = new AbortController();
    const timer = setTimeout(() => {
      checkProject({ name: checkName, key: checkKey }, controller.signal).then(
        (result) => {
          live = projectFieldTexts(result.fields);
        },
        // A failed check changes nothing: saving checks again.
        () => undefined,
      );
    }, CHECK_MS);
    return () => {
      clearTimeout(timer);
      controller.abort();
    };
  });

  function showSaved(): void {
    name = project.name;
    key = project.key;
    description = project.description;
    audience = project.default_audience;
    errors = {};
    live = {};
  }

  function forget(field: string): void {
    if (!(field in errors)) return;
    errors = Object.fromEntries(Object.entries(errors).filter(([path]) => path !== field));
  }

  function focusFirstError(): void {
    form?.querySelector<HTMLElement>('[aria-invalid="true"]')?.focus();
  }

  async function submit(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    problem = null;
    conflict = false;
    const found: Record<string, string> = {};
    if (!name.trim()) found.name = fieldErrorText.required();
    if (!typedKey) found.key = fieldErrorText.required();
    errors = found;
    if (Object.keys(found).length > 0) {
      queueMicrotask(focusFirstError);
      return;
    }
    if (!dirty) return;
    const sent = { ...changes };
    busy = true;
    try {
      const saved = await updateProject(project.id, sent);
      showSaved();
      toasts.show({ tone: 'success', message: m.settingsSaved() });
      if (sent.key !== undefined) await onkeychanged?.(saved);
    } catch (error) {
      if (error instanceof ConflictError) {
        // The row shows the other person's values now.
        showSaved();
        conflict = true;
      } else if (error instanceof ApiError && error.code === 'invalid_request') {
        const fields = error.body?.fields ?? [];
        errors = projectFieldTexts(fields);
        if (fields.some((field) => !FIELDS.includes(field.path))) {
          problem = errorText.invalid_request();
        }
        queueMicrotask(focusFirstError);
      } else {
        problem = describeError(error).message;
      }
    } finally {
      busy = false;
    }
  }

  async function restore(): Promise<void> {
    restoring = true;
    try {
      const restored = await restoreProject(project.id);
      showSaved();
      toasts.show({ tone: 'success', message: shared.projectRestored({ name: restored.name }) });
    } catch (error) {
      showError(error, restore);
    } finally {
      restoring = false;
    }
  }

  function archivedNow(): void {
    showSaved();
    conflict = false;
    problem = null;
  }
</script>

<svelte:head><title>{m.title()} · {project.name} · TaskMaster</title></svelte:head>

<PageHeader title={m.title()} />

<div class="page">
  <dl class="facts">
    <div>
      <dt>{shared.owner()}</dt>
      <dd>
        {#if project.owner}<PersonName person={project.owner} />{:else}—{/if}
      </dd>
    </div>
    <div>
      <dt>{shared.created()}</dt>
      <dd><Time value={project.created_at} kind="date" year="always" /></dd>
    </div>
  </dl>

  <LogoForm {project} {editable} />

  <form class="form" novalidate onsubmit={submit} bind:this={form}>
    {#if conflict}<Notice tone="warning">{m.changedMeanwhile()}</Notice>{/if}
    {#if problem}<Notice tone="danger">{problem}</Notice>{/if}

    <TextField
      label={shared.nameLabel()}
      bind:value={name}
      error={errors.name ?? live.name}
      maxlength={100}
      autocomplete="off"
      required={editable}
      readonly={!editable}
      oninput={() => {
        forget('name');
      }}
    />

    <div class="key-field">
      <TextField
        class="key"
        label={shared.keyLabel()}
        bind:value={key}
        hint={m.keyHint({ example: `${typedKey || project.key}-1` })}
        error={errors.key ?? live.key}
        maxlength={10}
        autocomplete="off"
        autocapitalize="characters"
        spellcheck="false"
        required={editable}
        readonly={!editable}
        oninput={() => {
          forget('key');
        }}
      />
      {#if project.former_keys.length > 0}
        <div class="former" role="group" aria-labelledby="{id}-former">
          <span id="{id}-former" class="former-label">{shared.formerKeys()}</span>
          <ul class="keys">
            {#each project.former_keys as former (former)}
              <li>{former}</li>
            {/each}
          </ul>
          <p class="hint">{m.formerKeysNote()}</p>
        </div>
      {/if}
    </div>

    <MarkdownEditor
      label={shared.descriptionLabel()}
      bind:value={description}
      profile="basic"
      hint={shared.descriptionHint()}
      error={errors.description}
      maxlength={10000}
      rows={8}
      readonly={!editable}
      oninput={() => {
        forget('description');
      }}
    />

    <fieldset class="audience" aria-describedby="{id}-audience-hint">
      <legend>{m.audience()}</legend>
      <p id="{id}-audience-hint" class="hint">{m.audienceHint()}</p>
      <div class="audience-value">
        <span class="current">{audience ? groupName(audience) : m.everyone()}</span>
        {#if editable}
          <span class="audience-actions">
            <Button
              onclick={() => {
                choosing = true;
              }}
            >
              {m.chooseAudience()}
            </Button>
            {#if audience}
              <Button
                variant="ghost"
                onclick={() => {
                  audience = null;
                }}
              >
                {m.clearAudience()}
              </Button>
            {/if}
          </span>
        {/if}
      </div>
    </fieldset>

    {#if editable}
      <div class="actions">
        <Button type="submit" variant="primary" loading={busy} disabled={!dirty}>
          {m.saveChanges()}
        </Button>
      </div>
    {/if}
  </form>

  {#if canConfigure(project) || canDelete(project)}
    <section class="zone" aria-labelledby="{id}-zone">
      <h2 id="{id}-zone">{m.dangerZone()}</h2>
      {#if canConfigure(project)}
        <div class="zone-item">
          {#if archived}
            <p class="hint">{m.restoreText()}</p>
            <div>
              <Button icon={ArchiveRestore} loading={restoring} onclick={restore}>
                {shared.restoreProject()}
              </Button>
            </div>
          {:else}
            <p class="hint">{m.archiveText()}</p>
            <div>
              <Button
                icon={Archive}
                onclick={() => {
                  archiving = true;
                }}
              >
                {shared.archiveProject()}
              </Button>
            </div>
          {/if}
        </div>
      {/if}
      {#if canDelete(project)}
        <div class="zone-item">
          <p class="hint">{m.deleteText()}</p>
          <div>
            <Button
              variant="danger"
              icon={Trash2}
              onclick={() => {
                deleting = true;
              }}
            >
              {shared.deleteProject()}
            </Button>
          </div>
        </div>
      {/if}
    </section>
  {/if}
</div>

<AudienceDialog
  bind:open={choosing}
  current={audience}
  onchoose={(chosen: GroupRef | null) => {
    audience = chosen;
  }}
/>
{#if canConfigure(project)}
  <ArchiveDialog bind:open={archiving} {project} onarchived={archivedNow} />
{/if}
{#if canDelete(project)}
  <DeleteProjectDialog bind:open={deleting} {project} {ondeleted} />
{/if}

<style>
  .page {
    display: flex;
    flex-direction: column;
    gap: var(--space-6);
    max-width: 40rem;
    padding: var(--space-2) var(--space-6) var(--space-12);
  }

  .facts {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2) var(--space-8);
    margin: 0;
  }

  .facts > div {
    display: flex;
    flex-direction: column;
    gap: var(--space-0_5);
  }

  dt {
    color: var(--color-text-muted);
    font-size: var(--text-sm);
  }

  dd {
    display: flex;
    align-items: center;
    min-height: var(--control-sm);
    margin: 0;
  }

  .form {
    display: flex;
    flex-direction: column;
    gap: var(--space-5);
  }

  .form :global(input[readonly]),
  .form :global(textarea[readonly]) {
    background: var(--color-surface-sunken);
  }

  .form :global(.key input) {
    font-family: var(--font-mono);
    text-transform: uppercase;
  }

  .key-field {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }

  .former {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-1) var(--space-2);
  }

  .former-label {
    font-size: var(--text-sm);
    font-weight: var(--weight-medium);
  }

  .keys {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-1);
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .keys li {
    padding: 0 var(--space-1_5);
    border: 1px solid var(--color-border-control);
    border-radius: var(--radius-sm);
    color: var(--color-text-muted);
    font-family: var(--font-mono);
    font-size: var(--text-sm);
  }

  .former .hint {
    flex-basis: 100%;
  }

  .hint {
    color: var(--color-text-muted);
    font-size: var(--text-sm);
  }

  .audience {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    min-width: 0;
    margin: 0;
    padding: 0;
    border: none;
  }

  legend {
    margin-bottom: var(--space-1);
    padding: 0;
    font-weight: var(--weight-medium);
  }

  .audience-value {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-2) var(--space-4);
    margin-top: var(--space-1);
  }

  .current {
    font-weight: var(--weight-medium);
    overflow-wrap: anywhere;
  }

  .audience-actions {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
  }

  .zone {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
    padding-top: var(--space-5);
    border-top: 1px solid var(--color-border);
  }

  h2 {
    font-size: var(--text-lg);
  }

  .zone-item {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }

  @media (width < 48rem) {
    .page {
      padding: var(--space-2) var(--space-4) var(--space-8);
    }

    .actions :global(button) {
      width: 100%;
    }
  }
</style>
