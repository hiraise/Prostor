<script lang="ts">
  // The pages of the administration area the person may open (ADM-001): Access, and the fields and
  // value sets of the instance (FLD-025), its link types (LNK-018); Integrations and Server pages join as their features
  // arrive. Tabs scroll sideways on phones.
  import { resolve } from '$app/paths';

  import {
    canSeeCapabilities,
    canSeeDirectory,
    canSeeFields,
    canSeeGroups,
    canSeeJobs,
    canSeeLinkTypes,
    canSeeMail,
    canSeeRoles,
    canSeeSettings,
  } from '#lib/session/index.ts';

  import * as m from './messages.ts';

  let { pathname }: { pathname: string } = $props();

  const pages = $derived(
    [
      { href: resolve('/(app)/admin/users'), label: m.users(), visible: canSeeDirectory() },
      { href: resolve('/(app)/admin/groups'), label: m.groups(), visible: canSeeGroups() },
      { href: resolve('/(app)/admin/roles'), label: m.roles(), visible: canSeeRoles() },
      { href: resolve('/(app)/admin/fields'), label: m.fields(), visible: canSeeFields() },
      {
        href: resolve('/(app)/admin/value-sets'),
        label: m.valueSets(),
        visible: canSeeFields(),
      },
      {
        href: resolve('/(app)/admin/link-types'),
        label: m.linkTypes(),
        visible: canSeeLinkTypes(),
      },
      {
        href: resolve('/(app)/admin/mail'),
        label: m.mail(),
        visible: canSeeMail(),
      },
      {
        href: resolve('/(app)/admin/settings'),
        label: m.settings(),
        visible: canSeeSettings(),
      },
      {
        href: resolve('/(app)/admin/capabilities'),
        label: m.capabilities(),
        visible: canSeeCapabilities(),
      },
      {
        href: resolve('/(app)/admin/jobs'),
        label: m.jobs(),
        visible: canSeeJobs(),
      },
    ].filter((page) => page.visible),
  );
</script>

<!-- Hrefs come from resolve() above. -->
<!-- eslint-disable svelte/no-navigation-without-resolve -->
<nav class="admin-nav" aria-label={m.administration()}>
  <ul>
    {#each pages as page (page.href)}
      {@const current = pathname === page.href || pathname.startsWith(`${page.href}/`)}
      <li>
        <a href={page.href} aria-current={current ? 'page' : undefined}>{page.label}</a>
      </li>
    {/each}
  </ul>
</nav>

<!-- eslint-enable svelte/no-navigation-without-resolve -->

<style>
  .admin-nav {
    padding: var(--space-3) var(--space-6) 0;
    overflow-x: auto;
    border-bottom: 1px solid var(--color-border);
  }

  ul {
    display: flex;
    gap: var(--space-1);
    margin: 0;
    padding: 0;
    list-style: none;
  }

  a {
    display: inline-flex;
    align-items: center;
    min-height: var(--control-md);
    padding: 0 var(--space-3);
    border-bottom: 2px solid transparent;
    color: var(--color-text-muted);
    font-weight: var(--weight-medium);
    text-decoration: none;
    white-space: nowrap;
  }

  a:hover {
    color: var(--color-text);
  }

  /* The current page: a saffron bar, the text color and aria-current, never color alone. */
  a[aria-current='page'] {
    border-bottom-color: var(--color-brand);
    color: var(--color-text);
    font-weight: var(--weight-semibold);
  }

  @media (width < 48rem) {
    .admin-nav {
      padding: var(--space-2) var(--space-4) 0;
    }
  }
</style>
