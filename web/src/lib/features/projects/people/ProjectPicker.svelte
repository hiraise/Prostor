<script lang="ts">
  // Choosing projects (ACC-059, ACC-060; the projects of a new board, BRD-003): a search field and
  // the active projects `allow` offers as checkboxes, so it works the same with a keyboard, a
  // screen reader and a finger. Chosen projects stay chosen while the search changes.
  import type { Project } from '#lib/api/schema.gen.ts';
  import { getLocale } from '#lib/i18n/locale.svelte.ts';
  import { formatNumber } from '#lib/i18n/message.ts';
  import { can } from '#lib/session/index.ts';
  import Button from '#lib/ui/Button.svelte';
  import Spinner from '#lib/ui/Spinner.svelte';
  import TextField from '#lib/ui/TextField.svelte';
  import { describeError } from '#lib/ui/toast.svelte.ts';

  import ProjectLogo from '../ProjectLogo.svelte';
  import { findProjects } from '../projects.ts';
  import * as m from './messages.ts';
  import type { ProjectChoice } from './people.ts';

  interface Props {
    /** The legend of the choices. */
    label: string;
    chosen?: ProjectChoice[];
    /** Shown under the choices when nothing is chosen but something must be. */
    error?: string | null;
    /** Which active projects are offered; by default those the viewer manages (roles). */
    allow?: (project: Project) => boolean;
  }

  let {
    label,
    chosen = $bindable([]),
    error = null,
    allow = (project: Project) => can('project.update', project.id),
  }: Props = $props();

  const id = $props.id();
  let text = $state('');
  let results = $state<Project[]>([]);
  let cursor = $state<string | null>(null);
  let loading = $state(false);
  let searched = $state(false);
  let problem = $state<string | null>(null);
  let request = 0;

  async function load(more: boolean): Promise<void> {
    const mine = ++request;
    loading = true;
    problem = null;
    try {
      const page = await findProjects(text, more ? cursor : null);
      if (mine !== request) return;
      results = more ? [...results, ...page.items] : page.items;
      cursor = page.next_cursor;
      searched = true;
    } catch (failure) {
      if (mine === request) problem = describeError(failure).message;
    } finally {
      if (mine === request) loading = false;
    }
  }

  // Search a moment after typing stops; the first page at once.
  $effect(() => {
    const typed = text;
    const timer = setTimeout(() => void load(false), typed ? 250 : 0);
    return () => {
      clearTimeout(timer);
    };
  });

  const shown = $derived(
    results.filter((project) => project.archived_at === null && allow(project)),
  );

  function isChosen(projectId: number): boolean {
    return chosen.some((project) => project.id === projectId);
  }

  function toggle(project: Project, on: boolean): void {
    if (on && !isChosen(project.id)) {
      chosen = [
        ...chosen,
        {
          id: project.id,
          name: project.name,
          key: project.key,
          logo_color: project.logo_color,
          logo_url: project.logo_url,
        },
      ];
    } else if (!on) {
      chosen = chosen.filter((other) => other.id !== project.id);
    }
  }
</script>

<div class="picker">
  <TextField
    label={m.findProjects()}
    type="search"
    hint={m.findProjectsHint()}
    bind:value={text}
    autocomplete="off"
    spellcheck="false"
  />
  <fieldset
    class="results"
    aria-busy={loading}
    aria-describedby={error ? `${id}-error` : undefined}
  >
    <legend class="legend">
      {label}
      {#if chosen.length > 0}
        <span class="count"
          >· {m.chosenCount({ count: formatNumber(getLocale(), chosen.length) })}</span
        >
      {/if}
    </legend>
    {#if problem}
      <p class="note danger">{problem}</p>
    {:else if searched && shown.length === 0 && !cursor}
      <p class="note">{m.noProjectsFound()}</p>
    {:else if !searched}
      <p class="note"><Spinner /></p>
    {/if}
    <ul class="list">
      {#each shown as project (project.id)}
        <li>
          <label class="option">
            <input
              type="checkbox"
              checked={isChosen(project.id)}
              onchange={(event) => {
                toggle(project, event.currentTarget.checked);
              }}
            />
            <ProjectLogo
              projectKey={project.key}
              color={project.logo_color}
              src={project.logo_url ?? null}
              size="sm"
            />
            <span class="name">{project.name}</span>
            <span class="key">{project.key}</span>
          </label>
        </li>
      {/each}
    </ul>
    {#if cursor}
      <div>
        <Button size="sm" variant="ghost" {loading} onclick={() => load(true)}>
          {m.showMore()}
        </Button>
      </div>
    {/if}
    {#if error}<p id="{id}-error" class="note danger">{error}</p>{/if}
  </fieldset>
</div>

<style>
  .picker {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }

  .results {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    min-width: 0;
    margin: 0;
    padding: 0;
    border: none;
  }

  .legend {
    margin-bottom: var(--space-1);
    padding: 0;
    font-weight: var(--weight-medium);
  }

  .count {
    color: var(--color-text-muted);
    font-weight: var(--weight-regular);
  }

  .list {
    display: flex;
    flex-direction: column;
    max-height: 16rem;
    margin: 0;
    padding: 0;
    overflow-y: auto;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    list-style: none;
  }

  .list:empty {
    display: none;
  }

  .option {
    display: flex;
    align-items: center;
    gap: var(--space-2) var(--space-3);
    min-height: var(--control-md);
    padding: var(--space-1_5) var(--space-3);
    cursor: pointer;
  }

  .option:hover {
    background: var(--color-hover);
  }

  li + li .option {
    border-top: 1px solid var(--color-border);
  }

  input {
    flex: none;
    width: 1rem;
    height: 1rem;
    margin: 0;
    accent-color: var(--color-ink);
  }

  .name {
    min-width: 0;
    overflow-wrap: anywhere;
  }

  .key {
    margin-left: auto;
    color: var(--color-text-muted);
    font-family: var(--font-mono);
    font-size: var(--text-sm);
  }

  .note {
    color: var(--color-text-muted);
    font-size: var(--text-sm);
  }

  .danger {
    color: var(--color-danger);
  }
</style>
