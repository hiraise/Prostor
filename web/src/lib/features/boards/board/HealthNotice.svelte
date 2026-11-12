<script lang="ts">
  // The board's health (BRD-023): what is wrong with its settings, errors first, each with a
  // link to the settings tab that fixes it for people who may edit the board.
  import type { Board, BoardHealth } from '#lib/api/schema.gen.ts';
  import Button from '#lib/ui/Button.svelte';
  import Notice from '#lib/ui/Notice.svelte';

  import * as m from '../messages.ts';
  import type { SettingsTab } from '../settings/settings.ts';

  interface Props {
    board: Board;
    /** Opens the settings on a tab; absent for people who may not edit the board. */
    onopen?: (tab: SettingsTab) => unknown;
    /** Findings the page already shows another way. */
    skip?: readonly BoardHealth['kind'][];
  }

  let { board, onopen, skip = [] }: Props = $props();

  const findings = $derived(
    board.health
      .filter((one) => !skip.includes(one.kind))
      .toSorted((a, b) => Number(a.severity === 'warning') - Number(b.severity === 'warning')),
  );
  const tone = $derived(findings.some((one) => one.severity === 'error') ? 'danger' : 'warning');

  function text(one: BoardHealth): string {
    const project = board.projects.find((p) => p.project.id === one.project)?.project.name ?? '';
    const value = board.columns.find((c) => c.value.id === one.value)?.value.name ?? '';
    const slot = one.slot ? m.healthSlots[one.slot]() : '';
    return m.healthKinds[one.kind]({ project, value, slot });
  }

  function tabOf(one: BoardHealth): SettingsTab {
    switch (one.kind) {
      case 'no_projects':
      case 'archived_project':
        return 'general';
      case 'backlog_hidden':
        return 'backlog';
      case 'field_missing':
      case 'field_wrong_kind':
        return one.slot === 'lane' ? 'lanes' : 'card';
      default:
        return 'columns';
    }
  }
</script>

{#if findings.length > 0}
  <Notice {tone}>
    <p class="title">{m.healthTitle()}</p>
    <ul class="list">
      {#each findings as one, index (index)}
        <li>
          <span>{text(one)}</span>
          {#if onopen}
            <Button size="sm" variant="ghost" onclick={() => onopen(tabOf(one))}
              >{m.healthOpen()}</Button
            >
          {/if}
        </li>
      {/each}
    </ul>
  </Notice>
{/if}

<style>
  .title {
    font-weight: var(--weight-semibold);
  }

  .list {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .list li {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-2);
  }
</style>
