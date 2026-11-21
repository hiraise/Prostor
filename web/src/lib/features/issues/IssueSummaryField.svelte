<script lang="ts">
  import TextField from '#lib/ui/TextField.svelte';

  interface Props {
    label: string;
    value?: string;
    input?: HTMLInputElement;
    error?: string | null;
    placeholder?: string;
    oninput?: () => void;
  }

  let {
    label,
    value = $bindable(''),
    input = $bindable(),
    error,
    placeholder,
    oninput,
  }: Props = $props();
</script>

<div class="summary">
  <TextField
    {label}
    bind:value
    bind:input
    {error}
    required
    maxlength={255}
    {placeholder}
    {oninput}
  />
</div>

<style>
  .summary {
    min-width: 0;
  }

  .summary :global(label) {
    position: absolute;
    width: 1px;
    height: 1px;
    padding: 0;
    margin: -1px;
    overflow: hidden;
    clip-path: inset(50%);
    white-space: nowrap;
    border: 0;
  }

  .summary :global(input) {
    height: 3rem;
    padding: 0 0 var(--space-2);
    border: 0;
    border-block-end: 1px solid var(--color-border-control);
    border-radius: 0;
    background: transparent;
    font-size: var(--text-2xl);
    font-weight: var(--weight-semibold);
  }

  .summary :global(input:focus-visible) {
    outline: none;
    border-block-end-color: var(--color-focus);
  }

  .summary :global(input[aria-invalid='true']) {
    border-block-end-color: var(--color-danger);
  }
</style>
