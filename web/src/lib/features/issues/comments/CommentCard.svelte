<script lang="ts">
  // A comment in the activity stream (CNT-001, CNT-003…CNT-005, CNT-007, CNT-018, HIST-009,
  // HIST-011): author, time, «изменён», whom it is limited to (a tinted card), the text, its files
  // and the changes its author made right after posting it. «Ещё» edits, copies the link and
  // deletes; F2 anywhere in the card edits too. A deleted comment is a placeholder with
  // «Восстановить». Files dropped on the card are added to it. A long text is cut with a fade and
  // «Показать полностью» when the person keeps long comments collapsed. The comment of a deep link
  // is scrolled to, focused and highlighted.
  import Ellipsis from '@lucide/svelte/icons/ellipsis';
  import Link from '@lucide/svelte/icons/link';
  import Lock from '@lucide/svelte/icons/lock';
  import Pencil from '@lucide/svelte/icons/pencil';
  import Trash2 from '@lucide/svelte/icons/trash-2';
  import { tick } from 'svelte';

  import type { Audience, Comment, HistoryItem, Issue, Upload } from '#lib/api/schema.gen.ts';
  import { uploadFile } from '#lib/api/upload.ts';
  import { PersonName, personName } from '#lib/features/access/index.ts';
  import { MarkdownEditor, MarkdownView } from '#lib/features/content/index.ts';
  import { refusalText } from '#lib/features/fields/index.ts';
  import * as common from '#lib/i18n/messages/common.ts';
  import { preferences } from '#lib/session/index.ts';
  import { ConflictError, store } from '#lib/store/index.ts';
  import Time from '#lib/time/Time.svelte';
  import { announce } from '#lib/ui/announce.svelte.ts';
  import Button from '#lib/ui/Button.svelte';
  import { copyLink } from '#lib/ui/copy.ts';
  import IconButton from '#lib/ui/IconButton.svelte';
  import Menu, { type MenuEntry } from '#lib/ui/Menu.svelte';
  import Notice from '#lib/ui/Notice.svelte';
  import { showError, toasts } from '#lib/ui/toast.svelte.ts';

  import ChangeRecord from '../activity/ChangeRecord.svelte';
  import { audienceText, isRestricted } from '../audience/audience.ts';
  import * as m from '../messages.ts';
  import { issueFiles } from '../files/files.svelte.ts';
  import CommentFiles from './CommentFiles.svelte';
  import {
    addCommentFiles,
    commentHref,
    deleteComment,
    restoreComment,
    updateComment,
  } from './comments.ts';
  import CommentVisibility from './CommentVisibility.svelte';

  interface Props {
    comment: Comment;
    changes: HistoryItem[];
    issue: Issue;
    /** The comment of the deep link. */
    focused?: boolean;
    /** Sends a file; the app's upload by default (tests give their own). */
    upload?: (file: File) => Promise<Upload>;
  }

  let {
    comment: given,
    changes,
    issue,
    focused = false,
    upload = (file: File) => uploadFile(file),
  }: Props = $props();

  // The page's files: those added here join the attachments section (CNT-020).
  const pageFiles = issueFiles();

  const id = $props.id();
  /** The stored row follows edits made after the stream loaded. */
  const comment = $derived(store.entities.table('comment').get(given.id) ?? given);
  const collapse = $derived(preferences.value('long_comments') === 'collapsed');
  const restricted = $derived(isRestricted(comment.audience));
  const deleted = $derived(comment.deleted !== null);

  let card = $state<HTMLElement>();
  let text = $state<HTMLElement>();
  let long = $state(false);
  let expanded = $state(false);
  let busy = $state(false);
  let dragging = $state(false);

  // ---- Editing (CNT-004) ----

  let editing = $state(false);
  let draft = $state('');
  let draftUploads = $state<number[]>([]);
  let draftAudience = $state<Audience>({ users: [], groups: [] });
  let draftRestricted = $state(false);
  let problem = $state<string | null>(null);
  let audienceError = $state<string | null>(null);
  /** The comment as someone else saved it while this person edited. */
  let theirs = $state.raw<Comment | null>(null);

  function startEditing(): void {
    if (!comment.can.update || deleted || editing) return;
    draft = comment.text;
    draftUploads = [];
    draftAudience = comment.audience;
    draftRestricted = restricted;
    problem = null;
    audienceError = null;
    theirs = null;
    editing = true;
    // The menu that started editing is gone with the card's head: the focus goes to the text.
    void tick().then(() => card?.querySelector('textarea')?.focus());
  }

  function stopEditing(): void {
    editing = false;
    theirs = null;
    queueMicrotask(() => card?.focus());
  }

  async function save(): Promise<void> {
    if (busy) return;
    audienceError =
      draftRestricted && draftAudience.users.length + draftAudience.groups.length === 0
        ? m.audienceEmpty()
        : null;
    if (audienceError) return;
    if (draft.trim() === '' && comment.files.length === 0 && draftUploads.length === 0) {
      problem = m.commentEmpty();
      return;
    }
    busy = true;
    problem = null;
    try {
      const audience = draftRestricted ? draftAudience : { users: [], groups: [] };
      // After a conflict the row holds their version, so this saves over it on purpose.
      const saved = await updateComment(comment.id, draft, audience, draftUploads);
      pageFiles?.()?.add(saved.files);
      toasts.show({ tone: 'success', message: m.commentSaved() });
      stopEditing();
    } catch (error) {
      if (error instanceof ConflictError) theirs = error.server as Comment;
      else problem = refusalText(error);
    } finally {
      busy = false;
    }
  }

  function editKeydown(event: KeyboardEvent): void {
    if (event.key === 'Escape' && !event.defaultPrevented) {
      event.preventDefault();
      stopEditing();
    }
  }

  function cardKeydown(event: KeyboardEvent): void {
    // F2, and `e` of the issue view when the card holds the focus (step 2.2 §7) — handled here
    // even while editing: neither key ever edits the issue around the comment. A field of the
    // card (the editor, its lists) keeps its letters.
    const editKey =
      event.key === 'F2' ||
      (!event.metaKey &&
        !event.ctrlKey &&
        !event.altKey &&
        !event.shiftKey &&
        event.code === 'KeyE');
    if (!editKey || event.defaultPrevented) return;
    if (
      event.target instanceof HTMLElement &&
      event.target.closest('input, textarea, select, [contenteditable="true"]') !== null
    ) {
      return;
    }
    if (!editing && !comment.can.update) return;
    event.preventDefault();
    startEditing();
  }

  // ---- Delete and restore (CNT-005) ----

  async function remove(): Promise<void> {
    busy = true;
    try {
      await deleteComment(comment.id);
      toasts.show({ tone: 'success', message: m.commentRemoved(), undo: () => restore() });
      card?.focus();
    } catch (error) {
      showError(error);
    } finally {
      busy = false;
    }
  }

  async function restore(): Promise<void> {
    busy = true;
    try {
      await restoreComment(comment.id);
      announce(m.commentRestored());
      card?.focus();
    } catch (error) {
      showError(error);
    } finally {
      busy = false;
    }
  }

  const items = $derived.by((): MenuEntry[] => {
    const entries: MenuEntry[] = [];
    if (comment.can.update) {
      entries.push({ type: 'action', label: m.edit(), icon: Pencil, onSelect: startEditing });
    }
    entries.push({
      type: 'action',
      label: common.copyLink(),
      icon: Link,
      onSelect: () => void copyLink(commentHref(issue, comment.id)),
    });
    if (comment.can.delete) {
      entries.push(
        { type: 'separator' },
        {
          type: 'action',
          label: m.deleteComment(),
          icon: Trash2,
          danger: true,
          onSelect: () => void remove(),
        },
      );
    }
    return entries;
  });

  // ---- Files dropped on the card (CNT-018) ----

  const takesFiles = $derived(comment.can.update && !deleted && !editing);

  function hasFiles(event: DragEvent): boolean {
    return takesFiles && (event.dataTransfer?.types.includes('Files') ?? false);
  }

  function dragover(event: DragEvent): void {
    if (!hasFiles(event)) return;
    event.preventDefault();
    if (event.dataTransfer) event.dataTransfer.dropEffect = 'copy';
    dragging = true;
  }

  function dragleave(event: DragEvent & { currentTarget: HTMLElement }): void {
    const to = event.relatedTarget;
    if (!(to instanceof Node && event.currentTarget.contains(to))) dragging = false;
  }

  async function drop(event: DragEvent): Promise<void> {
    dragging = false;
    const files = event.dataTransfer?.files;
    if (!hasFiles(event) || !files || files.length === 0) return;
    event.preventDefault();
    busy = true;
    try {
      const staged = await Promise.all([...files].map((file) => upload(file)));
      const saved = await addCommentFiles(
        comment.id,
        staged.map((file) => file.id),
      );
      pageFiles?.()?.add(saved.files);
      toasts.show({ tone: 'success', message: m.filesAdded({ count: staged.length }) });
    } catch (error) {
      showError(error);
    } finally {
      busy = false;
    }
  }

  // ---- Long texts and the deep link ----

  $effect(() => {
    const element = text;
    if (!element || !collapse) return;
    const measure = () => {
      long = element.scrollHeight > element.clientHeight + 1;
    };
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(element);
    for (const child of element.children) observer.observe(child);
    return () => {
      observer.disconnect();
    };
  });

  $effect(() => {
    if (!focused || !card) return;
    card.scrollIntoView({ block: 'center' });
    card.focus({ preventScroll: true });
  });
