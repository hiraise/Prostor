<script lang="ts">
  // A button that lets the person choose files (docs/design.md §7): it opens the platform's file
  // picker (on phones also the camera and the photo library) through a hidden file input and
  // reports the chosen files. Choosing the same file again reports it again.
  import type { LucideIcon } from '@lucide/svelte';
  import type { Snippet } from 'svelte';

  import Button from './Button.svelte';

  interface Props {
    /** File types the picker offers, as the `accept` attribute (`image/*`). */
    accept?: string;
    multiple?: boolean;
    onfiles: (files: File[]) => void;
    variant?: 'primary' | 'secondary' | 'ghost' | 'danger';
    size?: 'sm' | 'md' | 'lg';
    icon?: LucideIcon;
    loading?: boolean;
    disabled?: boolean;
    children?: Snippet;
  }

  let {
    accept,
    multiple = false,
    onfiles,
    variant = 'secondary',
    size = 'md',
    icon,
    loading = false,
    disabled = false,
    children,
  }: Props = $props();

  let input: HTMLInputElement | undefined = $state();

  function choose(): void {
    input?.click();
  }

  function chosen(): void {
    if (!input) return;
    const files = [...(input.files ?? [])];
    input.value = '';
    if (files.length > 0) onfiles(files);
  }
</script>

<Button {variant} {size} {icon} {loading} {disabled} onclick={choose}>
  {@render children?.()}
</Button>
<input
  bind:this={input}
  type="file"
  {accept}
  {multiple}
  hidden
  tabindex="-1"
  aria-hidden="true"
  onchange={chosen}
/>
