<script lang="ts">
  // The shortcut reference (UI-020, step 2.2 §5): the bindings of the active context plus the
  // global ones, grouped by purpose, rendered for a chosen platform. «Печать» prints the card
  // alone — a print stylesheet shows only the table, and the platform switch chooses which
  // glyphs print. Loaded lazily by the shell: first use pays (§5). Phones: the dialog is a bottom
  // sheet and the card still prints; nothing else changes.
  import Printer from '@lucide/svelte/icons/printer';

  import type { Binding, ContextId } from '#lib/shortcuts/kinds.ts';
  import {
    bindingGroups,
    contextGroups,
    contextTitles,
    globalGroups,
    globalTitle,
    groups,
    labels,
    type GroupId,
  } from '#lib/shortcuts/messages.ts';
  import {
    comboChips,
    detectPlatform,
    sequenceChips,
    type Platform,
  } from '#lib/shortcuts/platform.ts';
  import { active } from '#lib/shortcuts/registry.svelte.ts';
  import { tables } from '#lib/shortcuts/tables/index.ts';
  import Button from '#lib/ui/Button.svelte';
  import Dialog from '#lib/ui/Dialog.svelte';
  import RadioGroup from '#lib/ui/RadioGroup.svelte';
  import TextField from '#lib/ui/TextField.svelte';

  import * as m from './messages.ts';

  let { open = $bindable(false) }: { open?: boolean } = $props();

  const context = $derived(active());
  /** The glyphs of the card: the person's platform until they switch — also what prints. */
  let platform = $state<Platform>(detectPlatform());
  let filter = $state('');

  interface Row {
    id: string;
    label: string;
    /** The chips of one or two presses (a sequence). */
    keys: string[][];
  }
  interface Group {
    title: string;
    rows: Row[];
  }

  function rowOf(binding: Binding): Row {
    return {
      id: binding.id,
      label: (labels as Record<string, () => string | undefined>)[binding.id]?.() ?? binding.id,
      keys:
        'code' in binding.combo
          ? [comboChips(binding.combo, platform, binding.display)]
          : sequenceChips(binding.combo),
    };
  }

  const GLOBAL_GROUP_ORDER: readonly GroupId[] = ['navigation', 'actions', 'reference'];

  /** One section's groups with rows that pass the filter, in display order, empty ones gone. */
  function groupsOf(
    section: 'global' | ContextId,
    wanted: readonly GroupId[],
    groupOf: (id: string) => GroupId | undefined,
    needle: string,
  ): Group[] {
    const result: Group[] = [];
    for (const group of wanted) {
      const rows: Row[] = [];
      for (const binding of tables[section]) {
        if (binding.if?.() === false) continue;
        if (groupOf(binding.id) !== group) continue;
        const row = rowOf(binding);
        const haystack = `${row.label} ${row.keys.flat().join(' ')}`.toLowerCase();
        if (needle !== '' && !haystack.includes(needle)) continue;
        rows.push(row);
      }
      if (rows.length > 0) result.push({ title: groups[group](), rows });
    }
    return result;
  }

  const sections = $derived.by(() => {
    const needle = filter.trim().toLowerCase();
    const result: { title: string; groups: Group[] }[] = [];
    const everywhere = groupsOf(
      'global',
      GLOBAL_GROUP_ORDER,
      (id) => (globalGroups as Record<string, GroupId | undefined>)[id],
      needle,
    );
    if (everywhere.length > 0) result.push({ title: globalTitle(), groups: everywhere });
    const id = context;
    if (id !== null) {
      const own = groupsOf(
        id,
        contextGroups[id],
        (binding) => (bindingGroups[id] as Record<string, GroupId | undefined>)[binding],
        needle,
      );
      if (own.length > 0) result.push({ title: contextTitles[id](), groups: own });
    }
    return result;
  });

  // ---- The printed card (§5) ----

  function print(): void {
    document.documentElement.classList.add('tf-print-card');
    window.print();
  }

  function stopPrinting(): void {
    document.documentElement.classList.remove('tf-print-card');
  }
</script>

