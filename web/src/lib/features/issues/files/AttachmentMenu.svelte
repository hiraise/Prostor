<script lang="ts">
  // The menu of a file (CNT-020): «Скачать», «Сведения», and for those who may — «Переименовать»,
  // «Видимость», «Удалить». The dialogs belong to the section (AttachmentDialogs).
  import Download from '@lucide/svelte/icons/download';
  import Ellipsis from '@lucide/svelte/icons/ellipsis';
  import Info from '@lucide/svelte/icons/info';
  import Lock from '@lucide/svelte/icons/lock';
  import Pencil from '@lucide/svelte/icons/pencil';
  import Trash2 from '@lucide/svelte/icons/trash-2';

  import type { Attachment } from '#lib/api/schema.gen.ts';
  import IconButton from '#lib/ui/IconButton.svelte';
  import Menu, { type MenuEntry } from '#lib/ui/Menu.svelte';

  import * as m from '../messages.ts';
  import type { FileAction } from './files.svelte.ts';

  interface Props {
    file: Attachment;
    onaction: (kind: FileAction['kind']) => unknown;
  }

  let { file, onaction }: Props = $props();

  /** Saves the file under its name, as a link with `download` does. */
  function save(): void {
    const link = document.createElement('a');
    link.href = file.url;
    link.download = file.name;
    link.click();
  }

  const items = $derived.by((): MenuEntry[] => {
    const entries: MenuEntry[] = [
      { type: 'action', label: m.download(), icon: Download, onSelect: save },
      {
        type: 'action',
        label: m.fileDetails(),
        icon: Info,
        onSelect: () => void onaction('details'),
      },
    ];
    if (file.can.update) {
      entries.push(
        {
          type: 'action',
          label: m.rename(),
          icon: Pencil,
          onSelect: () => void onaction('rename'),
        },
        {
          type: 'action',
          label: m.fileVisibility(),
          icon: Lock,
          onSelect: () => void onaction('visibility'),
        },
      );
    }
    if (file.can.delete) {
      entries.push(
        { type: 'separator' },
        {
          type: 'action',
          label: m.deleteFile(),
          icon: Trash2,
          danger: true,
          onSelect: () => void onaction('delete'),
        },
      );
    }
    return entries;
  });
</script>

<Menu
  label={m.fileActions({ name: file.name })}
  {items}
  placement={{ side: 'bottom', align: 'end' }}
>
  {#snippet trigger(props)}
    <IconButton icon={Ellipsis} label={m.fileActions({ name: file.name })} size="sm" {...props} />
  {/snippet}
</Menu>
