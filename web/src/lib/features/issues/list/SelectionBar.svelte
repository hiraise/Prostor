<script lang="ts">
  // The selection toolbar (step 2.2 §8.2, QRY-084): the chosen count, the offer «Выбрать все N
  // задач по запросу», and the actions — every one through the command engine (D-064). Shown
  // while anything is chosen: a bar over the list, a bottom sheet on a phone (§8.1). The link,
  // tag and assignee entries open the pickers of §8.3; export (2.2b-3) adds its entries here.
  import { MediaQuery } from 'svelte/reactivity';

  import { NARROW } from '#lib/theme/breakpoints.ts';
  import Button from '#lib/ui/Button.svelte';

  import * as issueMessages from '../messages.ts';
  import * as m from './messages.ts';
  import type { ExportFormat } from './export.ts';

  interface Props {
    /** How many rows are chosen: the IDs', or the query's when everything matches. */
    count: number;
    /** The all-matching state (§8.1): the count names the query, rows have no own checkboxes. */
    allMatching: boolean;
    /** The offer of §8.1 — more issues match than the page shows; `null` when there is none. */
    matching: number | null;
    /** «Удалить» needs the delete right on what is chosen; the server checks anyway (§8.2). */
    canDelete: boolean;
    /** The link, tag and assignee flows need the update right (§8.3); the server checks anyway. */
    canChange: boolean;
    oncommands: () => void;
    /** Opens the link picker of §8.3. */
    onlink: () => void;
    /** Opens the tag picker of §8.3. */
    ontag: () => void;
    /** Opens the assignee picker of §8.3. */
    onassignee: () => void;
    /** Exports the selection in this format (§9) — read is all it needs, and reading the list
     * already asked for it. */
    onexport: (format: ExportFormat) => void;
    /** The format now being fetched, for the buttons' busy state. */
    exporting: ExportFormat | null;
    ondelete: () => void;
    onclear: () => void;
    /** Accepts «Выбрать все N задач по запросу» (§8.1). */
    onallmatching: () => void;
  }

  let {
    count,
    allMatching,
    matching,
    canDelete,
    canChange,
    oncommands,
    onlink,
    ontag,
    onassignee,
    onexport,
    exporting,
    ondelete,
    onclear,
    onallmatching,
  }: Props = $props();

  const phone = new MediaQuery(NARROW, false);

  // The sheet and the toasts share the phone's bottom edge (UI-038): while the sheet shows, the
  // toasts lift above it by its measured height — the Popover rule, for a sheet that stays. The
  // page learns the same height, so its content can scroll clear of the fixed sheet.
  let sheetEl: HTMLElement | undefined = $state();
  $effect(() => {
    const element = sheetEl;
    if (!element) return;
    const place = () => {
      document.body.style.setProperty('--toast-lift', `${String(element.offsetHeight + 12)}px`);
      document.body.style.setProperty('--selection-sheet', `${String(element.offsetHeight)}px`);
    };
    place();
    const observer = new ResizeObserver(place);
    observer.observe(element);
    return () => {
      observer.disconnect();
      document.body.style.removeProperty('--toast-lift');
      document.body.style.removeProperty('--selection-sheet');
    };
  });
</script>

