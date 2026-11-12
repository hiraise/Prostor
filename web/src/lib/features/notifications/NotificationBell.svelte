<script lang="ts">
  // The notification bell (UI-001's part, NTF-048): the unread dot over the bell, the inbox in a
  // popover next to it (a bottom sheet on phones, D-019). While the bell is shown the person's
  // `inbox:` topic is followed, so the badge — and the open popup — stay live after sign-in.
  import Bell from '@lucide/svelte/icons/bell';

  import type { CurrentUser } from '#lib/api/schema.gen.ts';
  import IconButton from '#lib/ui/IconButton.svelte';
  import Popover from '#lib/ui/Popover.svelte';
  import type { Placement } from '#lib/ui/position.ts';

  import { followInbox, inboxSummary, type InboxSource } from './inbox.svelte.ts';
  import InboxPopup from './InboxPopup.svelte';
  import * as m from './messages.ts';

  interface Props {
    user: CurrentUser;
    placement?: Placement;
    /** The live connection; the app's shared one unless a test brings its own. */
    connection?: InboxSource;
  }

  let { user, placement = { side: 'bottom', align: 'end' }, connection }: Props = $props();

  const unread = $derived(inboxSummary());
  const count = $derived(unread?.total ?? 0);
  const label = $derived(count > 0 ? m.bellUnread({ count }) : m.bell());

  // The follow starts when the bell appears and restarts if the signed-in person changes.
  $effect(() => {
    return followInbox(user.id, connection ? { connection } : {});
  });
</script>

<Popover {label} {placement}>
  {#snippet trigger(props)}
    <span class="bell">
      <IconButton icon={Bell} {label} {...props} />
      {#if count > 0}<span class="dot" aria-hidden="true"></span>{/if}
    </span>
  {/snippet}
  {#snippet children({ close })}
    <InboxPopup onnavigate={close} />
  {/snippet}
</Popover>

<style>
  .bell {
    position: relative;
    display: inline-flex;
  }

  .dot {
    position: absolute;
    top: calc(var(--space-1) + var(--space-0_5));
    inset-inline-end: var(--space-1);
    width: var(--space-2);
    height: var(--space-2);
    border: 2px solid var(--color-surface-sunken);
    border-radius: var(--radius-full);
    background: var(--color-brand);
    pointer-events: none;
  }
</style>
