<script lang="ts">
  // «Создать» (UI-002): a menu of what the person may create — an issue for people who may create
  // issues somewhere, then what other features add (`more`: a board). Hidden when it would be
  // empty. In the rail it is a labelled button, an icon when the rail is folded; on phones a «+»
  // in the top bar.
  import Plus from '@lucide/svelte/icons/plus';
  import SquarePen from '@lucide/svelte/icons/square-pen';

  import { canAnywhere } from '#lib/session/index.ts';
  import Button from '#lib/ui/Button.svelte';
  import IconButton from '#lib/ui/IconButton.svelte';
  import Menu, { type MenuEntry } from '#lib/ui/Menu.svelte';
  import type { Placement } from '#lib/ui/position.ts';

  import * as m from '../messages.ts';
  import { openNewIssue } from './open.svelte.ts';

  interface Props {
    /** Only the icon: the folded rail and the top bar. */
    compact?: boolean;
    placement?: Placement;
    /** Entries after «Задачу», already limited to what the person may create. */
    more?: MenuEntry[];
  }

  let { compact = false, placement = { side: 'bottom', align: 'start' }, more }: Props = $props();

  const items = $derived.by((): MenuEntry[] => {
    const list: MenuEntry[] = [];
    if (canAnywhere('issue.create')) {
      list.push({
        type: 'action',
        label: m.createIssueItem(),
        icon: SquarePen,
        onSelect: () => {
          openNewIssue();
        },
      });
    }
    for (const entry of more ?? []) list.push(entry);
    return list;
  });
</script>

{#if items.length > 0}
  <Menu label={m.create()} {placement} {items}>
    {#snippet trigger(props)}
      {#if compact}
        <IconButton icon={Plus} variant="secondary" label={m.create()} {...props} />
      {:else}
        <Button icon={Plus} {...props}>{m.create()}</Button>
      {/if}
    {/snippet}
  </Menu>
{/if}
