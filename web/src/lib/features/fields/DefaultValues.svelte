<script lang="ts">
  // Choosing default values from a set (FLD-019, FLD-055): one as radio buttons with "No default
  // value", several as checkboxes. Archived values are not offered (FLD-045).
  import type { SetValue } from './fields.ts';
  import * as m from './messages.ts';
  import ValueChip from './ValueChip.svelte';

  interface Props {
    values: readonly SetValue[];
    multi: boolean;
    chosen?: number[];
    disabled?: boolean;
  }

  let { values, multi, chosen = $bindable([]), disabled = false }: Props = $props();

  const name = $props.id();
  const offered = $derived(values.filter((value) => !value.archived || chosen.includes(value.id)));

  function toggle(id: number, on: boolean): void {
    chosen = on ? [...chosen.filter((other) => other !== id), id] : chosen.filter((x) => x !== id);
  }
</script>

<fieldset class="defaults" {disabled} aria-describedby="{name}-hint">
  <legend>{multi ? m.defaultValues() : m.defaultValue()}</legend>
  <p id="{name}-hint" class="hint">{m.defaultsHint()}</p>
  <ul class="list">
    {#if !multi}
      <li>
        <label class="option">
          <input
            type="radio"
            {name}
            checked={chosen.length === 0}
            onchange={() => {
              chosen = [];
            }}
          />
          <span>{m.noDefault()}</span>
        </label>
      </li>
    {/if}
    {#each offered as value (value.id)}
      <li>
        <label class="option">
          <input
            type={multi ? 'checkbox' : 'radio'}
            {name}
            checked={chosen.includes(value.id)}
            onchange={(event) => {
              if (multi) toggle(value.id, event.currentTarget.checked);
              else chosen = [value.id];
            }}
          />
          <ValueChip name={value.name} color={value.color} />
        </label>
      </li>
    {/each}
  </ul>
</fieldset>

<style>
  .defaults {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
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

  .hint {
    color: var(--color-text-muted);
    font-size: var(--text-sm);
  }

  .list {
    display: flex;
    flex-direction: column;
    max-height: 16rem;
    margin: 0;
    padding: 0;
    overflow-y: auto;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    list-style: none;
  }

  .option {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    min-height: var(--control-md);
    padding: var(--space-1) var(--space-3);
    cursor: pointer;
  }

  .option:hover {
    background: var(--color-hover);
  }

  li + li .option {
    border-top: 1px solid var(--color-border);
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

  fieldset:disabled .option {
    cursor: default;
    opacity: 0.5;
  }
</style>
