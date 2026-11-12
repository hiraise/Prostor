<script lang="ts">
  // Preview of a picture or a video of the issue (CNT-022): a large dialog with the file, «Скачать»,
  // and previous/next among the issue's pictures and videos (buttons, ← and →). PDFs open in a new
  // tab and other files download, so they never come here (`opensIn`).
  import ChevronLeft from '@lucide/svelte/icons/chevron-left';
  import ChevronRight from '@lucide/svelte/icons/chevron-right';
  import Download from '@lucide/svelte/icons/download';

  import Button from '#lib/ui/Button.svelte';
  import Dialog from '#lib/ui/Dialog.svelte';

  import * as m from '../messages.ts';
  import { opensIn, type IssueFiles } from './files.svelte.ts';

  let { files }: { files: IssueFiles } = $props();

  const shown = $derived(files.items.filter((file) => opensIn(file) === 'lightbox'));
  const index = $derived(shown.findIndex((file) => file.id === files.previewing));
  const file = $derived(index >= 0 ? shown[index] : undefined);

  function step(by: number): void {
    if (shown.length < 2) return;
    const next = shown[(index + by + shown.length) % shown.length];
    if (next) files.previewing = next.id;
  }

  function keydown(event: KeyboardEvent): void {
    if (!file || event.defaultPrevented) return;
    // Keys inside the player (seeking) stay with it.
    if (event.target instanceof HTMLVideoElement) return;
    if (event.key === 'ArrowLeft') step(-1);
    else if (event.key === 'ArrowRight') step(1);
    else return;
    event.preventDefault();
  }
</script>

<svelte:window onkeydown={keydown} />

<Dialog
  bind:open={() => file !== undefined, (open) => !open && (files.previewing = null)}
  title={file?.name ?? ''}
  size="lg"
>
  <div class="stage">
    {#if file?.kind === 'image'}
      <img src={file.url} alt={file.name} width={file.width} height={file.height} />
    {:else if file?.kind === 'video'}
      {#key file.id}
        <!-- svelte-ignore a11y_media_has_caption -->
        <video src={file.url} controls preload="metadata" aria-label={file.name}></video>
      {/key}
    {/if}
  </div>
  {#snippet footer()}
    {#if shown.length > 1}
      <Button icon={ChevronLeft} onclick={() => step(-1)} aria-keyshortcuts="ArrowLeft"
        >{m.previousFile()}</Button
      >
      <span class="count">{m.fileOf({ index: index + 1, count: shown.length })}</span>
      <Button icon={ChevronRight} onclick={() => step(1)} aria-keyshortcuts="ArrowRight"
        >{m.nextFile()}</Button
      >
    {/if}
    {#if file}
      <Button icon={Download} href={file.url} download={file.name}>{m.download()}</Button>
    {/if}
  {/snippet}
</Dialog>

<style>
  .stage {
    display: flex;
    justify-content: center;
    align-items: center;
    min-height: 12rem;
  }

  img,
  video {
    display: block;
    max-width: 100%;
    height: auto;
    max-height: 70dvh;
    object-fit: contain;
  }

  .count {
    margin-inline-end: auto;
    color: var(--color-text-muted);
    font-size: var(--text-sm);
  }
</style>
