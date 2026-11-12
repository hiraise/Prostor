<script lang="ts">
  // The files of a comment (CNT-018): pictures as thumbnails, other files as named links. On the
  // issue page they follow the attachments section: renamed and deleted files show so here, and
  // pictures and videos open in its lightbox (CNT-022).
  import FileIcon from '@lucide/svelte/icons/file';

  import type { Attachment } from '#lib/api/schema.gen.ts';
  import { store } from '#lib/store/index.ts';

  import { issueFiles, opensIn } from '../files/files.svelte.ts';
  import * as m from '../messages.ts';

  let { files }: { files: Attachment[] } = $props();

  const pageFiles = issueFiles();
  const shelf = $derived(pageFiles?.() ?? null);
  const shown = $derived(
    files
      .filter((file) => !shelf?.isRemoved(file.id))
      .map((file) => store.entities.table('attachment').get(file.id) ?? file),
  );

  function open(event: MouseEvent, file: Attachment): void {
    if (!shelf || opensIn(file) !== 'lightbox') return;
    event.preventDefault();
    shelf.previewing = file.id;
  }
</script>

{#if shown.length > 0}
  <ul class="files" aria-label={m.commentFiles()}>
    {#each shown as file (file.id)}
      <li>
        <!-- Server file addresses, not app routes. -->
        <!-- eslint-disable svelte/no-navigation-without-resolve -->
        <a
          href={file.url}
          target="_blank"
          rel="noopener"
          class="file"
          title={file.name}
          onclick={(event) => {
            open(event, file);
          }}
        >
          {#if file.thumbnail_url}
            <img src={file.thumbnail_url} alt={file.name} loading="lazy" />
          {:else}
            <FileIcon aria-hidden="true" size={16} />
            <span class="name">{file.name}</span>
          {/if}
        </a>
        <!-- eslint-enable svelte/no-navigation-without-resolve -->
      </li>
    {/each}
  </ul>
{/if}

<style>
  .files {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .file {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
    max-width: 16rem;
    min-height: var(--control-md);
    padding: var(--space-1) var(--space-2);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
    background: var(--color-surface-sunken);
    color: var(--color-link);
    font-size: var(--text-sm);
  }

  .file:has(img) {
    padding: 0;
    overflow: hidden;
  }

  img {
    display: block;
    width: 6rem;
    height: 4.5rem;
    object-fit: cover;
  }

  .name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