{#if phone.current}
  <!-- The sheet: fixed at the bottom, out of the page's flow, targets ≥ 44 px (§8.1, D-019). -->
  <div class="sheet" role="region" aria-label={m.toolbar()} bind:this={sheetEl}>
    <p class="count" aria-live="polite">
      {#if allMatching}{m.allMatchingChosen({ count })}{:else}{m.selectedCount({ count })}{/if}
    </p>
    <div class="actions">
      {#if matching !== null}
        <button type="button" class="all" onclick={onallmatching}>
          {m.selectAllMatching({ count: matching })}
        </button>
      {/if}
      <Button variant="secondary" onclick={oncommands}>{issueMessages.commandsMenu()}</Button>
      {#if canChange}
        <Button variant="secondary" onclick={onlink}>{m.addLinkSelection()}</Button>
        <Button variant="secondary" onclick={ontag}>{m.tagSelection()}</Button>
        <Button variant="secondary" onclick={onassignee}>{m.assigneeSelection()}</Button>
      {/if}
      <Button variant="secondary" onclick={() => onexport('csv')} loading={exporting === 'csv'}>
        {m.exportCsv()}
      </Button>
      <Button variant="secondary" onclick={() => onexport('xlsx')} loading={exporting === 'xlsx'}>
        {m.exportXlsx()}
      </Button>
      {#if canDelete}
        <Button variant="secondary" onclick={ondelete}>{m.deleteSelection()}</Button>
      {/if}
      <Button variant="ghost" onclick={onclear}>{m.clearSelection()}</Button>
    </div>
  </div>
{:else}
  <div class="bar" role="region" aria-label={m.toolbar()}>
    <p class="count" aria-live="polite">
      {#if allMatching}{m.allMatchingChosen({ count })}{:else}{m.selectedCount({ count })}{/if}
    </p>
    {#if matching !== null}
      <button type="button" class="all" onclick={onallmatching}>
        {m.selectAllMatching({ count: matching })}
      </button>
    {/if}
    <div class="actions">
      <Button variant="secondary" size="sm" onclick={oncommands}>
        {issueMessages.commandsMenu()}
      </Button>
      {#if canChange}
        <Button variant="secondary" size="sm" onclick={onlink}>{m.addLinkSelection()}</Button>
        <Button variant="secondary" size="sm" onclick={ontag}>{m.tagSelection()}</Button>
        <Button variant="secondary" size="sm" onclick={onassignee}>
          {m.assigneeSelection()}
        </Button>
      {/if}
      <Button
        variant="secondary"
        size="sm"
        onclick={() => onexport('csv')}
        loading={exporting === 'csv'}
      >
        {m.exportCsv()}
      </Button>
      <Button
        variant="secondary"
        size="sm"
        onclick={() => onexport('xlsx')}
        loading={exporting === 'xlsx'}
      >
        {m.exportXlsx()}
      </Button>
      {#if canDelete}
        <Button variant="secondary" size="sm" onclick={ondelete}>{m.deleteSelection()}</Button>
      {/if}
      <Button variant="ghost" size="sm" onclick={onclear}>{m.clearSelection()}</Button>
    </div>
  </div>
{/if}

<style>
  .bar,
  .sheet {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-2);
  }

  .bar {
    padding: var(--space-2) var(--space-3);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    background: var(--color-surface-raised);
  }

  .count {
    margin: 0;
    color: var(--color-text-muted);
    font-size: var(--text-sm);
  }

  .all {
    padding: 0;
    border: 0;
    background: transparent;
    color: var(--color-link);
    font-size: var(--text-sm);
    text-decoration: underline;
    cursor: pointer;
  }

  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
    margin-inline-start: auto;
  }

  /* The phone's bottom sheet (§8.1): over the page at the screen's bottom edge, above the
     navigation bar. The actions share rows two wide — a sheet that stacks its every action
     full width (§11's dialog rule) would stand over half the list. */
  .sheet {
    position: fixed;
    z-index: 40;
    inset-inline: 0;
    bottom: 0;
    flex-direction: column;
    align-items: stretch;
    padding: var(--space-3) var(--space-4) calc(var(--space-3) + env(safe-area-inset-bottom));
    border-block: 1px solid var(--color-border);
    background: var(--color-surface-raised);
    box-shadow: var(--shadow-lg);
  }

  .sheet .actions {
    margin-inline-start: 0;
  }

  .sheet .actions :global(button) {
    flex: 1 1 calc(50% - var(--space-2));
  }

  /* Targets ≥ 44 px on touch (D-019). */
  @media (pointer: coarse) {
    .all {
      min-height: 44px;
    }

    .sheet :global(button) {
      min-height: 44px;
    }
  }
</style>
