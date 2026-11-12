<script lang="ts">
  // The files of the issue under its description (CNT-017, CNT-020): thumbnails of pictures, kind
  // icons with names for other files, its comments' files included. Pictures and videos open in the
  // lightbox, PDFs in a new tab, other files download; each file has its menu. Restricted files
  // show a lock with whom they are shown to. The section folds (a personal setting, UI-018); files
  // on their way show their progress, failed ones the reason.
  import ChevronDown from '@lucide/svelte/icons/chevron-down';
  import ChevronRight from '@lucide/svelte/icons/chevron-right';
  import FileIcon from '@lucide/svelte/icons/file';
  import FileText from '@lucide/svelte/icons/file-text';
  import FileVideo from '@lucide/svelte/icons/file-video';
  import Lock from '@lucide/svelte/icons/lock';
  import Paperclip from '@lucide/svelte/icons/paperclip';
  import X from '@lucide/svelte/icons/x';

  import type { Attachment, Issue } from '#lib/api/schema.gen.ts';
  import { preferences } from '#lib/session/index.ts';
  import Button from '#lib/ui/Button.svelte';
  import FileButton from '#lib/ui/FileButton.svelte';
  import IconButton from '#lib/ui/IconButton.svelte';
  import { showError } from '#lib/ui/toast.svelte.ts';
  import { tooltip } from '#lib/ui/tooltip.ts';

  import { audienceText, isRestricted } from '../audience/audience.ts';
  import * as m from '../messages.ts';
  import AttachmentDialogs from './AttachmentDialogs.svelte';
  import AttachmentMenu from './AttachmentMenu.svelte';
  import { opensIn, type FileAction, type IssueFiles } from './files.svelte.ts';

  interface Props {
    issue: Issue;
    files: IssueFiles;
    /** The viewer may attach files. */
    canAttach: boolean;
    /** Attaches picked files; the page reports the outcome. */
    onattach: (files: File[]) => unknown;
  }

  let { issue, files, canAttach, onattach }: Props = $props();

  const id = $props.id();
  let action = $state<FileAction | null>(null);

  const items = $derived(files.items);
  const collapsed = $derived(preferences.value('attachments_collapsed'));
  const shown = $derived(items.length > 0 || files.pending.length > 0 || canAttach);

  function toggle(): void {
    preferences.set({ attachments_collapsed: collapsed ? null : true });
  }

  function kindIcon(file: Attachment) {
    if (file.kind === 'video') return FileVideo;
    if (file.kind === 'pdf') return FileText;
    return FileIcon;
  }

  function percent(loaded: number, total: number): number {
    return total > 0 ? Math.min(100, Math.round((loaded / total) * 100)) : 0;
  }
</script>

