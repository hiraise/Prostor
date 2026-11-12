<script lang="ts">
  // The pages of a project's settings (PRJ-037): general, people and fields now; notifications,
  // time tracking, automation and integrations join with their features. A list beside the page on
  // wide screens, a row of tabs on narrow ones.
  import { resolve } from '$app/paths';

  import type { Project } from '#lib/api/schema.gen.ts';

  import * as m from './messages.ts';
  import { canConfigure } from './projects.ts';

  let { project, pathname }: { project: Project; pathname: string } = $props();

  // People who only read the project in full see the people page alone.
  const pages = $derived(
    [
      {
        href: resolve('/(app)/projects/[key]/settings/general', { key: project.key }),
        label: m.general(),
        configure: true,
      },
      {
        href: resolve('/(app)/projects/[key]/settings/people', { key: project.key }),
        label: m.people(),
        configure: false,
      },
      {
        href: resolve('/(app)/projects/[key]/settings/fields', { key: project.key }),
        label: m.fields(),
        configure: true,
      },
      {
        href: resolve('/(app)/projects/[key]/settings/rules', { key: project.key }),
        label: m.rules(),
        configure: true,
      },
    ].filter((page) => !page.configure || canConfigure(project)),
  );
</script>

<!-- Hrefs come from resolve() above. -->
<!-- eslint-disable svelte/no-navigation-without-resolve -->
<nav class="settings-nav" aria-label={m.settingsNavigation()}>
  <ul>
    {#each pages as page (page.href)}
      <li>
        <a href={page.href} aria-current={pathname === page.href ? 'page' : undefined}
          >{page.label}</a
        >
      </li>
    {/each}
  </ul>
</nav>

<!-- eslint-enable svelte/no-navigation-without-resolve -->

<style>
  .settings-nav {
    flex: none;
    width: 12rem;
    padding: var(--space-4) 0 var(--space-4) var(--space-6);
  }

  ul {
    display: flex;
    flex-direction: column;
    gap: var(--space-0_5);
    margin: 0;
    padding: 0;
    list-style: none;
  }

  a {
    display: flex;
    align-items: center;
    min-height: var(--control-md);
    padding: 0 var(--space-3);
    border-left: 2px solid transparent;
    border-radius: 0 var(--radius-md) var(--radius-md) 0;
    color: var(--color-text-muted);
    font-weight: var(--weight-medium);
    text-decoration: none;
  }

  a:hover {
    background: var(--color-hover);
    color: var(--color-text);
  }

  a[aria-current='page'] {
    border-left-color: var(--color-brand);
    background: var(--color-selected);
    color: var(--color-text);
    font-weight: var(--weight-semibold);
  }

  @media (width < 48rem) {
    .settings-nav {
      width: auto;
      padding: var(--space-2) var(--space-4) 0;
      overflow-x: auto;
    }

    ul {
      flex-direction: row;
    }

    a {
      border-bottom: 2px solid transparent;
      border-left: none;
      border-radius: 0;
      white-space: nowrap;
    }

    a[aria-current='page'] {
      border-bottom-color: var(--color-brand);
      background: none;
    }
  }
</style>
