<script lang="ts">
  // A project's overview (PRJ-014): its description (rendered Markdown whose checklist people who
  // configure the project can tick, PRJ-006, CNT-041), owner, creation date, team, former keys and
  // template, with links to its settings for people who configure it. The header has the page's
  // only h1 and the project's actions (PRJ-011): settings, archive or restore (PRJ-061, PRJ-063)
  // and delete (PRJ-064). The frame around it shows the logo, key and favorite star. Issue
  // counts, boards and widgets join with their features.
  import Archive from '@lucide/svelte/icons/archive';
  import ArchiveRestore from '@lucide/svelte/icons/archive-restore';
  import Ellipsis from '@lucide/svelte/icons/ellipsis';
  import Plus from '@lucide/svelte/icons/plus';
  import Settings2 from '@lucide/svelte/icons/settings-2';
  import Trash2 from '@lucide/svelte/icons/trash-2';

  import type { Project } from '#lib/api/schema.gen.ts';
  import { groupName, PersonName } from '#lib/features/access/index.ts';
  import { MarkdownView, toggleTask } from '#lib/features/content/index.ts';
  import { ConflictError } from '#lib/store/index.ts';
  import { can } from '#lib/session/index.ts';
  import Time from '#lib/time/Time.svelte';
  import Button from '#lib/ui/Button.svelte';
  import IconButton from '#lib/ui/IconButton.svelte';
  import Menu, { type MenuEntry } from '#lib/ui/Menu.svelte';
  import PageHeader from '#lib/ui/PageHeader.svelte';
  import { showError, toasts } from '#lib/ui/toast.svelte.ts';

  import ArchiveDialog from '../ArchiveDialog.svelte';
  import DeleteProjectDialog from '../DeleteProjectDialog.svelte';
  import * as shared from '../messages.ts';
  import {
    canConfigure,
    canDelete,
    canSeePeople,
    projectHref,
    restoreProject,
    updateProject,
  } from '../projects.ts';
  import * as m from './messages.ts';

  interface Props {
    project: Project;
    /** The project is being deleted; the route leaves for the list. */
    ondeleted: () => unknown;
    /** «Новая задача» in this project (ISS-005); the route opens the form. */
    oncreateissue?: () => unknown;
  }

  let { project, ondeleted, oncreateissue }: Props = $props();

  const id = $props.id();
  let archiving = $state(false);
  let deleting = $state(false);
  let restoring = $state(false);

  const configure = $derived(canConfigure(project));
  const archived = $derived(project.archived_at !== null);
  const menu = $derived.by((): MenuEntry[] => {
    const items: MenuEntry[] = [];
    if (configure && !archived) {
      items.push({
        type: 'action',
        label: shared.archiveProject(),
        icon: Archive,
        onSelect: () => (archiving = true),
      });
    }
    if (canDelete(project)) {
      items.push({
        type: 'action',
        label: shared.deleteProject(),
        icon: Trash2,
        danger: true,
        onSelect: () => (deleting = true),
      });
    }
    return items;
  });

  /** Ticks or clears a checklist box of the description (CNT-041); a failure puts it back. */
  async function toggle(line: number, checked: boolean): Promise<void> {
    const description = toggleTask(project.description, line, checked);
    if (description === null) {
      // The text changed under the box: the overview shows the new one.
      toasts.show({ message: m.descriptionChanged() });
      throw new Error('The checklist item is gone');
    }
    try {
      await updateProject(project.id, { description });
    } catch (error) {
      if (error instanceof ConflictError) toasts.show({ message: m.descriptionChanged() });
      else showError(error);
      throw error;
    }
  }

  async function restore(): Promise<void> {
    restoring = true;
    try {
      const restored = await restoreProject(project.id);
      toasts.show({ tone: 'success', message: shared.projectRestored({ name: restored.name }) });
    } catch (error) {
      showError(error, restore);
    } finally {
      restoring = false;
    }
  }
</script>

<svelte:head><title>{project.name} · TaskMaster</title></svelte:head>

