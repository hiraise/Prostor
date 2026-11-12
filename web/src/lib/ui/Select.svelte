<script lang="ts" module>
  export interface SelectOption {
    value: string;
    label: string;
    /** Language of the label, when it differs from the page (language names). */
    lang?: string;
  }
</script>

<script lang="ts">
  // A labelled native select: the platform's own picker, which on phones is a large touch-friendly
  // list. Hint and error are tied to it like in TextField (UI-032).
  import ChevronDown from '@lucide/svelte/icons/chevron-down';
  import CircleAlert from '@lucide/svelte/icons/circle-alert';
  import type { HTMLSelectAttributes } from 'svelte/elements';

  interface Props extends Omit<HTMLSelectAttributes, 'value' | 'children'> {
    label: string;
    options: readonly SelectOption[];
    value?: string;
    hint?: string;
    error?: string | null;
  }

  let {
    label,
    options,
    value = $bindable(''),
    hint,
    error,
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
  <div class="control">
    <select
      {id}
      {required}
      bind:value
      aria-invalid={error ? 'true' : undefined}
      aria-describedby={describedBy}
      {...rest}
    >
      {#each options as option (option.value)}
        <option value={option.value} lang={option.lang}>{option.label}</option>
      {/each}
    </select>
    <ChevronDown class="chevron" aria-hidden="true" />
  </div>
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

  .control {
    position: relative;
    display: flex;
  }

  select {
    flex: 1;
    min-width: 0;
    height: var(--control-md);
    padding: 0 calc(var(--space-2) + var(--icon-md) + var(--space-1)) 0 var(--space-2);
    border: 1px solid var(--color-border-control);
    border-radius: var(--radius-md);
    background: var(--color-surface);
    color: var(--color-text);
    font: inherit;
    /* 16 px on touch screens keeps mobile Safari from zooming into the field. */
    font-size: max(var(--text-md), 1rem);
    appearance: none;
    cursor: pointer;
  }

  @media (pointer: fine) {
    select {
      font-size: var(--text-md);
    }
  }

  select:hover {
    border-color: var(--color-text-muted);
  }

  select:focus-visible {
    outline-offset: 0;
    border-color: var(--color-focus);
  }

  select[aria-invalid='true'] {
    border-color: var(--color-danger);
  }

  select:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }

  .control :global(.chevron) {
    position: absolute;
    inset-block: 0;
    inset-inline-end: var(--space-2);
    width: var(--icon-md);
    height: 100%;
    color: var(--color-text-muted);
    pointer-events: none;
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
