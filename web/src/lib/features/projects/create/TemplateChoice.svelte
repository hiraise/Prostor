<script lang="ts">
  // The template of a new project (PRJ-017, PRJ-021): one card per built-in template with what it
  // brings. Native radio buttons, so arrows move between them; the label covers its card, so the
  // whole card is the click and touch target. The radio dot and a stronger edge mark the choice.
  import type { LucideIcon } from '@lucide/svelte';
  import IterationCcw from '@lucide/svelte/icons/iteration-ccw';
  import Kanban from '@lucide/svelte/icons/kanban';
  import SquareDashed from '@lucide/svelte/icons/square-dashed';

  import type { TemplateKey } from '#lib/api/schema.gen.ts';

  import * as shared from '../messages.ts';
  import * as m from './messages.ts';

  let { value = $bindable('blank') }: { value?: TemplateKey } = $props();

  const id = $props.id();
  const choices: readonly { key: TemplateKey; icon: LucideIcon }[] = [
    { key: 'blank', icon: SquareDashed },
    { key: 'scrum', icon: IterationCcw },
    { key: 'kanban', icon: Kanban },
  ];
</script>

<fieldset class="templates" aria-describedby="{id}-hint">
  <legend>{m.template()}</legend>
  <p id="{id}-hint" class="hint">{m.templateHint()}</p>
  <div class="cards">
    {#each choices as choice (choice.key)}
      {@const texts = m.templates[choice.key]}
      <div class="card">
        <input
          id="{id}-{choice.key}"
          type="radio"
          name="{id}-template"
          value={choice.key}
          bind:group={value}
          aria-describedby="{id}-{choice.key}-what"
        />
        <span class="icon"><choice.icon aria-hidden="true" /></span>
        <label for="{id}-{choice.key}">{shared.templateNames[choice.key]()}</label>
        <div id="{id}-{choice.key}-what" class="what">
          <p class="summary">{texts.summary()}</p>
          <ul aria-label={m.brings()}>
            {#each texts.items as item, index (index)}
              <li>{item()}</li>
            {/each}
          </ul>
        </div>
      </div>
    {/each}
  </div>
</fieldset>

<style>
  .templates {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
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

  .cards {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(13rem, 1fr));
    gap: var(--space-3);
    margin-top: var(--space-2);
  }

  .card {
    position: relative;
    display: grid;
    grid-template-columns: auto auto 1fr;
    align-items: center;
    gap: var(--space-1) var(--space-2);
    padding: var(--space-3) var(--space-4) var(--space-4);
    border: 1px solid var(--color-border-control);
    border-radius: var(--radius-lg);
    background: var(--color-surface);
    transition:
      border-color var(--duration-fast) var(--ease-out),
      background var(--duration-fast) var(--ease-out);
  }

  .card:hover {
    background: var(--color-hover);
  }

  /* The chosen card: a stronger edge besides the radio dot. */
  .card:has(input:checked) {
    border-color: var(--color-ink);
    box-shadow: inset 0 0 0 1px var(--color-ink);
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

  .icon {
    display: inline-flex;
    color: var(--color-text-muted);
  }

  .icon :global(svg) {
    width: var(--icon-lg);
    height: var(--icon-lg);
  }

  label {
    display: flex;
    align-items: center;
    min-height: var(--control-sm);
    font-weight: var(--weight-semibold);
    cursor: pointer;
  }

  /* The label covers the card: anywhere on it chooses the template. */
  label::after {
    content: '';
    position: absolute;
    inset: 0;
    border-radius: inherit;
  }

  .what {
    display: flex;
    grid-column: 1 / -1;
    flex-direction: column;
    gap: var(--space-1);
    color: var(--color-text-muted);
    font-size: var(--text-sm);
  }

  .summary {
    color: var(--color-text);
  }

  ul {
    display: flex;
    flex-direction: column;
    gap: var(--space-0_5);
    margin: 0;
    padding-left: var(--space-4);
  }

  @media (forced-colors: active) {
    .card:has(input:checked) {
      outline: 2px solid Highlight;
    }
  }
</style>
