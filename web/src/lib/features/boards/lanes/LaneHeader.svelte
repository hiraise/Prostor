<script lang="ts">
  // A lane's header (BRD-053): collapse for this person, the title (a parent lane's ID links to
  // the issue), the count of cards and their estimation, and a menu for editors: up and down,
  // and a parent lane back to the backlog (BRD-056). It stays at the left edge while the grid
  // scrolls sideways.
  import ChevronDown from '@lucide/svelte/icons/chevron-down';
  import ChevronRight from '@lucide/svelte/icons/chevron-right';
  import EllipsisVertical from '@lucide/svelte/icons/ellipsis-vertical';

  import type { Board, Lane, ProjectRef } from '#lib/api/schema.gen.ts';
  import { ValueChip } from '#lib/features/fields/index.ts';
  import { IssueRef } from '#lib/features/issues/index.ts';
  import IconButton from '#lib/ui/IconButton.svelte';
  import Menu, { type MenuEntry } from '#lib/ui/Menu.svelte';

  import { estimateText } from '../boards.ts';
  import type { BoardState } from '../board/board-state.svelte.ts';
  import * as m from '../messages.ts';

  interface Props {
    board: Board;
    model: BoardState;
    lane: Lane;
    collapsed: boolean;
  }

  let { board, model, lane, collapsed }: Props = $props();

  const label = $derived(model.laneLabel(lane.key));
  const parent = $derived(lane.issue === null ? undefined : model.issues.get(lane.issue));
  const brief = $derived.by(() => {
    if (!parent) return null;
    const project: ProjectRef = board.projects.find((one) => one.project.id === parent.project)
      ?.project ?? {
      id: parent.project,
      name: '',
      key: '',
      archived: false,
      logo_color: 0,
      logo_url: null,
    };
    return {
      id: parent.id,
      readable_id: parent.readable_id ?? String(parent.id),
      summary: parent.summary,
      project,
      resolved: parent.resolved_at !== null,
      restricted: false,
    };
  });
  const colored = $derived(lane.key.startsWith('v:') || lane.key.startsWith('t:'));

  const actions = $derived.by((): MenuEntry[] => {
    const list: MenuEntry[] = [];
    if (model.canMoveLane(lane.key, -1) || model.canMoveLane(lane.key, 1)) {
      list.push(
        {
          type: 'action',
          label: m.moveUp(),
          disabled: !model.canMoveLane(lane.key, -1),
          onSelect: () => void model.moveLane(lane.key, -1),
        },
        {
          type: 'action',
          label: m.moveDown(),
          disabled: !model.canMoveLane(lane.key, 1),
          onSelect: () => void model.moveLane(lane.key, 1),
        },
      );
    }
    if (model.canLaneToBacklog(lane)) {
      if (list.length > 0) list.push({ type: 'separator' });
      list.push({
        type: 'action',
        label: m.laneToBacklog(),
        onSelect: () => void model.laneToBacklog(lane),
      });
    }
    return list;
  });
</script>

<div class={['header', { collapsed }]}>
  <IconButton
    icon={collapsed ? ChevronRight : ChevronDown}
    size="sm"
    label={collapsed ? m.expandLane({ name: label.name }) : m.collapseLane({ name: label.name })}
    aria-expanded={!collapsed}
    onclick={() => model.toggleLane(lane.key)}
  />
  <h3 class="title">
    {#if brief}
      <span class="id"><IssueRef issue={brief} summary={false} /></span>
      <span class="name">{brief.summary}</span>
    {:else if colored && label.color !== null}
      <ValueChip name={label.name} color={label.color} />
    {:else}
      <span class={['name', { other: lane.key === 'other' }]}>{label.name}</span>
    {/if}
  </h3>
  <span class="count">{m.cardCount({ count: lane.count })}</span>
  {#if lane.estimate !== null}
    <span class="estimate">{estimateText(board, lane.estimate)}</span>
  {/if}
  {#if actions.length > 0}
    <Menu
      label={m.laneActions({ name: label.name })}
      items={actions}
      placement={{ side: 'bottom', align: 'start' }}
    >
      {#snippet trigger(props)}
        <IconButton
          icon={EllipsisVertical}
          size="sm"
          label={m.laneActions({ name: label.name })}
          {...props}
        />
      {/snippet}
    </Menu>
  {/if}
</div>

<style>
  .header {
    display: flex;
    position: sticky;
    left: 0;
    align-items: center;
    gap: var(--space-2);
    width: fit-content;
    max-width: min(100%, 40rem);
    padding: var(--space-1) var(--space-2);
  }

  .title {
    display: flex;
    align-items: baseline;
    gap: var(--space-2);
    min-width: 0;
    font-size: var(--text-sm);
    font-weight: var(--weight-semibold);
  }

  .id {
    flex-shrink: 0;
  }

  .name {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .name.other {
    color: var(--color-text-muted);
  }

  .count,
  .estimate {
    flex-shrink: 0;
    color: var(--color-text-muted);
    font-size: var(--text-xs);
    font-variant-numeric: tabular-nums;
  }
</style>
