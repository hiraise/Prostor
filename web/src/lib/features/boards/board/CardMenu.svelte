<script lang="ts">
  // The card menu (BRD-068, BRD-074; UI-032): what a drag does, for keyboards and screen readers
  // — another column, the top or the bottom of the cell, after a chosen card — watching the issue
  // (NTF-014), taking the card off the sprint or the board, and opening the issue. Its entries are
  // built when the menu is about to open, so 500 cards do not carry 500 filled menus.
  import Ellipsis from '@lucide/svelte/icons/ellipsis';
  import Star from '@lucide/svelte/icons/star';
  import StarOff from '@lucide/svelte/icons/star-off';

  import { goto } from '$app/navigation';
  import type { Board, CardIssue } from '#lib/api/schema.gen.ts';
  import { issueHref } from '#lib/features/issues/index.ts';
  import * as common from '#lib/i18n/messages/common.ts';
  import Button from '#lib/ui/Button.svelte';
  import Dialog from '#lib/ui/Dialog.svelte';
  import IconButton from '#lib/ui/IconButton.svelte';
  import Menu, { type MenuEntry } from '#lib/ui/Menu.svelte';
  import Select from '#lib/ui/Select.svelte';

  import * as m from '../messages.ts';
  import type { BoardState } from './board-state.svelte.ts';

  interface Props {
    issue: CardIssue;
    board: Board;
    model: BoardState;
  }

  let { issue, board, model }: Props = $props();

  const name = $derived(issue.readable_id ?? String(issue.id));
  let wanted = $state(false);
  let afterOpen = $state(false);
  let afterId = $state('');

  const none: MenuEntry[] = [];
  const items = $derived(wanted ? entries() : none);

  /** The watch marker of the card's issue (NTF-014): anyone who reads it may toggle it. */
  function pushWatch(list: MenuEntry[]): void {
    list.push({
      type: 'action',
      label: issue.starred ? m.unwatchCard() : m.watchCard(),
      icon: issue.starred ? StarOff : Star,
      onSelect: () => void model.watch(issue.id, !issue.starred),
    });
  }

  function entries(): MenuEntry[] {
    const open: MenuEntry = {
      type: 'action',
      label: m.openIssue(),
      // eslint-disable-next-line svelte/no-navigation-without-resolve -- issueHref resolves it.
      onSelect: () => void goto(issueHref({ id: issue.id, readable_id: issue.readable_id })),
    };
    const list: MenuEntry[] = [];
    const place = model.placeOf(issue.id);
    if (!place || !model.canMove(issue.id)) {
      pushWatch(list);
      list.push(open);
      return list;
    }
    const ids = model.cell(place.column, place.lane)?.ids ?? [];
    const columns = board.columns.filter(
      (column) => column.id !== place.column && model.cell(column.id, place.lane) !== undefined,
    );
    if (columns.length > 0) {
      list.push({
        type: 'group',
        label: m.moveToColumn(),
        items: columns.map((column) => ({
          type: 'action',
          label: column.value.name,
          onSelect: () =>
            void model.move(issue.id, {
              column: column.id,
              index: model.cell(column.id, place.lane)?.ids.length ?? 0,
            }),
        })),
      });
    }
    // Lanes shown and open (BRD-050, BRD-051); project and date lanes take no drops.
    const lanes = model.lanes.filter(
      (lane) =>
        lane.key !== place.lane &&
        !/^[pd]:/.test(lane.key) &&
        model.cell(place.column, lane.key) !== undefined,
    );
    if (lanes.length > 0 && !/^[pd]:/.test(place.lane ?? '')) {
      list.push({
        type: 'group',
        label: m.moveToLane(),
        items: lanes.map((lane) => ({
          type: 'action',
          label: model.laneLabel(lane.key).name,
          onSelect: () =>
            void model.move(issue.id, {
              column: place.column,
              lane: lane.key,
              index: model.cell(place.column, lane.key)?.ids.length ?? 0,
            }),
        })),
      });
    }
    list.push({
      type: 'group',
      label: m.moveInColumn(),
      items: [
        {
          type: 'action',
          label: m.toStart(),
          disabled: place.index === 0,
          onSelect: () => void model.move(issue.id, { column: place.column, index: 0 }),
        },
        {
          type: 'action',
          label: m.toEnd(),
          disabled: place.index === ids.length - 1,
          onSelect: () =>
            void model.move(issue.id, { column: place.column, index: ids.length - 1 }),
        },
        {
          type: 'action',
          label: m.afterCard(),
          disabled: ids.length < 2,
          onSelect: () => {
            afterId = String(ids.find((id) => id !== issue.id) ?? '');
            afterOpen = true;
          },
        },
      ],
    });
    // The watch marker of the card's issue (NTF-014): anyone who reads it may toggle it.
    pushWatch(list);
    list.push({ type: 'separator' });
    if (model.canRemove(issue.id)) {
      list.push({
        type: 'action',
        label: board.sprints_on ? m.removeFromSprint() : m.removeFromBoard(),
        danger: true,
        onSelect: () => void model.remove(issue.id),
      });
    }
    list.push(open);
    return list;
  }

  const others = $derived.by(() => {
    if (!afterOpen) return [];
    const place = model.placeOf(issue.id);
    const ids = place ? (model.cell(place.column, place.lane)?.ids ?? []) : [];
    return ids
      .filter((id) => id !== issue.id)
      .map((id) => {
        const other = model.issues.get(id);
        return {
          value: String(id),
          label: `${other?.readable_id ?? String(id)} ${other?.summary ?? ''}`,
        };
      });
  });

  function moveAfter(): void {
    const place = model.placeOf(issue.id);
    const after = Number(afterId);
    if (!place || !afterId) return;
    const ids = (model.cell(place.column, place.lane)?.ids ?? []).filter((id) => id !== issue.id);
    const index = ids.indexOf(after);
    afterOpen = false;
    if (index >= 0) void model.move(issue.id, { column: place.column, index: index + 1 });
  }

  function want(): void {
    wanted = true;
  }
</script>

<Menu label={m.cardActions({ id: name })} {items} placement={{ side: 'bottom', align: 'end' }}>
  {#snippet trigger(props)}
    <IconButton
      icon={Ellipsis}
      label={m.cardActions({ id: name })}
      size="sm"
      class="card-menu"
      onfocus={want}
      onpointerenter={want}
      onpointerdown={want}
      {...props}
    />
  {/snippet}
</Menu>

{#if afterOpen}
  <Dialog bind:open={afterOpen} title={m.afterTitle({ id: name })} size="sm">
    <Select label={m.afterLabel()} options={others} bind:value={afterId} required />
    {#snippet footer()}
      <Button
        onclick={() => {
          afterOpen = false;
        }}>{common.cancel()}</Button
      >
      <Button variant="primary" disabled={afterId === ''} onclick={moveAfter}
        >{m.moveAction()}</Button
      >
    {/snippet}
  </Dialog>
{/if}
