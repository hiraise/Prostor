<script lang="ts">
  // A labelled text input. The hint and the error are tied to the input (aria-describedby), so a
  // screen reader reads them with the field; the error also sets aria-invalid (UI-032).
  import CircleAlert from '@lucide/svelte/icons/circle-alert';
  import type { HTMLInputAttributes } from 'svelte/elements';

  interface Props extends Omit<HTMLInputAttributes, 'value' | 'children'> {
    label: string;
    value?: string;
    hint?: string;
    error?: string | null;
    /** The input element, for moving focus to it. */
    input?: HTMLInputElement;
  }

  let {
    label,
    value = $bindable(''),
    input = $bindable(),
    hint,
    error,
    type = 'text',
    required,
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
  <input
    {id}
    {type}
    {required}
    bind:value
    bind:this={input}
    aria-invalid={error ? 'true' : undefined}
    aria-describedby={describedBy}
    {...rest}
  />
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

  input {
    height: var(--control-md);
    padding: 0 var(--space-2);
    border: 1px solid var(--color-border-control);
    border-radius: var(--radius-md);
    background: var(--color-surface);
    color: var(--color-text);
    /* 16 px on touch screens keeps mobile Safari from zooming into the field. */
    font-size: max(var(--text-md), 1rem);
  }

  @media (pointer: fine) {
    input {
      font-size: var(--text-md);
    }
  }

  input::placeholder {
    color: var(--color-text-faint);
  }

  input:hover {
    border-color: var(--color-text-muted);
  }

  input:focus-visible {
    outline-width: 1px;
    outline-offset: 1px;
    border-color: var(--color-focus);
  }

  input[aria-invalid='true'] {
    border-color: var(--color-danger);
  }

  input:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }

  .hint {
    color: var(--color-text-muted);
    font-size: var(--text-sm);
  }

  .error {
    display: flex;
    align-items: center;
    gap: var(--space-1);
    color: var(--color-danger);
    font-size: var(--text-sm);
  }

  .error :global(svg) {
    width: var(--icon-sm);
    height: var(--icon-sm);
  }
</style>
