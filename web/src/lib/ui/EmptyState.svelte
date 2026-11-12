<script lang="ts">
  // What a place shows when it has nothing yet, or cannot show it (UI-036): what will appear here
  // and the main action to take. The base of the not-found, forbidden and error states.
  import type { LucideIcon } from '@lucide/svelte';
  import type { Snippet } from 'svelte';

  interface Props {
    title: string;
    description?: string;
    icon?: LucideIcon;
    /** Heading level: 1 when the state is the whole page, 2 under a page title. */
    level?: 1 | 2 | 3;
    tone?: 'neutral' | 'danger';
    actions?: Snippet;
    children?: Snippet;
  }

  let {
    title,
    description,
    icon: Icon,
    level = 2,
    tone = 'neutral',
    actions,
    children,
  }: Props = $props();
</script>

<div class={['empty', tone]}>
  {#if Icon}
    <div class="icon"><Icon aria-hidden="true" /></div>
  {/if}
  <svelte:element this={`h${String(level)}`} class="title">{title}</svelte:element>
  {#if description}<p class="description">{description}</p>{/if}
  {@render children?.()}
  {#if actions}<div class="actions">{@render actions()}</div>{/if}
</div>

<style>
  .empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--space-2);
    max-width: 32rem;
    margin: 0 auto;
    padding: var(--space-12) var(--space-4);
    text-align: center;
  }

  .icon {
    display: grid;
    place-items: center;
    width: var(--space-12);
    height: var(--space-12);
    margin-bottom: var(--space-2);
    border-radius: var(--radius-lg);
    background: var(--color-surface-sunken);
    color: var(--color-text-muted);
  }

  .danger .icon {
    background: var(--color-danger-bg);
    color: var(--color-danger);
  }

  .icon :global(svg) {
    width: 1.5rem;
    height: 1.5rem;
  }

  .title {
    font-size: var(--text-lg);
  }

  .description {
    color: var(--color-text-muted);
  }

  .actions {
    display: flex;
    flex-wrap: wrap;
    justify-content: center;
    gap: var(--space-2);
    margin-top: var(--space-3);
  }
</style>
