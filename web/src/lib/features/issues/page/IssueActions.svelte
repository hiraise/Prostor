<script lang="ts">
  // «Ещё» of the issue page (ISS-017, P1 part): the copy actions (UI-006), «Прикрепить файлы»,
  // «Клонировать» when the project allows it (AUT-066), and «Удалить» for those who may delete.
  // Items without the right are not shown; «Изменить», «Добавить тег» and «Связать» are icon
  // buttons in the title row on every width and are not repeated here.
  import Copy from '@lucide/svelte/icons/copy';
  import CopyPlus from '@lucide/svelte/icons/copy-plus';
  import Ellipsis from '@lucide/svelte/icons/ellipsis';
  import History from '@lucide/svelte/icons/history';
  import Terminal from '@lucide/svelte/icons/terminal';
  import Files from '@lucide/svelte/icons/files';
  import FileStack from '@lucide/svelte/icons/file-stack';
  import Link from '@lucide/svelte/icons/link';
  import ListTree from '@lucide/svelte/icons/list-tree';
  import SquarePlus from '@lucide/svelte/icons/square-plus';
  import Paperclip from '@lucide/svelte/icons/paperclip';
  import Trash2 from '@lucide/svelte/icons/trash-2';

  import type { Issue } from '#lib/api/schema.gen.ts';
  import * as common from '#lib/i18n/messages/common.ts';
  import { can } from '#lib/session/index.ts';
  import { copyLink, copyText } from '#lib/ui/copy.ts';
  import IconButton from '#lib/ui/IconButton.svelte';
  import Menu, { type MenuEntry } from '#lib/ui/Menu.svelte';

  import { askCloning, cloneAsDrafts, cloneIssue, cloning, cloningKnown } from '../clone.svelte.ts';
  import { issueHref } from '../issues.ts';
  import * as m from '../messages.ts';

  interface Props {
    issue: Issue;
    ondelete: () => unknown;
    /** «Прикрепить файлы», for those who may (CNT-017). */
    onattach?: () => unknown;
    /** «Новая подзадача», «Новая связанная задача…» (ISS-009). */
    onsubtask?: () => unknown;
    onnewlinked?: () => unknown;
    /** «Отметить как дубликат…» (LNK-015). */
    onduplicate?: () => unknown;
    /** «Команды…» (step 2.1 §6, ISS-017). */
    oncommands?: () => unknown;
    /** «Повторить последнюю команду»; absent when nothing was applied in this tab yet. */
    onrepeat?: () => unknown;
  }

  let {
    issue,
    ondelete,
    onattach,
    onsubtask,
    onnewlinked,
    onduplicate,
    oncommands,
    onrepeat,
  }: Props = $props();

  // The project's `clone_issue` toggle (AUT-066), looked up before the menu can open and cached
  // per project in the clone module: the items appear when it lands.
  function wantCloning(): void {
    if (!issue.draft) askCloning(issue.project.id);
  }
  const mayClone = $derived(
    !issue.draft && cloningKnown(issue.project.id) && can('issue.create', issue.project.id),
  );

  const items = $derived.by((): MenuEntry[] => {
    const readable = issue.readable_id;
    const entries: MenuEntry[] = [];
    if (readable) {
      entries.push(
        {
          type: 'action',
          label: m.copyId(),
          icon: Copy,
          onSelect: () => void copyText(readable, m.idCopied({ id: readable })),
        },
        {
          type: 'action',
          label: common.copyLink(),
          icon: Link,
          onSelect: () => void copyLink(issueHref(issue)),
        },
        {
          type: 'action',
          label: m.copyIdAndSummary(),
          icon: Copy,
          onSelect: () => void copyText(`${readable} ${issue.summary}`.trim()),
        },
      );
    }
    if (onattach) {
      entries.push({
        type: 'action',
        label: m.attachFiles(),
        icon: Paperclip,
        onSelect: () => void onattach(),
      });
    }
    const before = entries.length;
    if (oncommands) {
      entries.push({
        type: 'action',
        label: m.commandsMenu(),
        icon: Terminal,
        onSelect: () => void oncommands(),
      });
      if (onrepeat) {
        entries.push({
          type: 'action',
          label: m.repeatMenu(),
          icon: History,
          onSelect: () => void onrepeat(),
        });
      }
    }
    if (onsubtask) {
      entries.push({
        type: 'action',
        label: m.newSubtask(),
        icon: ListTree,
        onSelect: () => void onsubtask(),
      });
    }
    if (onnewlinked) {
      entries.push({
        type: 'action',
        label: m.newLinkedIssue(),
        icon: SquarePlus,
        onSelect: () => void onnewlinked(),
      });
    }
    if (onduplicate) {
      entries.push({
        type: 'action',
        label: m.markDuplicate(),
        icon: CopyPlus,
        onSelect: () => void onduplicate(),
      });
    }
    if (mayClone) {
      const busy = cloning(issue.id);
      entries.push(
        {
          type: 'action',
          label: m.clone(),
          icon: Files,
          disabled: busy,
          onSelect: () => void cloneIssue(issue),
        },
        {
          type: 'action',
          label: m.cloneAsDraft(),
          icon: FileStack,
          disabled: busy,
          onSelect: () => void cloneAsDrafts([issue]),
        },
      );
    }
    if (before > 0 && entries.length > before) entries.splice(before, 0, { type: 'separator' });
    if (issue.can.delete) {
      if (entries.length > 0) entries.push({ type: 'separator' });
      entries.push({
        type: 'action',
        label: m.deleteIssue(),
        icon: Trash2,
        danger: true,
        onSelect: () => void ondelete(),
      });
    }
    return entries;
  });
</script>

{#if items.length > 0}
  <Menu label={m.actions()} {items} placement={{ side: 'bottom', align: 'end' }}>
    {#snippet trigger(props)}
      <IconButton
        icon={Ellipsis}
        variant="secondary"
        label={m.more()}
        onfocus={wantCloning}
        onpointerenter={wantCloning}
        onpointerdown={wantCloning}
        {...props}
      />
    {/snippet}
  </Menu>
{/if}
