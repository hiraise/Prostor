<script lang="ts">
  // The person's other drafts (ISS-011): each by its summary, or «Без названия», with the time it
  // was last changed; choosing one continues it in this form. «Удалить все черновики» asks first.
  import FileStack from '@lucide/svelte/icons/file-stack';
  import Trash2 from '@lucide/svelte/icons/trash-2';

  import { api } from '#lib/api/client.ts';
  import type { Issue } from '#lib/api/schema.gen.ts';
  import * as common from '#lib/i18n/messages/common.ts';
  import { getTimeContext } from '#lib/time/context.svelte.ts';
  import { formatInstant } from '#lib/time/format.ts';
  import Button from '#lib/ui/Button.svelte';
  import Dialog from '#lib/ui/Dialog.svelte';
  import Menu, { type MenuEntry } from '#lib/ui/Menu.svelte';

  import * as m from '../messages.ts';

  interface Props {
    /** The draft of this form, left out of the list. */
    current: number | null;
    onchoose: (draft: number) => void;
    ondeleteall: () => Promise<void>;
  }

  let { current, onchoose, ondeleteall }: Props = $props();

  let drafts = $state<Issue[]>([]);
  let confirming = $state(false);
  let deleting = $state(false);

  $effect(() => {
    // Read again when the form moves to another draft.
    const skip = current;
    const controller = new AbortController();
    api.get('/issues/drafts', { signal: controller.signal }).then(
      (page) => {
        drafts = page.items.filter((draft) => draft.id !== skip);
      },
      () => undefined,
    );
    return () => {
      controller.abort();
    };
  });

  const items = $derived.by((): MenuEntry[] => {
    const others = drafts.filter((draft) => draft.id !== current);
    const context = getTimeContext();
    return [
      others.length === 0
        ? { type: 'action', label: m.noDrafts(), disabled: true, onSelect: () => undefined }
        : {
            type: 'group',
            label: m.drafts(),
            items: others.map((draft) => ({
              type: 'action' as const,
              label: m.draftItem({
                summary: draft.summary.trim() || m.noSummary(),
                time: formatInstant(draft.updated_at, context),
              }),
              onSelect: () => {
                onchoose(draft.id);
              },
            })),
          },
      { type: 'separator' },
      {
        type: 'action',
        label: m.deleteAllDrafts(),
        icon: Trash2,
        danger: true,
        onSelect: () => {
          confirming = true;
        },
      },
    ];
  });

  async function deleteAll(): Promise<void> {
    deleting = true;
    try {
      await ondeleteall();
      drafts = [];
      confirming = false;
    } finally {
      deleting = false;
    }
  }
</script>

<Menu label={m.draftsMenu()} {items} placement={{ side: 'bottom', align: 'end' }}>
  {#snippet trigger(props)}
    <Button icon={FileStack} variant="ghost" {...props}>{m.drafts()}</Button>
  {/snippet}
</Menu>

<Dialog bind:open={confirming} title={m.deleteAllTitle()} size="sm" dismissible={!deleting}>
  <p>{m.deleteAllText()}</p>
  {#snippet footer()}
    <Button
      disabled={deleting}
      onclick={() => {
        confirming = false;
      }}>{common.cancel()}</Button
    >
    <Button variant="danger" loading={deleting} onclick={deleteAll}>{m.deleteAllDrafts()}</Button>
  {/snippet}
</Dialog>
