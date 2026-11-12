<script lang="ts">
  // The section links of the main navigation. The current section is marked by a saffron bar, a
  // bolder label and `aria-current`, never by color alone (UI-032). Collapsed, the labels stay in
  // the accessible name and show as tooltips.
  import { tooltip } from '#lib/ui/tooltip.ts';

  import { linkState, visibleSections } from './sections.ts';

  interface Props {
    pathname: string;
    collapsed?: boolean;
    onnavigate?: () => void;
  }

  let { pathname, collapsed = false, onnavigate }: Props = $props();
</script>

<!-- Section hrefs come from resolve() in sections.ts. -->
<!-- eslint-disable svelte/no-navigation-without-resolve -->
<ul class={['nav', { collapsed }]}>
  {#each visibleSections() as section (section.id)}
    {@const current = linkState(section.href, pathname)}
    <li>
      <a
        href={section.href}
        class="link"
        aria-current={current}
        onclick={onnavigate}
        {@attach collapsed && tooltip(section.label(), { side: 'right', align: 'center' })}
      >
        <section.icon aria-hidden="true" />
        <span class={{ 'sr-only': collapsed }}>{section.label()}</span>
      </a>
    </li>
  {/each}
</ul>

<!-- eslint-enable svelte/no-navigation-without-resolve -->

<style>
  .nav {
    display: flex;
    flex-direction: column;
    gap: var(--space-0_5);
    margin: 0;
    padding: 0 var(--space-2);
    list-style: none;
  }

  .link {
    position: relative;
    display: flex;
    align-items: center;
    gap: var(--space-3);
    min-height: var(--control-md);
    padding: 0 var(--space-3);
    border-radius: var(--radius-md);
    color: var(--color-text-muted);
    text-decoration: none;
    white-space: nowrap;
    transition:
      background-color var(--duration-fast) var(--ease-out),
      color var(--duration-fast) var(--ease-out);
  }

  .link :global(svg) {
    width: var(--icon-lg);
    height: var(--icon-lg);
  }

  .link:hover {
    background: var(--color-hover);
    color: var(--color-text);
  }

  .link[aria-current] {
    background: var(--color-selected);
    color: var(--color-text);
    font-weight: var(--weight-semibold);
  }

  .link[aria-current]::before {
    content: '';
    position: absolute;
    inset-block: var(--space-1_5);
    inset-inline-start: calc(-1 * var(--space-2));
    width: 3px;
    border-radius: 0 var(--radius-full) var(--radius-full) 0;
    background: var(--color-brand);
  }

  .collapsed .link {
    justify-content: center;
    padding: 0;
  }

  @media (forced-colors: active) {
    .link[aria-current] {
      outline: 2px solid LinkText;
    }
  }
</style>
