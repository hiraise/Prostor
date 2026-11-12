<script lang="ts">
  // The settings of a board (BRD-086): a side panel (the whole screen on phones) with its tabs.
  // «Основное», «Колонки», «Дорожки», «Карточка» and «Бэклог».
  // Every change saves at once; a change that alters the board's view reloads it (`onchanged`).
  import type { Board, Sprint } from '#lib/api/schema.gen.ts';
  import Dialog from '#lib/ui/Dialog.svelte';

  import * as m from '../messages.ts';
  import BacklogTab from './BacklogTab.svelte';
  import CardTab from './CardTab.svelte';
  import ColumnsTab from './ColumnsTab.svelte';
  import GeneralTab from './GeneralTab.svelte';
  import LanesTab from './LanesTab.svelte';
  import type { SettingsTab } from './settings.ts';

  interface Props {
    open?: boolean;
    board: Board;
    /** Sprints that are not archived («Основное»: the sprint new issues join). */
    sprints?: readonly Sprint[];
    onchanged: () => unknown;
    /** The tab shown (the health notice opens the tab of a finding). */
    tab?: SettingsTab;
  }

  let {
    open = $bindable(false),
    board,
    sprints = [],
    onchanged,
    tab = $bindable('general'),
  }: Props = $props();

  const TABS = [
    { key: 'general', label: m.generalTab },
    { key: 'columns', label: m.columnsTab },
    { key: 'lanes', label: m.lanesTab },
    { key: 'card', label: m.cardTab },
    { key: 'backlog', label: m.backlogTab },
  ] as const satisfies readonly { key: SettingsTab; label: () => string }[];

  const id = $props.id();
  let busy = $state(false);
  const buttons: HTMLButtonElement[] = [];

  function onkeydown(event: KeyboardEvent, index: number): void {
    const step = { ArrowRight: 1, ArrowLeft: -1, Home: -index, End: TABS.length - 1 - index }[
      event.key
    ];
    if (step === undefined) return;
    event.preventDefault();
    const next = (index + step + TABS.length) % TABS.length;
    const found = TABS[next];
    if (!found) return;
    tab = found.key;
    buttons[next]?.focus();
  }
</script>

<Dialog bind:open title={m.settings()} side="end" dismissible={!busy}>
  <div class="tabs" role="tablist" aria-label={m.tabs()}>
    {#each TABS as one, index (one.key)}
      <button
        bind:this={buttons[index]}
        type="button"
        role="tab"
        id="{id}-{one.key}-tab"
        aria-controls="{id}-{one.key}"
        aria-selected={tab === one.key}
        tabindex={tab === one.key ? 0 : -1}
        onclick={() => {
          tab = one.key;
        }}
        onkeydown={(event) => onkeydown(event, index)}>{one.label()}</button
      >
    {/each}
  </div>
  <div class="tab" role="tabpanel" id="{id}-{tab}" aria-labelledby="{id}-{tab}-tab">
    {#if tab === 'general'}
      <GeneralTab
        {board}
        {sprints}
        bind:busy
        {onchanged}
        ondeleted={() => {
          open = false;
        }}
      />
    {:else if tab === 'columns'}
      <ColumnsTab {board} bind:busy {onchanged} />
    {:else if tab === 'lanes'}
      <LanesTab {board} bind:busy {onchanged} />
    {:else if tab === 'card'}
      <CardTab {board} bind:busy {onchanged} />
    {:else}
      <BacklogTab {board} bind:busy {onchanged} />
    {/if}
  </div>
</Dialog>

<style>
  .tabs {
    display: flex;
    gap: var(--space-1);
    margin-bottom: var(--space-4);
    border-bottom: 1px solid var(--color-border);
  }

  [role='tab'] {
    min-height: var(--control-md);
    margin-bottom: -1px;
    padding: 0 var(--space-3);
    border: 0;
    border-bottom: 2px solid transparent;
    background: none;
    color: var(--color-text-muted);
    font: inherit;
    font-weight: var(--weight-medium);
    cursor: pointer;
  }

  [role='tab']:hover {
    color: var(--color-text);
  }

  [role='tab'][aria-selected='true'] {
    border-bottom-color: var(--color-brand);
    color: var(--color-text);
  }

  @media (pointer: coarse) {
    [role='tab'] {
      min-height: 2.75rem;
    }
  }
</style>
