<script lang="ts">
  // Choosing a value's color scheme (FLD-044): a radio group of named swatches and "No color".
  // Each swatch shows its name in its own colors, so the choice never depends on color alone.
  import * as m from './messages.ts';
  import ValueChip from './ValueChip.svelte';

  let { value = $bindable(null) }: { value?: number | null } = $props();

  const name = $props.id();
  const options = $derived([
    { key: 'none', scheme: null, label: m.noColor() },
    ...m.colorNames.map((label, scheme) => ({ key: String(scheme), scheme, label: label() })),
  ]);
</script>

<fieldset class="picker">
  <legend>{m.color()}</legend>
  <div class="options">
    {#each options as option (option.key)}
      <label class="option">
        <input
          type="radio"
          {name}
          value={option.key}
          checked={value === option.scheme}
          onchange={() => {
            value = option.scheme;
          }}
        />
        <ValueChip name={option.label} color={option.scheme} />
      </label>
    {/each}
  </div>
</fieldset>

<style>
  .picker {
    min-width: 0;
    margin: 0;
    padding: 0;
    border: none;
  }

  legend {
    margin-bottom: var(--space-2);
    padding: 0;
    font-weight: var(--weight-medium);
  }

  .options {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(8.5rem, 1fr));
    gap: var(--space-1) var(--space-2);
  }

  .option {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    min-height: var(--control-md);
    padding: 0 var(--space-1);
    border-radius: var(--radius-md);
    cursor: pointer;
  }

  .option:hover {
    background: var(--color-hover);
  }

  input {
    flex: none;
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
</style>
