<script lang="ts">
  // Files dropped anywhere on the issue page or pasted outside a text field are attached (CNT-017).
  // While files are dragged over the page an overlay says so. Places that take files themselves
  // (editors, a comment) prevent the event, and the page leaves those drops to them.
  import Paperclip from '@lucide/svelte/icons/paperclip';

  import * as m from '../messages.ts';

  interface Props {
    onfiles: (files: File[]) => unknown;
  }

  let { onfiles }: Props = $props();

  /** `dragover` repeats while dragging; this long without one means the files left the page. */
  const LEFT_MS = 150;
  let over = $state(false);
  let timer: ReturnType<typeof setTimeout> | undefined;

  function carriesFiles(event: DragEvent): boolean {
    return event.dataTransfer?.types.includes('Files') ?? false;
  }

  function dragover(event: DragEvent): void {
    if (!carriesFiles(event)) return;
    clearTimeout(timer);
    timer = setTimeout(() => (over = false), LEFT_MS);
    if (event.defaultPrevented) {
      over = false;
      return;
    }
    event.preventDefault();
    if (event.dataTransfer) event.dataTransfer.dropEffect = 'copy';
    over = true;
  }

  function drop(event: DragEvent): void {
    clearTimeout(timer);
    over = false;
    if (event.defaultPrevented || !carriesFiles(event)) return;
    event.preventDefault();
    const files = [...(event.dataTransfer?.files ?? [])];
    if (files.length > 0) onfiles(files);
  }

  function editable(target: EventTarget | null): boolean {
    return (
      target instanceof HTMLElement &&
      (target.isContentEditable || target.closest('input, textarea, select') !== null)
    );
  }

  function paste(event: ClipboardEvent): void {
    if (event.defaultPrevented || editable(event.target)) return;
    const files = [...(event.clipboardData?.files ?? [])];
    if (files.length === 0) return;
    event.preventDefault();
    onfiles(files);
  }

  $effect(() => () => {
    clearTimeout(timer);
  });
</script>

<svelte:window ondragover={dragover} ondrop={drop} onpaste={paste} />

{#if over}
  <div class="overlay" aria-hidden="true">
    <p><Paperclip size={24} />{m.dropToAttach()}</p>
  </div>
{/if}

<style>
  .overlay {
    display: flex;
    position: fixed;
    z-index: var(--z-sticky);
    justify-content: center;
    align-items: center;
    inset: 0;
    border: 2px dashed var(--color-focus);
    background: color-mix(in srgb, var(--color-surface) 80%, transparent);
    pointer-events: none;
  }

  p {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-3) var(--space-4);
    border-radius: var(--radius-md);
    background: var(--color-surface-raised);
    box-shadow: var(--shadow-md);
    font-size: var(--text-lg);
  }
</style>