<PageHeader title={project.name}>
  {#snippet actions()}
    {#if oncreateissue && !archived && can('issue.create', project.id)}
      <Button icon={Plus} variant="primary" onclick={oncreateissue}>{m.newIssue()}</Button>
    {/if}
    {#if configure && archived}
      <Button icon={ArchiveRestore} loading={restoring} onclick={restore}>
        {shared.restoreProject()}
      </Button>
    {:else if configure}
      <Button icon={Settings2} href={projectHref.settings(project)}>{shared.settings()}</Button>
    {/if}
    {#if menu.length > 0}
      <Menu
        label={shared.projectActions({ name: project.name })}
        items={menu}
        placement={{ side: 'bottom', align: 'end' }}
      >
        {#snippet trigger(props)}
          <IconButton
            icon={Ellipsis}
            variant="secondary"
            label={shared.projectActions({ name: project.name })}
            {...props}
          />
        {/snippet}
      </Menu>
    {/if}
  {/snippet}
</PageHeader>

<div class="page">
  <section class="section description-section" aria-labelledby="{id}-description">
    <h2 id="{id}-description">{shared.descriptionLabel()}</h2>
    {#if project.description}
      <MarkdownView
        class="description"
        html={project.description_html}
        headingLevel={3}
        ontoggle={configure && !archived ? toggle : undefined}
      />
    {:else}
      <p class="muted">
        {m.noDescription()}
        {#if configure && !archived}
          <!-- eslint-disable-next-line svelte/no-navigation-without-resolve -->
          <a href={projectHref.settings(project)}>{m.addDescription()}</a>
        {/if}
      </p>
    {/if}
  </section>

  {#if configure}
    <section class="section settings" aria-labelledby="{id}-settings">
      <h2 id="{id}-settings">{m.settingsLinks()}</h2>
      <!-- Hrefs come from projectHref, which resolves them. -->
      <!-- eslint-disable svelte/no-navigation-without-resolve -->
      <ul class="links">
        <li>
          <a href={projectHref.settings(project)}>{m.generalLink()}</a>
          <span class="muted">{m.generalText()}</span>
        </li>
        <li>
          <a href={projectHref.people(project)}>{m.peopleLink()}</a>
          <span class="muted">{m.peopleText()}</span>
        </li>
        <li>
          <a href={projectHref.fields(project)}>{m.fieldsLink()}</a>
          <span class="muted">{m.fieldsText()}</span>
        </li>
      </ul>
      <!-- eslint-enable svelte/no-navigation-without-resolve -->
    </section>
  {/if}

  <section class="section details" aria-labelledby="{id}-details">
    <h2 id="{id}-details">{m.details()}</h2>
    <dl>
      <div>
        <dt>{shared.keyLabel()}</dt>
        <dd class="mono">{project.key}</dd>
      </div>
      {#if project.former_keys.length > 0}
        <div>
          <dt>{shared.formerKeys()}</dt>
          <dd>
            <span class="mono">{project.former_keys.join(', ')}</span>
            <span class="note">{m.formerKeysNote()}</span>
          </dd>
        </div>
      {/if}
      <div>
        <dt>{shared.owner()}</dt>
        <dd>
          {#if project.owner}<PersonName person={project.owner} />{:else}{m.noOwner()}{/if}
        </dd>
      </div>
      <div>
        <dt>{shared.created()}</dt>
        <dd><Time value={project.created_at} kind="date" year="always" /></dd>
      </div>
      <div>
        <dt>{shared.team()}</dt>
        <dd>
          <span class="count">{shared.teamSize({ count: project.team_size })}</span>
          {#if canSeePeople(project)}
            <!-- eslint-disable-next-line svelte/no-navigation-without-resolve -->
            <a href={projectHref.people(project)}>{m.peopleAndRoles()}</a>
          {/if}
        </dd>
      </div>
      <div>
        <dt>{m.template()}</dt>
        <dd>{shared.templateNames[project.template]()}</dd>
      </div>
      {#if project.default_audience}
        <div>
          <dt>{m.audience()}</dt>
          <dd>{groupName(project.default_audience)}</dd>
        </div>
      {/if}
    </dl>
  </section>
</div>

{#if configure}
  <ArchiveDialog bind:open={archiving} {project} />
{/if}
{#if canDelete(project)}
  <DeleteProjectDialog bind:open={deleting} {project} {ondeleted} />
{/if}

<style>
  /* Wide screens: the description and the settings beside the details; narrow ones: one column
     with the details between them. */
  .page {
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(16rem, 22rem);
    grid-template-rows: auto 1fr;
    grid-template-areas:
      'description details'
      'settings details';
    align-items: start;
    gap: var(--space-8);
    max-width: var(--content-max);
    padding: var(--space-2) var(--space-6) var(--space-12);
  }

  .description-section {
    grid-area: description;
    min-width: 0;
  }

  .settings {
    grid-area: settings;
    min-width: 0;
  }

  .details {
    grid-area: details;
  }

  .section {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }

  h2 {
    font-size: var(--text-lg);
  }

  .description-section :global(.description) {
    max-width: 44rem;
  }

  .muted {
    color: var(--color-text-muted);
  }

  a {
    color: var(--color-link);
  }

  .links {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .links li {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0 var(--space-3);
  }

  .links a,
  dd a {
    display: inline-flex;
    align-items: center;
    min-width: var(--control-md);
    min-height: var(--control-md);
    font-weight: var(--weight-medium);
  }

  .links .muted {
    font-size: var(--text-sm);
  }

  .muted a {
    margin-inline-start: var(--space-1);
  }

  dl {
    display: flex;
    flex-direction: column;
    margin: 0;
    padding: var(--space-1) var(--space-4);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-lg);
    background: var(--color-surface);
  }

  dl > div {
    display: flex;
    flex-direction: column;
    gap: var(--space-0_5);
    padding: var(--space-2) 0;
  }

  dl > div + div {
    border-top: 1px solid var(--color-border);
  }

  dt {
    color: var(--color-text-muted);
    font-size: var(--text-sm);
  }

  dd {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-1) var(--space-3);
    margin: 0;
    min-height: var(--control-sm);
    overflow-wrap: anywhere;
  }

  .mono {
    font-family: var(--font-mono);
  }

  .note {
    color: var(--color-text-muted);
    font-size: var(--text-sm);
  }

  .count {
    font-variant-numeric: tabular-nums;
  }

  @media (width < 64rem) {
    .page {
      grid-template-columns: minmax(0, 1fr);
      grid-template-rows: auto;
      grid-template-areas:
        'description'
        'details'
        'settings';
      gap: var(--space-6);
    }
  }

  @media (width < 48rem) {
    .page {
      padding: var(--space-2) var(--space-4) var(--space-8);
    }
  }
</style>
