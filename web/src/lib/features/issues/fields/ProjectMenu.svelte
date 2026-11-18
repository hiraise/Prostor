<script lang="ts">
  // Moving the issue to another project right from the panel's project row (ISS-020, ISS-025):
  // the value opens a searchable dropdown of the projects that would take the issue — active ones
  // where the viewer may create issues. A move that changes nothing happens at once; one that
  // erases fields or hides the issue from the viewer opens the move dialog (`onconfirm`) with the
  // preview of what happens.
  import { goto } from '$app/navigation';
  import { resolve } from '$app/paths';
  import ChevronDown from '@lucide/svelte/icons/chevron-down';

  import type { Issue, Project } from '#lib/api/schema.gen.ts';
  import { refusalText } from '#lib/features/fields/index.ts';
  import { findProjects, ProjectLogo } from '#lib/features/projects/index.ts';
  import { can } from '#lib/session/index.ts';
  import OptionList from '#lib/ui/OptionList.svelte';
  import Popover from '#lib/ui/Popover.svelte';
  import { describeError, toasts } from '#lib/ui/toast.svelte.ts';

  import { forgetIssue, moveIssue, previewMove } from '../actions.ts';
  import { issueHref } from '../issues.ts';
  import * as m from '../messages.ts';

  let { issue, onconfirm }: { issue: Issue; onconfirm: (project: Project) => void } = $props();

  let open = $state(false);
  let query = $state('');
  let projects = $state<Project[]>([]);
  let loading = $state(false);
  let problem = $state<string | null>(null);
  let busy = $state(false);

  $effect(() => {
    if (!open) {
      query = '';
      problem = null;
      return;
    }
    const text = query;
    const timer = setTimeout(
      () => {
        loading = true;
        findProjects(text).then(
          (page) => {
            projects = page.items.filter(
              (project) =>
                project.id !== issue.project.id &&
                project.archived_at === null &&
                can('issue.create', project.id),
            );
            loading = false;
          },
          (error: unknown) => {
            problem = describeError(error).message;
            loading = false;
          },
        );
      },
      text ? 250 : 0,
    );
    return () => clearTimeout(timer);
  });

  async function choose(project: Project): Promise<void> {
    if (busy) return;
    busy = true;
    problem = null;
    try {
      const preview = await previewMove(issue.id, project.id);
      if (preview.changes.length > 0 || preview.loses_access) {
        open = false;
        onconfirm(project);
        return;
      }
      // Nothing changes: the move happens here and now (ISS-025).
      const result = await moveIssue(issue.id, project.id);
      open = false;
      if (result.issue?.readable_id) {
        toasts.show({ tone: 'success', message: m.moved({ id: result.issue.readable_id }) });
        // issueHref resolves the address.
        // eslint-disable-next-line svelte/no-navigation-without-resolve
        await goto(issueHref(result.issue), { replaceState: true });
      } else {
        toasts.show({ tone: 'success', message: m.movedAway() });
        await goto(resolve('/(app)/projects'));
        forgetIssue(issue.id);
      }
    } catch (error) {
      problem = refusalText(error);
    } finally {
      busy = false;
    }
  }
</script>

<Popover label={m.changeProjectHint()} bind:open>
  {#snippet trigger(props)}
    <button type="button" class="pick" {...props}>
      <ProjectLogo
        projectKey={issue.project.key}
        color={issue.project.logo_color}
        src={issue.project.logo_url}
        size="sm"
      />
      <span class="name">{issue.project.name}</span>
      <ChevronDown size={16} aria-hidden="true" />
    </button>
  {/snippet}
  <div class="menu">
    <OptionList
      label={m.changeProjectHint()}
      options={projects}
      key={(project: Project) => project.id}
      checked={() => false}
      onchange={(project: Project) => void choose(project)}
      bind:query
      searchLabel={m.findProject()}
      {loading}
      problem={problem ?? (!loading && projects.length === 0 && !query ? m.noProjects() : null)}
    >
      {#snippet option(project: Project)}
        <span class="option">
          <ProjectLogo
            projectKey={project.key}
            color={project.logo_color}
            src={project.logo_url}
            size="sm"
          />
          <span class="name">{project.name}</span>
          <span class="key">{project.key}</span>
        </span>
      {/snippet}
    </OptionList>
  </div>
</Popover>

<style>
  /* Looks like the panel's other value buttons: quiet, full width of the value column. */
  .pick {
    display: flex;
    align-items: center;
    gap: var(--space-1_5);
    align-self: flex-start;
    width: 100%;
    min-height: var(--control-md);
    margin-inline: calc(-1 * var(--space-2));
    padding: var(--space-1) var(--space-2);
    border: 0;
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--color-link);
    font: inherit;
    text-align: start;
    cursor: pointer;
  }

  .pick:hover,
  .pick[aria-expanded='true'] {
    background: var(--color-hover);
  }

  .pick .name {
    flex: 1 1 0;
    min-width: 0;
    overflow-wrap: anywhere;
  }

  .pick :global(svg:last-child) {
    flex: none;
    color: var(--color-text-muted);
  }

  .menu {
    min-width: 16rem;
  }

  .option {
    display: flex;
    align-items: center;
    gap: var(--space-1_5);
    min-width: 0;
  }

  .option .name {
    flex: 1 1 0;
    min-width: 0;
  }

  .option .key {
    flex: none;
    color: var(--color-text-muted);
    font-family: var(--font-mono);
    font-size: var(--text-sm);
  }
</style>
