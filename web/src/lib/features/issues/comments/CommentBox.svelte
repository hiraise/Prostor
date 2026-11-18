<script lang="ts">
  // The box for a new comment (CNT-001, CNT-003, CNT-018, CNT-054). At rest it is one quiet line,
  // «Написать комментарий…»; a click opens the Markdown editor with the issue's people to mention
  // and files, the visibility inside the issue's audience, and «Отправить» or Ctrl/⌘+Enter — the
  // button appears once writing has begun (text or files). «Отмена» folds the box back, asking
  // first when a draft would be thrown away; a sent comment folds it too. Files alone are a
  // comment too. The posted comment goes to `onposted`.
  import { tick } from 'svelte';

  import type { Audience, Comment, Issue, Upload, UserRef } from '#lib/api/schema.gen.ts';
  import { uploadFile } from '#lib/api/upload.ts';
  import { MarkdownEditor } from '#lib/features/content/index.ts';
  import { refusalText } from '#lib/features/fields/index.ts';
  import * as common from '#lib/i18n/messages/common.ts';
  import Button from '#lib/ui/Button.svelte';
  import Dialog from '#lib/ui/Dialog.svelte';
  import Notice from '#lib/ui/Notice.svelte';
  import { toasts } from '#lib/ui/toast.svelte.ts';

  import * as m from '../messages.ts';
  import { issueFiles } from '../files/files.svelte.ts';
  import { postComment } from './comments.ts';
  import CommentVisibility from './CommentVisibility.svelte';

  interface Props {
    issue: Issue;
    author: UserRef;
    onposted?: (comment: Comment) => unknown;
    /** Each rise opens the box and puts the caret in the text — `r` of the issue view (§7). */
    openRequest?: number;
    /** Sends a file; the app's upload by default (tests give their own). */
    upload?: (file: File) => Promise<Upload>;
  }

  let {
    issue,
    author,
    onposted,
    openRequest = 0,
    upload = (file: File) => uploadFile(file),
  }: Props = $props();

  const pageFiles = issueFiles();
  const attachments = $derived(pageFiles?.()?.items ?? []);

  /** The box starts folded and folds back after a sent comment. */
  let active = $state(false);
  let text = $state('');
  let uploads = $state<number[]>([]);
  let audience = $state<Audience>({ users: [], groups: [] });
  let restricted = $state(false);
  let busy = $state(false);
  let problem = $state<string | null>(null);
  let audienceError = $state<string | null>(null);
  /** Asks before a draft is thrown away by «Отмена». */
  let confirmOpen = $state(false);
  let editor: HTMLElement | undefined = $state();

  const empty = $derived(text.trim() === '' && uploads.length === 0);

  // Opening the box puts the caret where the person asked for: a click, or `r` of the issue
  // view (§7) — each rise of the request opens and focuses, also over an already open box.
  $effect(() => {
    if (active) editor?.querySelector('textarea')?.focus();
  });
  $effect(() => {
    if (openRequest === 0) return;
    active = true;
    void tick().then(() => editor?.querySelector('textarea')?.focus());
  });

  function fold(): void {
    active = false;
    confirmOpen = false;
    text = '';
    uploads = [];
    restricted = false;
    audience = { users: [], groups: [] };
    problem = null;
    audienceError = null;
  }

  function cancel(): void {
    if (empty) {
      fold();
      return;
    }
    confirmOpen = true;
  }

  /** Esc in the empty editor folds the box; an open mention list takes its Esc first. */
  function escape(event: KeyboardEvent): void {
    if (event.key !== 'Escape' || !empty || event.defaultPrevented) return;
    event.preventDefault();
    fold();
  }

  async function send(): Promise<void> {
    if (busy) return;
    if (empty) {
      problem = m.commentEmpty();
      return;
    }
    audienceError =
      restricted && audience.users.length + audience.groups.length === 0 ? m.audienceEmpty() : null;
    if (audienceError) return;
    busy = true;
    problem = null;
    try {
      const posted = await postComment(issue.id, {
        text,
        uploads,
        audience: restricted ? audience : null,
      });
      pageFiles?.()?.add(posted.files);
      toasts.show({ tone: 'success', message: m.commentPosted() });
      fold();
      await onposted?.(posted);
    } catch (error) {
      problem = refusalText(error);
    } finally {
      busy = false;
    }
  }
</script>

{#if active}
  <form
    class="box"
    aria-label={m.newComment()}
    onsubmit={(event) => {
      event.preventDefault();
      void send();
    }}
  >
    <!-- The wrapper only catches Esc aimed at the empty editor. -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="editor" bind:this={editor} onkeydown={escape}>
      <MarkdownEditor
        label={m.comment()}
        bind:value={text}
        issue={issue.id}
        rows={3}
        hint={m.commentHint()}
        onsubmit={send}
        oninput={() => (problem = null)}
        {upload}
        bind:uploads
        {attachments}
      />
    </div>
    {#if problem}<Notice tone="danger">{problem}</Notice>{/if}
    <div class="actions">
      <CommentVisibility
        {issue}
        {author}
        bind:value={audience}
        bind:restricted
        error={audienceError}
      />
      <div class="send">
        <Button variant="ghost" onclick={cancel}>{common.cancel()}</Button>
        {#if !empty}
          <Button type="submit" variant="primary" loading={busy}>{m.send()}</Button>
        {/if}
      </div>
    </div>
  </form>
{:else}
  <button type="button" class="start" onclick={() => (active = true)}>
    {m.writeComment()}
  </button>
{/if}

<Dialog bind:open={confirmOpen} title={m.discardCommentTitle()}>
  <p class="draft">{m.discardCommentText()}</p>
  {#snippet footer()}
    <Button variant="ghost" onclick={() => (confirmOpen = false)}>{m.keepComment()}</Button>
    <Button variant="danger" onclick={fold}>{m.discardComment()}</Button>
  {/snippet}
</Dialog>

<style>
  .box {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }

  /* The resting line: quiet, one row, full width of the zone. */
  .start {
    display: flex;
    align-items: center;
    width: 100%;
    min-height: var(--control-lg);
    padding: 0 var(--space-3);
    border: 1px solid var(--color-border-control);
    border-radius: var(--radius-md);
    background: transparent;
    color: var(--color-text-muted);
    font: inherit;
    text-align: start;
    cursor: text;
  }

  .start:hover {
    background: var(--color-hover);
    color: var(--color-text-muted);
  }

  .actions {
    display: flex;
    flex-wrap: wrap;
    align-items: flex-start;
    justify-content: space-between;
    gap: var(--space-2);
  }

  .send {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }

  .draft {
    margin: 0;
    color: var(--color-text-muted);
  }
</style>
