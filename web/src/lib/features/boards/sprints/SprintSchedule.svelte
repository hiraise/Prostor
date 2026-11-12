<script lang="ts">
  // The shown sprint's schedule (BRD-083): its dates (a button that edits them for editors) and the
  // countdown in calendar days, weekends included; the last day shows only a flag. Editors start
  // and finish a sprint by hand here (D-013).
  import Flag from '@lucide/svelte/icons/flag';

  import type { Sprint } from '#lib/api/schema.gen.ts';
  import Button from '#lib/ui/Button.svelte';

  import { sprintPhase } from '../boards.ts';
  import * as m from '../messages.ts';
  import { sprintDates, today } from './schedule.ts';

  interface Props {
    sprint: Sprint;
    edit: boolean;
    /** The current time, for tests; the clock by default. */
    now?: number;
    onedit: () => unknown;
    onstart: () => unknown;
    onfinish: () => unknown;
  }

  let { sprint, edit, now, onedit, onstart, onfinish }: Props = $props();

  const phase = $derived(sprintPhase(sprint, today(now)));
  const dates = $derived(sprintDates(sprint));
  const running = $derived(phase.kind === 'running' || phase.kind === 'open');
</script>

<div class="schedule" role="group" aria-label={m.schedule()}>
  {#if edit && !sprint.archived}
    <Button size="sm" variant="ghost" aria-label="{m.schedule()}: {dates}" onclick={() => onedit()}
      >{dates}</Button
    >
  {:else}
    <span class="dates">{dates}</span>
  {/if}
  {#if phase.kind === 'running' && phase.days === 0}
    <span class="count last"><Flag aria-hidden="true" />{m.lastDay()}</span>
  {:else if phase.kind === 'running'}
    <span class="count">{m.daysLeft({ days: phase.days })}</span>
  {:else if phase.kind === 'ahead'}
    <span class="count">{m.startsIn({ days: phase.days })}</span>
  {:else if phase.kind === 'finished'}
    <span class="count">{m.sprintFinished()}</span>
  {/if}
  {#if edit && !sprint.archived}
    {#if running || phase.kind === 'none'}
      {#if phase.kind === 'none'}
        <Button size="sm" variant="ghost" onclick={() => onstart()}>{m.startSprint()}</Button>
      {/if}
      <Button size="sm" variant="ghost" onclick={() => onfinish()}>{m.finishSprint()}</Button>
    {:else}
      <Button size="sm" variant="ghost" onclick={() => onstart()}>{m.startSprint()}</Button>
    {/if}
  {/if}
</div>

<style>
  .schedule {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-1) var(--space-2);
    font-size: var(--text-sm);
  }

  .dates,
  .count {
    color: var(--color-text-muted);
  }

  .last {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
    color: var(--color-text);
  }

  .last :global(svg) {
    width: var(--icon-sm);
    height: var(--icon-sm);
  }
</style>