{#if shown}
  <section class="files" aria-labelledby="{id}-title">
    <div class="head">
      <h2 id="{id}-title">
        <button
          type="button"
          class="toggle"
          aria-expanded={!collapsed}
          aria-controls="{id}-list"
          onclick={toggle}
        >
          {#if collapsed}
            <ChevronRight aria-hidden="true" size={16} />
          {:else}
            <ChevronDown aria-hidden="true" size={16} />
          {/if}
          {m.attachments()}
          {#if items.length > 0}<span class="count">{items.length}</span>{/if}
        </button>
      </h2>
      {#if canAttach}
        <FileButton multiple size="sm" variant="ghost" icon={Paperclip} onfiles={onattach}>
          {m.attachFiles()}
        </FileButton>
      {/if}
    </div>
    <div id="{id}-list" class="list" hidden={collapsed}>
      {#if files.pending.length > 0}
        <ul class="pending" aria-label={m.attachingFiles()}>
          {#each files.pending as entry (entry.key)}
            <li class={{ failed: entry.error !== null }}>
              {#if entry.error === null}
                <span class="name">{entry.name}</span>
                <progress
                  max="100"
                  value={percent(entry.loaded, entry.total)}
                  aria-label={m.attaching({ name: entry.name })}
                ></progress>
              {:else}
                <span class="reason" role="alert">
                  {m.attachFailed({ name: entry.name, reason: entry.error })}
                </span>
                <IconButton
                  icon={X}
                  size="sm"
                  label={m.dismiss()}
                  onclick={() => files.dismiss(entry.key)}
                />
              {/if}
            </li>
          {/each}
        </ul>
      {/if}
      {#if items.length > 0}
        <ul class="grid" aria-label={m.attachmentCount({ count: items.length })}>
          {#each items as file (file.id)}
            {@const opens = opensIn(file)}
            {@const Icon = kindIcon(file)}
            {@const who = isRestricted(file.audience) ? audienceText(file.audience) : null}
            <li class="tile">
              {#if opens === 'lightbox'}
                <button
                  type="button"
                  class="open"
                  aria-label={m.openFile({ name: file.name })}
                  onclick={() => (files.previewing = file.id)}
                >
                  {#if file.kind === 'image'}
                    <img src={file.thumbnail_url ?? file.url} alt="" loading="lazy" />
                  {:else}
                    <Icon aria-hidden="true" size={32} />
                  {/if}
                </button>
              {:else}
                <!-- Server file addresses, not app routes. -->
                <!-- eslint-disable svelte/no-navigation-without-resolve -->
                <a
                  class="open"
                  href={file.url}
                  aria-label={m.openFile({ name: file.name })}
                  target={opens === 'tab' ? '_blank' : undefined}
                  rel={opens === 'tab' ? 'noopener' : undefined}
                  download={opens === 'download' ? file.name : undefined}
                >
                  <Icon aria-hidden="true" size={32} />
                </a>
                <!-- eslint-enable svelte/no-navigation-without-resolve -->
              {/if}
              <div class="caption">
                {#if who}
                  <span class="lock" role="img" aria-label={who} {@attach tooltip(who)}>
                    <Lock aria-hidden="true" size={14} />
                  </span>
                {/if}
                <span class="name" title={file.name}>{file.name}</span>
                <AttachmentMenu
                  {file}
                  onaction={(kind: FileAction['kind']) => (action = { kind, file })}
                />
              </div>
            </li>
          {/each}
        </ul>
      {:else if files.pending.length === 0}
        <p class="muted">{m.noAttachments()}</p>
      {/if}
      {#if files.hasMore}
        <Button
          size="sm"
          variant="ghost"
          loading={files.loadingMore}
          onclick={() => files.more().catch((error: unknown) => showError(error))}
          >{m.moreFiles()}</Button
        >
      {/if}
    </div>
  </section>
{/if}

<AttachmentDialogs {issue} {files} bind:action />

<style>
  .files {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }

  .head {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2);
  }

  h2 {
    font-size: var(--text-lg);
  }

  .toggle {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
    min-height: var(--control-md);
    padding: 0;
    border: 0;
    background: none;
    color: inherit;
    font: inherit;
    cursor: pointer;
  }

  .count {
    color: var(--color-text-muted);
    font-size: var(--text-sm);
    font-weight: normal;
  }

  .list {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: var(--space-2);
  }

  .list[hidden] {
    display: none;
  }

  .pending,
  .grid {
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .pending {
    display: flex;
    flex-direction: column;
    align-self: stretch;
    gap: var(--space-1);
  }

  .pending li {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    min-height: var(--control-md);
    font-size: var(--text-sm);
  }

  .pending progress {
    flex: 1 1 auto;
    max-width: 16rem;
  }

  .failed .reason {
    color: var(--color-danger);
  }

  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(9rem, 1fr));
    align-self: stretch;
    gap: var(--space-2);
  }

  .tile {
    display: flex;
    flex-direction: column;
    min-width: 0;
    overflow: hidden;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
    background: var(--color-surface-sunken);
  }

  .open {
    display: flex;
    justify-content: center;
    align-items: center;
    height: 6rem;
    padding: 0;
    border: 0;
    background: none;
    color: var(--color-text-muted);
    cursor: pointer;
  }

  .open img {
    display: block;
    width: 100%;
    height: 100%;
    object-fit: cover;
  }

  .caption {
    display: flex;
    align-items: center;
    gap: var(--space-1);
    min-height: var(--control-md);
    padding-inline: var(--space-2) 0;
    border-block-start: 1px solid var(--color-border);
    font-size: var(--text-sm);
  }

  .lock {
    display: inline-flex;
    flex: none;
    color: var(--color-text-muted);
  }

  .name {
    flex: 1 1 auto;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .muted {
    color: var(--color-text-muted);
  }
</style>
