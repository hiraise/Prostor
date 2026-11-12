<script lang="ts">
  // A new board (BRD-003…BRD-010): the preset with one line about it, the name (a board the person
  // already sees under that name is flagged), the projects («Все мои проекты» takes the projects
  // of the person's teams), who sees the board and who changes its settings. The same
  // form is the dialog on wide screens and the page on phones.
  import { untrack } from 'svelte';

  import type { Board, BoardPreset, Project } from '#lib/api/schema.gen.ts';
  import { refusal, focusFirstError } from '#lib/features/fields/index.ts';
  import { SharedWithPicker, type SharedAudience } from '#lib/features/issues/index.ts';
  import { ProjectPicker, type ProjectChoice } from '#lib/features/projects/index.ts';
  import { can } from '#lib/session/index.ts';
  import Button from '#lib/ui/Button.svelte';
  import Checkbox from '#lib/ui/Checkbox.svelte';
  import Notice from '#lib/ui/Notice.svelte';
  import RadioGroup from '#lib/ui/RadioGroup.svelte';
  import TextField from '#lib/ui/TextField.svelte';
  import { describeError } from '#lib/ui/toast.svelte.ts';

  import { createBoard, findBoards, myProjects, nameTaken } from '../boards.ts';
  import * as m from '../messages.ts';

  interface Props {
    /** The project the form opened from (a project's boards); chosen at the start. */
    project?: ProjectChoice;
    oncreated: (board: Board) => unknown;
    oncancel: () => unknown;
    /** A request is in flight: the dialog may not be dismissed. */
    busy?: boolean;
  }

  let { project, oncreated, oncancel, busy = $bindable(false) }: Props = $props();

  const NAME_MAX = 100;
  const PRESETS: BoardPreset[] = ['scrum', 'kanban', 'custom'];

  let form: HTMLFormElement | undefined = $state();
  let preset = $state<string>('scrum');
  let name = $state('');
  let chosen = $state<ProjectChoice[]>(untrack(() => (project ? [project] : [])));
  let view = $state<SharedAudience>({ users: [], groups: [] });
  let viewRestricted = $state(false);
  let edit = $state<SharedAudience>({ users: [], groups: [] });
  let editRestricted = $state(false);
  let viewReaders = $state(false);
  let editUpdaters = $state(false);
  let taken = $state(false);
  let errors = $state<Record<string, string | null>>({});
  let problem = $state<string | null>(null);
  let loadingMine = $state(false);

  // A board the person already sees under the typed name, from the list a moment after typing.
  $effect(() => {
    const typed = name.trim();
    if (!typed) {
      taken = false;
      return;
    }
    const controller = new AbortController();
    const timer = setTimeout(() => {
      findBoards({ q: typed }, null, controller.signal).then(
        (page) => {
          taken = nameTaken(typed, page.items);
        },
        () => undefined,
      );
    }, 250);
    return () => {
      clearTimeout(timer);
      controller.abort();
    };
  });

  function readsIssues(candidate: Project): boolean {
    return can('issue.read', candidate.id);
  }

  async function chooseMine(): Promise<void> {
    loadingMine = true;
    try {
      const found = await myProjects();
      const ids = new Set(chosen.map((one) => one.id));
      chosen = [
        ...chosen,
        ...found
          .filter((one) => !ids.has(one.id))
          .map((one) => ({
            id: one.id,
            name: one.name,
            key: one.key,
            logo_color: one.logo_color,
            logo_url: one.logo_url,
          })),
      ];
      errors.projects = null;
    } catch (error) {
      problem = describeError(error).message;
    } finally {
      loadingMine = false;
    }
  }

  function ids(audience: SharedAudience): { users: number[]; groups: number[] } {
    return {
      users: audience.users.map((user) => user.id),
      groups: audience.groups.map((group) => group.id),
    };
  }

  async function submit(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    problem = null;
    const found: Record<string, string | null> = {
      name: name.trim() ? null : m.nameRequired(),
      projects: chosen.length > 0 ? null : m.projectsRequired(),
      view:
        viewRestricted && view.users.length + view.groups.length === 0 ? m.nobodyChosen() : null,
      edit:
        editRestricted && edit.users.length + edit.groups.length === 0 ? m.nobodyChosen() : null,
    };
    errors = found;
    if (Object.values(found).some(Boolean)) {
      focusFirstError(form);
      return;
    }
    busy = true;
    try {
      const board = await createBoard({
        name: name.trim(),
        preset: preset as BoardPreset,
        projects: chosen.map((one) => one.id),
        sharing: {
          view: viewRestricted ? ids(view) : { users: [], groups: [] },
          edit: editRestricted ? ids(edit) : { users: [], groups: [] },
          view_readers: viewReaders,
          edit_updaters: editUpdaters,
        },
      });
      await oncreated(board);
    } catch (error) {
      const found = refusal(error, { paths: ['name', 'projects'] });
      errors = { ...errors, ...found.fields };
      problem = found.message;
      focusFirstError(form);
    } finally {
      busy = false;
    }
  }
</script>

<form bind:this={form} class="form" novalidate onsubmit={(event) => void submit(event)}>
  {#if problem}<Notice tone="danger">{problem}</Notice>{/if}
  <RadioGroup
    legend={m.presetLegend()}
    options={PRESETS.map((key) => ({
      value: key,
      label: m.presets[key](),
      hint: m.presetHints[key](),
    }))}
    bind:value={preset}
  />
  <TextField
    label={m.name()}
    bind:value={name}
    required
    maxlength={NAME_MAX}
    autocomplete="off"
    error={errors.name ?? null}
    hint={taken ? m.nameTaken() : undefined}
  />
  <div class="projects">
    <ProjectPicker
      label={m.projects()}
      bind:chosen
      allow={readsIssues}
      error={errors.projects ?? null}
    />
    <div>
      <Button size="sm" loading={loadingMine} onclick={chooseMine}>{m.allMyProjects()}</Button>
    </div>
  </div>
  <div class="sharing">
    <SharedWithPicker
      legend={m.whoViews()}
      bind:value={view}
      bind:restricted={viewRestricted}
      error={errors.view ?? null}
    />
    <Checkbox label={m.viewReaders()} bind:checked={viewReaders} />
  </div>
  <div class="sharing">
    <SharedWithPicker
      legend={m.whoEdits()}
      bind:value={edit}
      bind:restricted={editRestricted}
      error={errors.edit ?? null}
    />
    <Checkbox label={m.editUpdaters()} bind:checked={editUpdaters} />
  </div>
  <div class="buttons">
    <Button variant="ghost" disabled={busy} onclick={() => oncancel()}>{m.cancel()}</Button>
    <Button variant="primary" type="submit" loading={busy}>{m.create()}</Button>
  </div>
</form>

<style>
  .form {
    display: flex;
    flex-direction: column;
    gap: var(--space-5);
  }

  .projects,
  .sharing {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }

  .buttons {
    display: flex;
    flex-wrap: wrap;
    justify-content: flex-end;
    gap: var(--space-2);
  }
</style>