<!-- The browser's own print while the overlay is open prints the card too. -->
<svelte:window
  onbeforeprint={() => {
    if (open) document.documentElement.classList.add('tf-print-card');
  }}
  onafterprint={stopPrinting}
/>

<Dialog bind:open title={m.shortcutsTitle()} size="lg">
  <div class="card tf-shortcuts-card">
    <div class="controls">
      <TextField
        label={m.shortcutsFilterLabel()}
        placeholder={m.shortcutsFilterPlaceholder()}
        bind:value={filter}
      />
      <RadioGroup
        legend={m.shortcutsPlatform()}
        name="shortcuts-platform"
        options={[
          { value: 'mac', label: m.shortcutsPlatformMac() },
          { value: 'pc', label: m.shortcutsPlatformPc() },
        ]}
        bind:value={platform}
      />
      <Button variant="secondary" icon={Printer} onclick={print}>{m.shortcutsPrint()}</Button>
    </div>

    {#each sections as section (section.title)}
      <section aria-label={section.title}>
        <h3>{section.title}</h3>
        {#each section.groups as group (group.title)}
          <h4>{group.title}</h4>
          <ul class="rows">
            {#each group.rows as row (row.id)}
              <li>
                <span class="keys" aria-hidden="true">
                  {#each row.keys as press, i (i)}
                    {#if i > 0}<span class="then"></span>{/if}
                    {#each press as chip, j (j)}<kbd>{chip}</kbd>{/each}
                  {/each}
                </span>
                <span class="label">{row.label}</span>
              </li>
            {/each}
          </ul>
        {/each}
      </section>
    {:else}
      <p class="empty">{m.shortcutsNoMatches()}</p>
    {/each}

    <p class="caption">{m.shortcutsCaption()}</p>
  </div>
</Dialog>

<style>
  .card {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }

  .controls {
    display: flex;
    flex-wrap: wrap;
    align-items: start;
    gap: var(--space-3);
  }

  .controls :global(.field) {
    flex: 1;
    min-width: 12rem;
  }

  section {
    display: flex;
    flex-direction: column;
  }

  h3 {
    margin: 0 0 var(--space-1);
    font-size: var(--text-md);
    font-weight: var(--weight-medium);
  }

  h4 {
    margin: var(--space-2) 0 var(--space-1);
    color: var(--color-text-muted);
    font-size: var(--text-sm);
    font-weight: var(--weight-medium);
  }

  .rows {
    display: flex;
    flex-direction: column;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .rows li {
    display: flex;
    align-items: baseline;
    gap: var(--space-3);
    padding: var(--space-1) 0;
    break-inside: avoid;
  }

  .keys {
    display: inline-flex;
    flex: none;
    align-items: center;
    gap: var(--space-1);
    min-width: 8rem;
  }

  kbd {
    min-width: 1.75em;
    padding: 0 var(--space-1);
    border: 1px solid var(--color-border);
    border-bottom-width: 2px;
    border-radius: var(--radius-sm);
    background: var(--color-surface);
    text-align: center;
  }

  .then {
    width: var(--space-2);
  }

  .label {
    min-width: 0;
  }

  .empty {
    margin: 0;
    color: var(--color-text-muted);
  }

  .caption {
    margin: 0;
    padding-top: var(--space-2);
    border-top: 1px solid var(--color-border);
    color: var(--color-text-muted);
    font-size: var(--text-sm);
  }

  /* The printed reference card (§5): only the table, of the chosen platform, on a plain page.
   * The rules are global: they restyle the page around the card, not the card alone. */
  @media print {
    :global(html.tf-print-card body *) {
      visibility: hidden;
    }

    :global(html.tf-print-card .tf-shortcuts-card),
    :global(html.tf-print-card .tf-shortcuts-card *) {
      visibility: visible;
    }

    :global(html.tf-print-card dialog) {
      position: static;
      width: auto;
      max-height: none;
      border: none;
      background: none;
      box-shadow: none;
      overflow: visible;
    }

    :global(html.tf-print-card .tf-shortcuts-card) {
      position: absolute;
      inset-block-start: 0;
      inset-inline: 0;
    }

    .controls,
    .caption {
      display: none;
    }

    h3,
    h4 {
      break-after: avoid;
    }
  }
</style>
