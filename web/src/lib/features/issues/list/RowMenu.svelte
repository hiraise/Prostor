<script lang="ts">
  // The row's menu in the issue list (AUT-066, §3.13): cloning when the project allows it. The
  // button appears once the project's `clone_issue` toggle is known — the row's hover or focus
  // asks for it — and its items wait while a clone of this issue is on its way.
  import Ellipsis from '@lucide/svelte/icons/ellipsis';
  import FileStack from '@lucide/svelte/icons/file-stack';
  import Files from '@lucide/svelte/icons/files';

  import type { Issue } from '#lib/api/schema.gen.ts';
  import { can } from '#lib/session/index.ts';
  import IconButton from '#lib/ui/IconButton.svelte';
  import Menu, { type MenuEntry } from '#lib/ui/Menu.svelte';

  import { cloneAsDrafts, cloneIssue, cloning, cloningKnown } from '../clone.svelte.ts';
  import * as m from '../messages.ts';

  let { issue }: { issue: Issue } = $props();

  const name = $derived(issue.readable_id ?? String(issue.id));
  const offered = $derived(
    !issue.draft && cloningKnown(issue.project.id) && can('issue.create', issue.project.id),
  );

  const items = $derived.by((): MenuEntry[] => {
    if (!offered) return [];
    const busy = cloning(issue.id);
    return [
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
    ];
  });
</script>

{#if offered}
  <Menu label={m.rowActions({ id: name })} {items} placement={{ side: 'bottom', align: 'end' }}>
    {#snippet trigger(props)}
      <IconButton icon={Ellipsis} label={m.rowActions({ id: name })} size="sm" {...props} />
    {/snippet}
  </Menu>
{/if}
