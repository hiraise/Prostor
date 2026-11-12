<script lang="ts">
  // A labelled multi-line text field that looks like `TextField`: the hint and the error are tied
  // to it (aria-describedby), and the error sets aria-invalid (UI-032).
  import CircleAlert from '@lucide/svelte/icons/circle-alert';
  import type { HTMLTextareaAttributes } from 'svelte/elements';

  interface Props extends Omit<HTMLTextareaAttributes, 'value' | 'children'> {
    label: string;
    value?: string;
    hint?: string;
    error?: string | null;
  }

  let {
    label,
    value = $bindable(''),
    hint,
    error,
    required,
    rows = 4,
    class: className,
    ...rest
  }: Props = $props();

  const id = $props.id();
  const describedBy = $derived(
    [hint ? `${id}-hint` : null, error ? `${id}-error` : null].filter(Boolean).join(' ') ||
      undefined,
  );
</script>

<div class={['field', className]}>
  <label for={id}>
    {label}{#if required}<span class="required" aria-hidden="true"> *</span>{/if}
  </label>
  <textarea
    {id}
    {rows}
    {required}
    bind:value
    aria-invalid={error ? 'true' : undefined}
    aria-describedby={describedBy}
    {...rest}></textarea>
  {#if hint}<p id="{id}-hint" class="hint">{hint}</p>{/if}
  {#if error}
    <p id="{id}-error" class="error"><CircleAlert aria-hidden="true" />{error}</p>
  {/if}
</div>

<style>
  .field {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    min-width: 0;
  }

  label {
    font-weight: var(--weight-medium);
  }

  .required {
    margin-inline-start: 0.2em;
    color: var(--color-danger);
  }

  textarea {
    min-height: calc(var(--control-md) * 2);
    padding: var(--space-1_5) var(--space-2);
    border: 1px solid var(--color-border-control);
    border-radius: var(--radius-md);
    background: var(--color-surface);
    color: var(--color-text);
    font: inherit;
    /* 16 px on touch screens keeps mobile Safari from zooming into the field. */
    font-size: max(var(--text-md), 1rem);
    line-height: var(--leading-normal);
    resize: vertical;
  }

  @media (pointer: fine) {
    textarea {
      font-size: var(--text-md);
    }
  }

  textarea::placeholder {
    color: var(--color-text-faint);
  }

  textarea:hover {
    border-color: var(--color-text-muted);
  }

  textarea:focus-visible {
    outline-offset: 0;
    border-color: var(--color-focus);
  }

  textarea[aria-invalid='true'] {
    border-color: var(--color-danger);
  }

  textarea[readonly] {
    background: var(--color-surface-sunken);
  }

  textarea:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }

  .hint {
    color: var(--color-text-muted);
    font-size: var(--text-sm);
  }

  .error {
    display: flex;
    align-items: flex-start;
    gap: var(--space-1);
    color: var(--color-danger);
    font-size: var(--text-sm);
    overflow-wrap: anywhere;
  }

  .error :global(svg) {
    flex: none;
    width: var(--icon-sm);
    height: var(--icon-sm);
    margin-top: 0.15em;
  }
</style>
