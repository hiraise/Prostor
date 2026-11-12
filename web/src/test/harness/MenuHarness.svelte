<script lang="ts">
  // A menu with actions and a radio group, as the shell and pages use it.
  import Settings2 from '@lucide/svelte/icons/settings-2';

  import IconButton from '#lib/ui/IconButton.svelte';
  import Menu, { type MenuEntry } from '#lib/ui/Menu.svelte';

  let { onaction, late = false }: { onaction: (name: string) => void; late?: boolean } = $props();
  let size = $state('medium');

  const items: MenuEntry[] = $derived([
    {
      type: 'action',
      label: 'Copy link',
      onSelect: () => {
        onaction('copy');
      },
    },
    // Arrives late (the clone capability of the issue page), sliding the entries below it.
    ...(late
      ? [
          {
            type: 'action' as const,
            label: 'Clone',
            onSelect: () => {
              onaction('clone');
            },
          },
        ]
      : []),
    {
      type: 'action',
      label: 'Archive',
      disabled: true,
      onSelect: () => {
        onaction('archive');
      },
    },
    {
      type: 'action',
      label: 'Delete',
      danger: true,
      onSelect: () => {
        onaction('delete');
      },
    },
    { type: 'separator' },
    {
      type: 'group',
      label: 'Size',
      items: ['small', 'medium', 'large'].map((value) => ({
        type: 'radio' as const,
        label: value,
        checked: size === value,
        onSelect: () => {
          size = value;
        },
      })),
    },
  ]);
</script>

<button type="button">Before</button>
<Menu label="Project actions" {items}>
  {#snippet trigger(props)}
    <IconButton icon={Settings2} label="Project actions" {...props} />
  {/snippet}
</Menu>
<p data-testid="size">{size}</p>
