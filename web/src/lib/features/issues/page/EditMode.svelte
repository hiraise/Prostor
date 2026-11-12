<script lang="ts">
  // Edit mode of the issue page (ISS-018): the summary, the description, the text fields and the
  // visibility as one form. Ctrl/⌘+Enter or the button saves — the texts in one request on the
  // `text_version` editing began with, then each changed text field; Esc or the button cancels,
  // asking first when something changed. A text saved meanwhile by someone else opens the conflict
  // view (ISS-051); keeping on editing shows that text, as a diff, above the editors.
  import { onMount, untrack } from 'svelte';

  import type { Audience, Issue, IssueField } from '#lib/api/schema.gen.ts';
  import { MarkdownEditor } from '#lib/features/content/index.ts';
  import { refusalText } from '#lib/features/fields/index.ts';
  import { can } from '#lib/session/index.ts';
  import * as common from '#lib/i18n/messages/common.ts';
  import { ConflictError } from '#lib/store/index.ts';
  import Button from '#lib/ui/Button.svelte';
  import Dialog from '#lib/ui/Dialog.svelte';
  import Notice from '#lib/ui/Notice.svelte';
  import TextField from '#lib/ui/TextField.svelte';
  import { toasts } from '#lib/ui/toast.svelte.ts';

  import { saveTexts, type TextEdit, type Texts } from '../actions.ts';
  import AudiencePicker from '../audience/AudiencePicker.svelte';
  import { isRestricted } from '../audience/audience.ts';
  import { saveField } from '../fields/save.ts';
  import { issueFiles } from '../files/files.svelte.ts';
  import * as m from '../messages.ts';
  import ConflictDialog from './ConflictDialog.svelte';
  import DiffView from './DiffView.svelte';

  interface Props {
    issue: Issue;
    /** Edit mode ends: saved or cancelled. */
    onclose: () => unknown;
  }

  let { issue, onclose }: Props = $props();

  // Files dropped or pasted into the texts are attached to the issue at once (CNT-034).
  const pageFiles = issueFiles();
  const shelf = $derived(pageFiles?.() ?? null);
  const upload = $derived(
    shelf && can('attachment.create', issue.project.id) ? shelf.uploadForText : undefined,
  );
  const attachments = $derived(shelf?.items ?? []);

  const id = $props.id();
  // What editing began with: the form compares against it and saves on its text version.
  const start = untrack(() => ({
    summary: issue.summary,
    description: issue.description,
    audience: issue.audience,
    texts: new Map(
      issue.fields
        .filter((entry) => entry.field.kind === 'text' && entry.editable)
        .map((entry) => [entry.field.id, entry.text ?? '']),
    ),
  }));
  let textVersion = $state(untrack(() => issue.text_version));

  let summary = $state(start.summary);
  let description = $state(start.description);
  let audience = $state<Audience>(start.audience);
  let restricted = $state(isRestricted(start.audience));
  let texts = $state<Record<number, string>>(Object.fromEntries(start.texts));

  let busy = $state(false);
  let problem = $state<string | null>(null);
  let summaryError = $state<string | null>(null);
  let audienceError = $state<string | null>(null);
  let summaryInput = $state<HTMLInputElement>();
  let confirming = $state(false);
  /** The texts saved meanwhile, once a save met them. */
  let theirs = $state<Texts | null>(null);
  let conflictOpen = $state(false);

  const textFields = $derived(
    issue.fields.filter(
      (entry): entry is IssueField =>
        entry.field.kind === 'text' && start.texts.has(entry.field.id),
    ),
  );

  // The form replaces the button that opened it: focus goes to its first field.
  onMount(() => {
    summaryInput?.focus();
  });

  function sameAudience(a: Audience, b: Audience): boolean {
    const ids = (audience: Audience): string =>
      [
        ...audience.users.map((user) => `u${String(user.id)}`),
        ...audience.groups.map((group) => `g${String(group.id)}`),
      ]
        .sort()
        .join(',');
    return ids(a) === ids(b);
  }

  /** The changes of the one request, without unchanged parts. */
  function textEdit(): TextEdit {
    const edit: TextEdit = {};
    if (summary !== start.summary) edit.summary = summary;
    if (description !== start.description) edit.description = description;
    if (!sameAudience(audience, start.audience)) edit.audience = $state.snapshot(audience);
    return edit;
  }

  function changedTexts(): [number, string][] {
    return [...start.texts].flatMap(([fieldId, before]) => {
      const now = texts[fieldId] ?? '';
      return now === before ? [] : [[fieldId, now] as [number, string]];
    });
  }

  /** Someone saved the texts after editing began (ISS-048, ISS-051). */
  const changedMeanwhile = $derived(!busy && !conflictOpen && issue.text_version > textVersion);

  /** Shows their texts beside ours, as a save meeting them would. */
  function compare(): void {
    theirs = { summary: issue.summary, description: issue.description };
    textVersion = issue.text_version;
    conflictOpen = true;
  }

  const dirty = $derived(Object.keys(textEdit()).length > 0 || changedTexts().length > 0);

  async function save(): Promise<void> {
    if (busy) return;
    summaryError = summary.trim() ? null : m.summaryRequired();
    audienceError =
      restricted && audience.users.length + audience.groups.length === 0 ? m.audienceEmpty() : null;
    if (summaryError) {
      summaryInput?.focus();
      return;
    }
    if (audienceError) return;
    busy = true;
    problem = null;
    try {
      const edit = textEdit();
      if (Object.keys(edit).length > 0) {
        const saved = await saveTexts(issue.id, edit, textVersion);
        textVersion = saved.text_version;
      }
      for (const [fieldId, text] of changedTexts()) {
        const entry = issue.fields.find((other) => other.field.id === fieldId);
        if (!entry) continue;
        await saveField(issue.id, { ...entry, text }, { kind: 'set', value: text || null });
      }
      conflictOpen = false;
      toasts.show({ tone: 'success', message: m.issueSaved() });
      await onclose();
    } catch (error) {
      if (error instanceof ConflictError) {
        const server = error.server as Issue;
        theirs = { summary: server.summary, description: server.description };
        textVersion = server.text_version;
        conflictOpen = true;
      } else {
        problem = refusalText(error);
      }
    } finally {
      busy = false;
    }
  }

  function cancel(): void {
    if (dirty) confirming = true;
    else void onclose();
  }

  function keydown(event: KeyboardEvent): void {
    if (event.defaultPrevented || event.isComposing) return;
    if (event.key === 'Enter' && (event.ctrlKey || event.metaKey) && !event.altKey) {
      event.preventDefault();
      void save();
    } else if (event.key === 'Escape') {
      event.preventDefault();
      cancel();
    }
  }
