<script lang="ts">
  // The dialogs of a file's menu (CNT-020, CNT-024): details, rename, visibility, and the
  // confirmation of a delete. One set per page; `action` says which one is open for which file.
  // Problems are said inside the dialog.
  import type { Audience, Issue } from '#lib/api/schema.gen.ts';
  import { PersonName } from '#lib/features/access/index.ts';
  import { refusalText } from '#lib/features/fields/index.ts';
  import * as common from '#lib/i18n/messages/common.ts';
  import Time from '#lib/time/Time.svelte';
  import Button from '#lib/ui/Button.svelte';
  import Dialog from '#lib/ui/Dialog.svelte';
  import Notice from '#lib/ui/Notice.svelte';
  import TextField from '#lib/ui/TextField.svelte';
  import { toasts } from '#lib/ui/toast.svelte.ts';

  import AudiencePicker from '../audience/AudiencePicker.svelte';
  import { audienceText, isRestricted } from '../audience/audience.ts';
  import * as m from '../messages.ts';
  import { fileSize, type FileAction, type IssueFiles } from './files.svelte.ts';

  interface Props {
    issue: Issue;
    files: IssueFiles;
    action?: FileAction | null;
  }

  let { issue, files, action = $bindable(null) }: Props = $props();

  const uid = $props.id();
  let busy = $state(false);
  let problem = $state<string | null>(null);
  let name = $state('');
  let nameError = $state<string | null>(null);
  let audience = $state<Audience>({ users: [], groups: [] });
  let restricted = $state(false);

  const file = $derived(action?.file ?? null);

  // Each opening starts from the file as it is.
  $effect(() => {
    const opened = action;
    if (!opened) return;
    busy = false;
    problem = null;
    nameError = null;
    name = opened.file.name;
    audience = opened.file.audience;
    restricted = isRestricted(opened.file.audience);
  });

  function opened(kind: FileAction['kind']) {
    return {
      get: () => action?.kind === kind,
      set: (open: boolean) => {
        if (!open && !busy) action = null;
      },
    };
  }
  const details = opened('details');
  const renaming = opened('rename');
  const visibility = opened('visibility');
  const deleting = opened('delete');

  async function run(work: () => Promise<unknown>, done: string): Promise<void> {
    busy = true;
    problem = null;
    try {
      await work();
      busy = false;
      action = null;
      toasts.show({ tone: 'success', message: done });
    } catch (error) {
      problem = refusalText(error);
      busy = false;
    }
  }

  function saveName(): void {
    if (!file) return;
    const trimmed = name.trim();
    nameError = trimmed === '' ? m.fileNameEmpty() : null;
    if (nameError) return;
    const id = file.id;
    void run(() => files.rename(id, trimmed), m.fileRenamed());
  }

  function saveAudience(): void {
    if (!file) return;
    const id = file.id;
    const chosen = restricted ? audience : { users: [], groups: [] };
    void run(() => files.setAudience(id, chosen), m.fileVisibilitySaved());
  }

  function confirmDelete(): void {
    if (!file) return;
    const id = file.id;
    void run(() => files.remove(id), m.fileDeleted());
  }

  function close(): void {
    action = null;
  }
</script>

<Dialog bind:open={details.get, details.set} title={file?.name ?? m.fileDetails()} size="sm">
  {#if file}
    <dl class="facts">
      <dt>{m.fileSizeLabel()}</dt>
      <dd>{fileSize(file.size)}</dd>
      <dt>{m.fileType()}</dt>
      <dd>{file.content_type}</dd>
      {#if file.width !== null && file.height !== null}
        <dt>{m.fileDimensions()}</dt>
        <dd>{file.width} × {file.height}</dd>
      {/if}
      <dt>{m.uploadedBy()}</dt>
      <dd><PersonName person={file.author} /></dd>
      <dt>{m.uploadedAt()}</dt>
      <dd><Time value={file.created_at} /></dd>
      <dt>{m.fileVisibility()}</dt>
      <dd>{audienceText(file.audience) ?? m.audienceIssue()}</dd>
      {#if file.comment !== null}
        <dt>{m.fromComment()}</dt>
        <dd>#{file.comment}</dd>
      {/if}
    </dl>
  {/if}
  {#snippet footer()}
    <Button onclick={close}>{common.close()}</Button>
  {/snippet}
</Dialog>

<Dialog
  bind:open={renaming.get, renaming.set}
  title={m.renameTitle()}
  size="sm"
  dismissible={!busy}
>
  <form
    class="body"
    id="{uid}-rename"
    onsubmit={(event) => {
      event.preventDefault();
      saveName();
    }}
  >
    {#if problem}<Notice tone="danger">{problem}</Notice>{/if}
    <TextField label={m.fileName()} bind:value={name} error={nameError} required />
  </form>
  {#snippet footer()}
    <Button disabled={busy} onclick={close}>{common.cancel()}</Button>
    <Button variant="primary" type="submit" form="{uid}-rename" loading={busy}>{m.save()}</Button>
  {/snippet}
</Dialog>

<Dialog
  bind:open={visibility.get, visibility.set}
  title={m.fileVisibilityTitle()}
  size="md"
  dismissible={!busy}
>
  <div class="body">
    {#if problem}<Notice tone="danger">{problem}</Notice>{/if}
    {#if file && action?.kind === 'visibility'}
      <AudiencePicker
        project={issue.project.id}
        reporter={file.author}
        bind:value={audience}
        bind:restricted
        offer={isRestricted(issue.audience) ? issue.audience : undefined}
        allLabel={m.audienceIssue()}
      />
    {/if}
  </div>
  {#snippet footer()}
    <Button disabled={busy} onclick={close}>{common.cancel()}</Button>
    <Button variant="primary" loading={busy} onclick={saveAudience}>{m.save()}</Button>
  {/snippet}
</Dialog>

<Dialog
  bind:open={deleting.get, deleting.set}
  title={m.deleteFileTitle({ name: file?.name ?? '' })}
  size="sm"
  dismissible={!busy}
>
  <div class="body">
    {#if problem}<Notice tone="danger">{problem}</Notice>{/if}
    <p>{m.deleteFileText()}</p>
  </div>
  {#snippet footer()}
    <Button disabled={busy} onclick={close}>{common.cancel()}</Button>
    <Button variant="danger" loading={busy} onclick={confirmDelete}>{m.deleteFile()}</Button>
  {/snippet}
</Dialog>

<style>
  .body {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }

  .facts {
    display: grid;
    grid-template-columns: max-content minmax(0, 1fr);
    gap: var(--space-2) var(--space-4);
    margin: 0;
  }

  dt {
    color: var(--color-text-muted);
  }

  dd {
    margin: 0;
    overflow-wrap: anywhere;
  }
</style>