</script>

<!-- F2 edits the comment the focus is in; files may be dropped on it. -->
<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<article
  bind:this={card}
  id="comment-{comment.id}"
  class={['card', { focused, restricted, dragging }]}
  tabindex="-1"
  aria-label={m.commentBy({ name: personName(comment.author) })}
  aria-busy={busy}
  onkeydown={cardKeydown}
  ondragover={dragover}
  ondragleave={dragleave}
  ondrop={drop}
>
  <header class="head">
    <PersonName person={comment.author} />
    <Time value={comment.created_at} kind="relative" />
    {#if comment.updated_at && !deleted}
      <span
        class="edited"
        title={comment.updater
          ? m.commentEditedBy({ name: personName(comment.updater) })
          : undefined}
      >
        {m.commentEdited()}
      </span>
    {/if}
    {#if restricted && !deleted}
      <span class="audience">
        <Lock size={14} aria-hidden="true" />{audienceText(comment.audience)}
      </span>
    {/if}
    {#if !deleted && !editing}
      <span class="menu">
        <Menu label={m.commentActions()} {items} placement={{ side: 'bottom', align: 'end' }}>
          {#snippet trigger(props)}
            <IconButton icon={Ellipsis} size="sm" label={m.commentActions()} {...props} />
          {/snippet}
        </Menu>
      </span>
    {/if}
  </header>

  {#if deleted}
    <div class="deleted">
      <p class="muted">{m.commentDeleted()}</p>
      {#if comment.can.restore}
        <Button size="sm" loading={busy} onclick={() => void restore()}>{m.restoreComment()}</Button
        >
      {/if}
    </div>
  {:else if editing}
    <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
    <form
      class="edit"
      aria-label={m.editComment()}
      onkeydown={editKeydown}
      onsubmit={(event) => {
        event.preventDefault();
        void save();
      }}
    >
      {#if theirs}
        <Notice tone="warning">
          <p>{m.commentConflict()}</p>
          <div class="theirs">
            <MarkdownView html={theirs.html} refs={theirs.refs} headingLevel={4} />
          </div>
        </Notice>
      {/if}
      <MarkdownEditor
        label={m.comment()}
        bind:value={draft}
        issue={issue.id}
        rows={4}
        hint={m.editCommentHint()}
        onsubmit={save}
        {upload}
        bind:uploads={draftUploads}
        attachments={pageFiles?.()?.items ?? []}
      />
      {#if problem}<Notice tone="danger">{problem}</Notice>{/if}
      <CommentVisibility
        {issue}
        author={comment.author}
        bind:value={draftAudience}
        bind:restricted={draftRestricted}
        error={audienceError}
      />
      <div class="buttons">
        <Button type="submit" variant="primary" loading={busy}>
          {theirs ? m.keepMine() : m.save()}
        </Button>
        <Button onclick={stopEditing}>{m.cancel()}</Button>
      </div>
    </form>
  {:else}
    <div
      bind:this={text}
      id="{id}-text"
      class={['text', { cut: collapse && !expanded, faded: collapse && !expanded && long }]}
    >
      <MarkdownView html={comment.html} refs={comment.refs} headingLevel={3} />
    </div>
    {#if collapse && (long || expanded)}
      <button
        type="button"
        class="more"
        aria-expanded={expanded}
        aria-controls="{id}-text"
        onclick={() => (expanded = !expanded)}
      >
        {expanded ? m.showLess() : m.showWhole()}
      </button>
    {/if}
    {#if comment.files.length > 0}<CommentFiles files={comment.files} />{/if}
  {/if}
  {#if dragging}<p class="drop" aria-hidden="true">{m.dropOnComment()}</p>{/if}
  {#if changes.length > 0}
    <ul class="changes" aria-label={m.changesWithComment()}>
      {#each changes as change (change.id)}
        <li><ChangeRecord {change} {issue} bare /></li>
      {/each}
    </ul>
  {/if}
</article>

<style>
  .card {
    position: relative;
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    padding: var(--space-3);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    background: var(--color-surface);
    scroll-margin-block: var(--space-12);
  }

  .card:focus-visible {
    outline: 2px solid var(--color-focus);
    outline-offset: 2px;
  }

  /* Limited to some people (CNT-003): a warm tint says not everyone sees it. */
  .restricted {
    border-color: var(--color-warning);
    background: var(--color-warning-bg);
  }

  .focused {
    border-color: var(--color-focus);
    background: var(--color-info-bg);
  }

  .dragging {
    border-style: dashed;
    border-color: var(--color-focus);
    background: var(--color-selected);
  }

  .head {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-1) var(--space-2);
    color: var(--color-text-muted);
    font-size: var(--text-sm);
  }

  .edited {
    font-style: italic;
  }

  .audience {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
  }

  .menu {
    margin-inline-start: auto;
  }

  .deleted {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-2);
  }

  .muted {
    color: var(--color-text-muted);
    font-style: italic;
  }

  .edit {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }

  .theirs {
    margin-top: var(--space-2);
  }

  .buttons {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
  }

  .text {
    min-width: 0;
  }

  .cut {
    max-height: 20rem;
    overflow: hidden;
  }

  .faded {
    mask-image: linear-gradient(to bottom, #000 calc(100% - 4rem), transparent);
  }

  .more {
    align-self: flex-start;
    min-height: 2rem;
    padding: 0 var(--space-1);
    border: none;
    background: none;
    color: var(--color-link);
    font: inherit;
    cursor: pointer;
  }

  .drop {
    color: var(--color-link);
    font-size: var(--text-sm);
  }

  .changes {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    margin: 0;
    padding: var(--space-2) 0 0;
    border-top: 1px solid var(--color-border);
    list-style: none;
    font-size: var(--text-sm);
  }

  @media (pointer: coarse) {
    .more {
      min-height: 2.75rem;
    }
  }
</style>
