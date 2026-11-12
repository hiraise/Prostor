<script lang="ts">
  // Moving an issue to another project (ISS-020, ISS-025): first the project — active ones where
  // the viewer may create issues (archived ones are never offered, PRJ-061) — then what the move
  // does to the fields, by outcome, and a warning when the viewer would lose the issue. The page
  // follows the issue to its new ID, or leaves for the projects when it is out of sight.
  import { goto } from '$app/navigation';
  import { resolve } from '$app/paths';

  import type { ChangeValues, Issue, MovePreview, Project } from '#lib/api/schema.gen.ts';
  import { groupName, personName } from '#lib/features/access/index.ts';
  import { refusalText } from '#lib/features/fields/index.ts';
  import { findProjects, ProjectLogo } from '#lib/features/projects/index.ts';
  import * as common from '#lib/i18n/messages/common.ts';
  import { can } from '#lib/session/index.ts';
  import Button from '#lib/ui/Button.svelte';
  import Dialog from '#lib/ui/Dialog.svelte';
  import Notice from '#lib/ui/Notice.svelte';
  import OptionList from '#lib/ui/OptionList.svelte';
  import { describeError, toasts } from '#lib/ui/toast.svelte.ts';

  import { forgetIssue, moveIssue, previewMove } from '../actions.ts';
  import { issueHref } from '../issues.ts';
  import * as m from '../messages.ts';

  let { open = $bindable(false), issue }: { open?: boolean; issue: Issue } = $props();

  let query = $state('');
  let projects = $state<Project[]>([]);
  let loading = $state(false);
  let problem = $state<string | null>(null);
  let target = $state<Project | null>(null);
  let preview = $state<MovePreview | null>(null);
  let busy = $state(false);

  $effect(() => {
    if (!open) {
      target = null;
      preview = null;
      query = '';
      problem = null;
      return;
    }
    if (target) return;
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
    return () => {
      clearTimeout(timer);
    };
  });

  async function choose(project: Project): Promise<void> {
    target = project;
    preview = null;
    problem = null;
    try {
      preview = await previewMove(issue.id, project.id);
    } catch (error) {
      problem = refusalText(error);
    }
  }

  function words(values: ChangeValues): string {
    const parts = [
      ...values.values.map((value) => value.name),
      ...values.users.map(personName),
      ...values.groups.map(groupName),
      ...values.texts,
      ...values.numbers.map(String),
    ];
    return parts.length > 0 ? parts.join(', ') : m.emptyValue();
  }

  async function confirm(): Promise<void> {
    if (!target) return;
    busy = true;
    problem = null;
    try {
      const moved = issue.id;
      const result = await moveIssue(moved, target.id);
      open = false;
      if (result.issue?.readable_id) {
        toasts.show({ tone: 'success', message: m.moved({ id: result.issue.readable_id }) });
        // issueHref resolves the address.
        // eslint-disable-next-line svelte/no-navigation-without-resolve
        await goto(issueHref(result.issue), { replaceState: true });
      } else {
        toasts.show({ tone: 'success', message: m.movedAway() });
        await goto(resolve('/(app)/projects'));
        forgetIssue(moved);
      }
    } catch (error) {
      problem = refusalText(error);
    } finally {
      busy = false;
    }
  }
</script>

<Dialog bind:open title={m.moveTitle()} size="md" dismissible={!busy}>
  <div class="body">
    {#if problem}<Notice tone="danger">{problem}</Notice>{/if}
    {#if !target}
      <OptionList
        label={m.targetProject()}
        options={projects}
        key={(project: Project) => project.id}
        checked={() => false}
        onchange={(project: Project) => void choose(project)}
        bind:query
        searchLabel={m.findProject()}
        {loading}
        problem={!loading && projects.length === 0 && !query ? m.noProjects() : null}
      >
        {#snippet option(project: Project)}
          <span class="project">
            <ProjectLogo
              projectKey={project.key}
              color={project.logo_color}
              src={project.logo_url}
              size="sm"
            />
            {project.name}
          </span>
        {/snippet}
      </OptionList>
    {:else if preview}
      {#if preview.loses_access}
        <Notice tone="warning">{m.losesAccess()}</Notice>
      {/if}
      {#if preview.changes.length === 0}
        <p>{m.moveNoChanges()}</p>
      {:else}
        <p>{m.moveChanges({ name: preview.target.name })}</p>
        <ul class="changes">
          {#each preview.changes as change (change.field.id)}
            <li>
              <span class="name">{change.field.name}</span>
              {change.outcome === 'erased' ? m.outcomeErased() : m.outcomeReplaced()}:
              <span class="before">{words(change.before)}</span>
              → <span>{words(change.after)}</span>
            </li>
          {/each}
        </ul>
      {/if}
    {:else if !problem}
      <p>{common.loading()}</p>
    {/if}
  </div>
  {#snippet footer()}
    {#if target}
      <Button
        disabled={busy}
        onclick={() => {
          target = null;
          preview = null;
          problem = null;
        }}>{m.back()}</Button
      >
      <Button variant="primary" loading={busy} disabled={!preview} onclick={confirm}
        >{m.move()}</Button
      >
    {:else}
      <Button
        onclick={() => {
          open = false;
        }}>{common.cancel()}</Button
      >
    {/if}
  {/snippet}
</Dialog>

<style>
  .body {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }

  .project {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
  }

  .changes {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    margin: 0;
    padding-inline-start: var(--space-5);
  }

  .name {
    font-weight: var(--weight-medium);
  }

  .before {
    color: var(--color-text-muted);
    text-decoration: line-through;
  }
</style>
