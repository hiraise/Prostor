<script lang="ts" module>
  export interface RadioOption {
    value: string;
    label: string;
    hint?: string;
  }
</script>

<script lang="ts">
  // One choice from a few, all visible at once: a fieldset of native radio buttons. Arrow keys move
  // between them (the browser's own behaviour); each whole row is the click and touch target.
  let {
    legend,
    hint,
    options,
    value = $bindable(''),
    name,
  }: {
    legend: string;
    /** What the choice is about, read with the group. */
    hint?: string;
    options: readonly RadioOption[];
    value?: string;
    /** Form name of the group; unique per page. */
    name?: string;
  } = $props();

  const id = $props.id();
  const group = $derived(name ?? id);
</script>

<fieldset class="group" aria-describedby={hint ? `${id}-hint` : undefined}>
  <legend>{legend}</legend>
  {#if hint}<p id="{id}-hint" class="group-hint">{hint}</p>{/if}
  {#each options as option (option.value)}
    <div class="option">
      <input
        id="{id}-{option.value}"
        type="radio"
        name={group}
        value={option.value}
        bind:group={value}
        aria-describedby={option.hint ? `${id}-${option.value}-hint` : undefined}
      />
      <label for="{id}-{option.value}">{option.label}</label>
      {#if option.hint}
        <p id="{id}-{option.value}-hint" class="hint">{option.hint}</p>
      {/if}
    </div>
  {/each}
</fieldset>

<style>
  .group {
    display: flex;
    flex-direction: column;
    gap: var(--space-0_5);
    min-width: 0;
    margin: 0;
    padding: 0;
    border: none;
  }

  legend {
    margin-bottom: var(--space-1);
    padding: 0;
    font-weight: var(--weight-medium);
  }

  .group-hint {
    margin: calc(-1 * var(--space-1)) 0 var(--space-1);
    color: var(--color-text-muted);
    font-size: var(--text-sm);
  }

  .option {
    display: grid;
    grid-template-columns: auto 1fr;
    align-items: center;
    column-gap: var(--space-2);
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
    display: flex;
    align-items: center;
    min-height: var(--control-sm);
    cursor: pointer;
  }

  .hint {
    grid-column: 2;
    margin-top: calc(-1 * var(--space-1));
    color: var(--color-text-muted);
    font-size: var(--text-sm);
  }
</style>
