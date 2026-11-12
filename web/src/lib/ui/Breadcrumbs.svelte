<script lang="ts" module>
  export interface Crumb {
    label: string;
    href: string;
  }
</script>

<script lang="ts">
  // Where a page sits (UI-005): the trail of pages above it on wide screens, a single link back to
  // the nearest one on phones.
  import ChevronLeft from '@lucide/svelte/icons/chevron-left';
  import ChevronRight from '@lucide/svelte/icons/chevron-right';

  interface Props {
    /** The trail, outermost first; the page itself is not in it. */
    items: Crumb[];
    /** The phone's back link; default: the last item. */
    back?: Crumb;
    label: string;
  }

  let { items, back, label }: Props = $props();

  const up = $derived(back ?? items.at(-1));
</script>

<!-- Hrefs come resolved from the caller. -->
<!-- eslint-disable svelte/no-navigation-without-resolve -->
<nav class="crumbs" aria-label={label}>
  <ol class="trail">
    {#each items as item, index (index)}
      <li>
        {#if index > 0}<ChevronRight size={14} aria-hidden="true" />{/if}
        <a href={item.href}>{item.label}</a>
      </li>
    {/each}
  </ol>
  {#if up}
    <a class="back" href={up.href}><ChevronLeft size={16} aria-hidden="true" />{up.label}</a>
  {/if}
</nav>

<!-- eslint-enable svelte/no-navigation-without-resolve -->

<style>
  .crumbs {
    font-size: var(--text-sm);
  }

  .trail {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-1);
    margin: 0;
    padding: 0;
    list-style: none;
  }

  li {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
    color: var(--color-text-muted);
  }

  a {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-width: var(--control-md);
    min-height: var(--control-md);
    color: var(--color-link);
  }

  .back {
    display: none;
  }

  @media (width < 48rem) {
    .trail {
      display: none;
    }

    .back {
      display: inline-flex;
      gap: var(--space-0_5);
      min-height: var(--control-md);
    }
  }
</style>
