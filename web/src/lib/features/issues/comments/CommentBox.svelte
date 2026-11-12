<script lang="ts">
  // The box for a new comment (CNT-001, CNT-003, CNT-018, CNT-054): the Markdown editor with the
  // issue's people to mention and files, the visibility inside the issue's audience, and «Отправить»
  // or Ctrl/⌘+Enter. Files alone are a comment too. The posted comment goes to `onposted`.
  import type { Audience, Comment, Issue, Upload, UserRef } from '#lib/api/schema.gen.ts';
  import { uploadFile } from '#lib/api/upload.ts';
  import { MarkdownEditor } from '#lib/features/content/index.ts';
  import { refusalText } from '#lib/features/fields/index.ts';
  import Button from '#lib/ui/Button.svelte';
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
    /** Sends a file; the app's upload by default (tests give their own). */
    upload?: (file: File) => Promise<Upload>;
  }

  let { issue, author, onposted, upload = (file: File) => uploadFile(file) }: Props = $props();

  const pageFiles = issueFiles();
  const attachments = $derived(pageFiles?.()?.items ?? []);

  let text = $state('');
  let uploads = $state<number[]>([]);
  let audience = $state<Audience>({ users: [], groups: [] });
  let restricted = $state(false);
  let busy = $state(false);
  let problem = $state<string | null>(null);
  let audienceError = $state<string | null>(null);
  /** Remounts the editor after a post, so its upload list and errors start empty. */
  let generation = $state(0);

  const empty = $derived(text.trim() === '' && uploads.length === 0);

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
      text = '';
      uploads = [];
      generation += 1;
      pageFiles?.()?.add(posted.files);
      toasts.show({ tone: 'success', message: m.commentPosted() });
      await onposted?.(posted);
    } catch (error) {
      problem = refusalText(error);
    } finally {
      busy = false;
    }
  }
</script>

<form
  class="box"
  aria-label={m.newComment()}
  onsubmit={(event) => {
    event.preventDefault();
    void send();
  }}
>
  {#key generation}
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
  {/key}
  {#if problem}<Notice tone="danger">{problem}</Notice>{/if}
  <div class="actions">
    <CommentVisibility
      {issue}
      {author}
      bind:value={audience}
      bind:restricted
      error={audienceError}
    />
    <Button type="submit" variant="primary" loading={busy}>{m.send()}</Button>
  </div>
</form>

<style>
  .box {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }

  .actions {
    display: flex;
    flex-wrap: wrap;
    align-items: flex-start;
    justify-content: space-between;
    gap: var(--space-2);
  }
</style>