</script>

<!-- Keys of the whole form: Ctrl/⌘+Enter and Esc wherever focus is inside it. -->
<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<form
  class="edit"
  aria-labelledby="{id}-title"
  aria-describedby="{id}-hint"
  novalidate
  onkeydown={keydown}
  onsubmit={(event) => {
    event.preventDefault();
    void save();
  }}
>
  <h2 class="sr-only" id="{id}-title">{m.editIssue()}</h2>
  {#if problem}<Notice tone="danger">{problem}</Notice>{/if}
  {#if changedMeanwhile}
    <Notice tone="warning">
      <span class="line">
        {m.changedWhileEditing()}
        <Button size="sm" onclick={compare}>{m.showDifferences()}</Button>
      </span>
    </Notice>
  {/if}
  {#if theirs && !conflictOpen}
    <div class="theirs" role="group" aria-label={m.theirVersion()}>
      <p class="caption">{m.theirVersion()}</p>
      {#if theirs.summary !== summary}
        <DiffView label={m.summary()} before={theirs.summary} after={summary} />
      {/if}
      {#if theirs.description !== description}
        <DiffView label={m.description()} before={theirs.description} after={description} />
      {/if}
    </div>
  {/if}
  <TextField
    label={m.summary()}
    bind:value={summary}
    bind:input={summaryInput}
    error={summaryError}
    required
    maxlength={255}
  />
  <MarkdownEditor
    label={m.description()}
    bind:value={description}
    issue={issue.id}
    rows={8}
    {upload}
    {attachments}
  />
  {#each textFields as entry (entry.field.id)}
    <MarkdownEditor
      label={entry.field.name}
      bind:value={texts[entry.field.id]}
      issue={issue.id}
      rows={4}
      {upload}
      {attachments}
    />
  {/each}
  <AudiencePicker
    project={issue.project.id}
    reporter={issue.reporter}
    bind:value={audience}
    bind:restricted
    error={audienceError}
  />
  <p class="hint" id="{id}-hint">{m.editHint()}</p>
  <div class="buttons">
    <Button type="button" disabled={busy} onclick={cancel}>{common.cancel()}</Button>
    <Button type="submit" variant="primary" loading={busy}>{m.save()}</Button>
  </div>
</form>

<Dialog bind:open={confirming} title={m.discardTitle()} size="sm">
  <p>{m.discardText()}</p>
  {#snippet footer()}
    <Button
      onclick={() => {
        confirming = false;
      }}>{m.keepEditing()}</Button
    >
    <Button
      variant="danger"
      onclick={() => {
        confirming = false;
        void onclose();
      }}>{m.discard()}</Button
    >
  {/snippet}
</Dialog>

{#if theirs}
  <ConflictDialog
    bind:open={conflictOpen}
    {theirs}
    mine={{ summary, description }}
    {busy}
    onkeep={save}
    oncontinue={() => {
      conflictOpen = false;
    }}
    ondiscard={() => {
      conflictOpen = false;
      void onclose();
    }}
  />
{/if}

<style>
  .line {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-2);
  }

  .edit {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }

  .theirs {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    padding: var(--space-3);
    border: 1px solid var(--color-warning);
    border-radius: var(--radius-md);
    background: var(--color-warning-bg);
  }

  .caption {
    font-weight: var(--weight-medium);
  }

  .hint {
    color: var(--color-text-muted);
    font-size: var(--text-sm);
  }

  .buttons {
    display: flex;
    flex-wrap: wrap;
    justify-content: flex-end;
    gap: var(--space-2);
  }
</style>
