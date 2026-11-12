<script lang="ts">
  // A native checkbox with its label; the whole row is the click and touch target.
  import type { HTMLInputAttributes } from 'svelte/elements';

  interface Props extends Omit<HTMLInputAttributes, 'type' | 'checked' | 'children'> {
    label: string;
    checked?: boolean;
    hint?: string;
  }

  let { label, checked = $bindable(false), hint, class: className, ...rest }: Props = $props();
  const id = $props.id();
</script>

<div class={['checkbox', className]}>
  <input
    {id}
    type="checkbox"
    bind:checked
    aria-describedby={hint ? `${id}-hint` : undefined}
    {...rest}
  />
  <label for={id}>{label}</label>
  {#if hint}<p id="{id}-hint" class="hint">{hint}</p>{/if}
</div>

<style>
  .checkbox {
    display: grid;
    grid-template-columns: auto 1fr;
    align-items: center;
    column-gap: var(--space-2);
    min-height: var(--control-sm);
  }

  input {
    width: 1rem;
    height: 1rem;
    margin: 0;
    accent-color: var(--color-ink);
  }

  @media (pointer: coarse) {
    input {
      width: 1.25rem;
      height: 1.25rem;
    }
  }

  label {
    cursor: pointer;
    /* The label stretches to the full control height, so the touch target is large. */
    display: flex;
    align-items: center;
    min-height: var(--control-sm);
  }

  .hint {
    grid-column: 2;
    color: var(--color-text-muted);
    font-size: var(--text-sm);
  }
</style>
