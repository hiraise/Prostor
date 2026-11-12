<script lang="ts">
  // The frame of every project page (PRJ-016): the project's logo, name and key, the favorite
  // star, and the project's sections — overview, issues, boards and, for people who configure it,
  // settings. Issues open the issue list in the project's context; boards list the boards that
  // include the project. Sections scroll sideways on phones.
  import Star from '@lucide/svelte/icons/star';
  import type { Snippet } from 'svelte';
  import { resolve } from '$app/paths';

  import type { Project } from '#lib/api/schema.gen.ts';
  import * as common from '#lib/i18n/messages/common.ts';
  import IconButton from '#lib/ui/IconButton.svelte';
  import Notice from '#lib/ui/Notice.svelte';
  import { toasts, describeError } from '#lib/ui/toast.svelte.ts';

  import * as m from './messages.ts';
  import ProjectLogo from './ProjectLogo.svelte';
  import { canConfigure, projectHref, setFavorite } from './projects.ts';

  interface Props {
    project: Project;
    /** The current address, to mark the current section. */
    pathname: string;
    children: Snippet;
  }

  let { project, pathname, children }: Props = $props();

  const overview = $derived(projectHref.overview(project));
  const boards = $derived(resolve('/(app)/projects/[key]/boards', { key: project.key }));
  const settingsBase = $derived(resolve('/(app)/projects/[key]/settings', { key: project.key }));
  const sections = $derived(
    [
      { href: overview, label: m.overview(), current: pathname === overview, visible: true },
      {
        href: `${resolve('/(app)/issues')}?project=${encodeURIComponent(project.key)}`,
        label: m.issues(),
        current: false,
        visible: true,
      },
      {
        href: boards,
        label: m.boards(),
        current: pathname === boards,
        visible: true,
      },
      {
        href: projectHref.settings(project),
        label: m.settings(),
        current: pathname === settingsBase || pathname.startsWith(`${settingsBase}/`),
        visible: canConfigure(project),
      },
    ].filter((section) => section.visible),
  );

  let starring = $state(false);

  async function toggleStar(): Promise<void> {
    starring = true;
    try {
      await setFavorite(project.id, !project.favorite);
    } catch (error) {
      const { requestId } = describeError(error);
      toasts.show({
        tone: 'error',
        message: m.starFailed(),
        detail: requestId ? common.requestId({ id: requestId }) : undefined,
      });
    } finally {
      starring = false;
    }
  }
</script>

<div class="frame">
  <div class="identity">
    <ProjectLogo
      projectKey={project.key}
      color={project.logo_color}
      src={project.logo_url ?? null}
      size="md"
    />
    <!-- Hrefs come from resolve(). -->
    <!-- eslint-disable-next-line svelte/no-navigation-without-resolve -->
    <a class="name" href={overview}>{project.name}</a>
    <span class="key">{project.key}</span>
    {#if project.archived_at}<span class="badge">{m.archived()}</span>{/if}
    <IconButton
      icon={Star}
      label={project.favorite ? m.unstar() : m.star()}
      pressed={project.favorite}
      disabled={starring}
      class={['star', { starred: project.favorite }]}
      onclick={toggleStar}
    />
  </div>
  <!-- eslint-disable svelte/no-navigation-without-resolve -->
  <nav aria-label={m.projectNavigation()}>
    <ul>
      {#each sections as section (section.href)}
        <li>
          <a href={section.href} aria-current={section.current ? 'page' : undefined}
            >{section.label}</a
          >
        </li>
      {/each}
    </ul>
  </nav>
  <!-- eslint-enable svelte/no-navigation-without-resolve -->
</div>

{#if project.archived_at}
  <div class="archived"><Notice tone="info">{m.archivedNotice()}</Notice></div>
{/if}

{@render children()}

<style>
  .frame {
    padding: var(--space-3) var(--space-6) 0;
    border-bottom: 1px solid var(--color-border);
  }

  .identity {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-2);
    min-width: 0;
  }

  /* As tall as a control, so it is easy to hit with a finger (D-019). */
  .name {
    display: inline-flex;
    align-items: center;
    min-width: var(--control-md);
    min-height: var(--control-md);
    overflow: hidden;
    color: var(--color-text);
    font-weight: var(--weight-semibold);
    text-decoration: none;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .name:hover {
    text-decoration: underline;
  }

  .key {
    color: var(--color-text-muted);
    font-family: var(--font-mono);
    font-size: var(--text-sm);
  }

  .badge {
    padding: 0 var(--space-1_5);
    border: 1px solid var(--color-border-control);
    border-radius: var(--radius-sm);
    color: var(--color-text-muted);
    font-size: var(--text-xs);
  }

  .identity :global(.star.starred svg) {
    fill: var(--color-brand);
    stroke: var(--color-text);
  }

  nav {
    margin-top: var(--space-2);
    overflow-x: auto;
  }

  ul {
    display: flex;
    gap: var(--space-1);
    margin: 0;
    padding: 0;
    list-style: none;
  }

  nav a {
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

  nav a:hover {
    color: var(--color-text);
  }

  /* The current section: a saffron bar, the text color and aria-current, never color alone. */
  nav a[aria-current='page'] {
    border-bottom-color: var(--color-brand);
    color: var(--color-text);
    font-weight: var(--weight-semibold);
  }

  .archived {
    padding: var(--space-3) var(--space-6) 0;
  }

  @media (width < 48rem) {
    .frame {
      padding: var(--space-2) var(--space-4) 0;
    }

    .archived {
      padding: var(--space-3) var(--space-4) 0;
    }
  }
</style>
