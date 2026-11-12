<script lang="ts">
  // The project of a new issue (ISS-005, PRJ-012): a button with the project that opens a search
  // over the active projects where the person may create issues — the context project first,
  // then favorites, then the rest by name. Archived projects are never offered (PRJ-061).
  import ChevronDown from '@lucide/svelte/icons/chevron-down';

  import type { Project, ProjectRef } from '#lib/api/schema.gen.ts';
  import { ProjectLogo } from '#lib/features/projects/index.ts';
  import OptionList from '#lib/ui/OptionList.svelte';
  import Popover from '#lib/ui/Popover.svelte';
  import { describeError } from '#lib/ui/toast.svelte.ts';

  import * as m from '../messages.ts';
  import { creatableProjects, orderProjects } from './projects.ts';

  interface Props {
    value: ProjectRef;
    /** The entry point's project: first in the list. */
    first?: string;
    disabled?: boolean;
    onpick: (project: Project) => void;
  }

  let { value, first, disabled = false, onpick }: Props = $props();

  const id = $props.id();
  let open = $state<boolean>(false);
  let query = $state('');
  let found = $state<Project[]>([]);
  let loading = $state(false);
  let problem = $state<string | null>(null);

  const options = $derived(orderProjects(found, first));

  $effect(() => {
    if (!open) return;
    const text = query;
    const controller = new AbortController();
    const timer = setTimeout(
      () => {
        loading = true;
        problem = null;
        creatableProjects(text, controller.signal).then(
          (projects) => {
            found = projects;
            loading = false;
          },
          (error: unknown) => {
            if (controller.signal.aborted) return;
            problem = describeError(error).message;
            loading = false;
          },
        );
      },
      text ? 250 : 0,
    );
    return () => {
      clearTimeout(timer);
      controller.abort();
    };
  });
</script>

<Popover label={m.chooseProject()} bind:open>
  {#snippet trigger(props)}
    <button type="button" class="choice" {disabled} aria-describedby="{id}-hint" {...props}>
      <span class="logo" aria-hidden="true">
        <ProjectLogo
          projectKey={value.key}
          color={value.logo_color}
          src={value.logo_url}
          size="sm"
        />
      </span>
      <span class="name">{value.name}</span>
      <ChevronDown size={16} aria-hidden="true" />
    </button>
    <span class="sr-only" id="{id}-hint">{m.changeProjectHint()}</span>
  {/snippet}
  {#snippet children({ close }: { close: () => void })}
    <OptionList
      label={m.chooseProject()}
      {options}
      key={(project: Project) => project.id}
      checked={(project: Project) => project.id === value.id}
      onchange={(project: Project) => {
        close();
        if (project.id !== value.id) onpick(project);
      }}
      bind:query
      searchLabel={m.findProject()}
      {loading}
      {problem}
    >
      {#snippet option(project: Project)}{project.name}{/snippet}
      {#snippet secondary(project: Project)}{project.key}{/snippet}
    </OptionList>
  {/snippet}
</Popover>

<style>
  .choice {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    min-height: var(--control-md);
    max-width: 100%;
    margin-inline: calc(-1 * var(--space-2));
    padding: var(--space-1) var(--space-2);
    border: 0;
    border-radius: var(--radius-sm);
    background: transparent;
    color: inherit;
    font: inherit;
    cursor: pointer;
  }

  .choice:hover,
  .choice[aria-expanded='true'] {
    background: var(--color-hover);
  }

  .logo {
    display: inline-flex;
  }

  .name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
