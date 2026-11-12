<script lang="ts">
  // «Основное» (BRD-086, BRD-011, BRD-013, BRD-016): the name, the projects, sprints on or off and
  // how issues join (BRD-024…BRD-029: by hand, every new issue into a chosen sprint, or a query),
  // the owner, who sees and who changes the board, and deleting it. Each part saves on its own.
  import { goto } from '$app/navigation';

  import type {
    AccountSummary,
    Board,
    BoardImpact,
    BoardMode,
    Project,
    Sprint,
  } from '#lib/api/schema.gen.ts';
  import { PeopleSearch } from '#lib/features/access/index.ts';
  import { QueryBar, SharedWithPicker, type SharedAudience } from '#lib/features/issues/index.ts';
  import { ProjectPicker, type ProjectChoice } from '#lib/features/projects/index.ts';
  import { can } from '#lib/session/index.ts';
  import Button from '#lib/ui/Button.svelte';
  import Checkbox from '#lib/ui/Checkbox.svelte';
  import Dialog from '#lib/ui/Dialog.svelte';
  import Notice from '#lib/ui/Notice.svelte';
  import RadioGroup from '#lib/ui/RadioGroup.svelte';
  import Select from '#lib/ui/Select.svelte';
  import TextField from '#lib/ui/TextField.svelte';
  import { describeError, toasts } from '#lib/ui/toast.svelte.ts';

  import { boardHref, boardImpact, deleteBoard } from '../boards.ts';
  import * as m from '../messages.ts';
  import { saveBoard } from './settings.ts';

  interface Props {
    board: Board;
    /** Sprints that are not archived, for the sprint new issues join. */
    sprints?: readonly Sprint[];
    busy?: boolean;
    onchanged: () => unknown;
    ondeleted: () => unknown;
  }

  let { board, sprints = [], busy = $bindable(false), onchanged, ondeleted }: Props = $props();

  const NAME_MAX = 100;
  const uid = $props.id();

  let name = $state('');
  let chosen = $state<ProjectChoice[]>([]);
  let view = $state<SharedAudience>({ users: [], groups: [] });
  let viewRestricted = $state(false);
  let edit = $state<SharedAudience>({ users: [], groups: [] });
  let editRestricted = $state(false);
  let viewReaders = $state(false);
  let editUpdaters = $state(false);
  let mode = $state<string>('manual');
  let errors = $state<Record<string, string | null>>({});
  let problem = $state<string | null>(null);
  let saving = $state<string | null>(null);

  let transferOpen = $state(false);
  let newOwner = $state<AccountSummary[]>([]);
  let transferError = $state<string | null>(null);

  let deleteOpen = $state(false);
  let impact = $state<BoardImpact | null>(null);
  let deleteProblem = $state<string | null>(null);

  // The form follows the stored board: after each save and after a reload.
  $effect(() => {
    name = board.name;
    mode = board.mode;
    chosen = board.projects.map(({ project }) => ({
      id: project.id,
      name: project.name,
      key: project.key,
      logo_color: project.logo_color,
      logo_url: project.logo_url,
    }));
    const sharing = board.sharing;
    if (sharing) {
      view = { users: sharing.view.users, groups: sharing.view.groups };
      viewRestricted = sharing.view.users.length + sharing.view.groups.length > 0;
      edit = { users: sharing.edit.users, groups: sharing.edit.groups };
      editRestricted = sharing.edit.users.length + sharing.edit.groups.length > 0;
      viewReaders = sharing.view_readers;
      editUpdaters = sharing.edit_updaters;
    }
  });

  const projectsChanged = $derived(
    chosen.map((one) => one.id).join() !== board.projects.map((one) => one.project.id).join(),
  );

  async function save(
    part: string,
    patch: Parameters<typeof saveBoard>[1],
    reload = false,
  ): Promise<boolean> {
    problem = null;
    errors = {};
    saving = part;
    busy = true;
    try {
      const found = await saveBoard(board, patch, [
        'name',
        'projects',
        'owner',
        'sprints_on',
        'mode',
        'query',
        'auto_sprint',
      ]);
      if (found) {
        errors = found.fields;
        problem = found.message;
        return false;
      }
      toasts.show({ message: m.saved(), tone: 'success' });
      if (reload) await onchanged();
      return true;
    } finally {
      saving = null;
      busy = false;
    }
  }

  function saveName(event: SubmitEvent): void {
    event.preventDefault();
    const typed = name.trim();
    if (!typed) {
      errors = { name: m.nameRequired() };
      return;
    }
    if (typed !== board.name) void save('name', { name: typed });
  }

  function saveProjects(): void {
    if (chosen.length === 0) {
      errors = { projects: m.projectsRequired() };
      return;
    }
    void save('projects', { projects: chosen.map((one) => one.id) }, true);
  }

  const modeOptions = $derived(
    (['manual', 'auto', 'query'] as const).map((key) => ({
      value: key,
      label: m.modes[key](),
      hint: m.modeHints[key](),
    })),
  );
  const sprintOptions = $derived([
    { value: '', label: m.autoSprintCurrent() },
    ...sprints
      .filter((one) => !one.internal && !one.archived)
      .map((one) => ({ value: String(one.id), label: one.name })),
  ]);

  function saveSprints(event: Event): void {
    const on = (event.currentTarget as HTMLInputElement).checked;
    void save('sprints', { sprints_on: on }, true);
  }

  function saveMode(event: Event): void {
    const chosen = (event.target as HTMLInputElement).value as BoardMode;
    if (chosen === board.mode) return;
    void save('mode', { mode: chosen }, true).then((saved) => {
      if (!saved) mode = board.mode;
    });
  }

  function saveQuery(text: string): void {
    void save('query', { query: text }, true);
  }

  function saveAutoSprint(event: Event): void {
    const value = (event.currentTarget as HTMLSelectElement).value;
    void save('auto_sprint', { auto_sprint: value === '' ? null : Number(value) }, true);
  }

  function ids(audience: SharedAudience): { users: number[]; groups: number[] } {
    return {
      users: audience.users.map((user) => user.id),
      groups: audience.groups.map((group) => group.id),
    };
  }

  function saveSharing(): void {
    const found = {
      view:
        viewRestricted && view.users.length + view.groups.length === 0 ? m.nobodyChosen() : null,
      edit:
        editRestricted && edit.users.length + edit.groups.length === 0 ? m.nobodyChosen() : null,
    };
    errors = found;
    if (found.view || found.edit) return;
    void save(
      'sharing',
      {
        sharing: {
          view: viewRestricted ? ids(view) : { users: [], groups: [] },
          edit: editRestricted ? ids(edit) : { users: [], groups: [] },
          view_readers: viewReaders,
          edit_updaters: editUpdaters,
        },
      },
      true,
    );
  }

  async function transfer(): Promise<void> {
    const person = newOwner[0];
    if (!person) {
      transferError = m.transferRequired();
      return;
    }
    transferError = null;
    if (await save('owner', { owner: person.id })) {
      transferOpen = false;
      toasts.show({ message: m.transferred({ name: person.full_name }), tone: 'success' });
    } else {
      transferError = errors.owner ?? problem;
    }
  }

  async function askDelete(): Promise<void> {
    impact = null;
    deleteProblem = null;
    deleteOpen = true;
    try {
      impact = await boardImpact(board.id);
    } catch {
      impact = null;
    }
  }

  async function remove(): Promise<void> {
    busy = true;
    deleteProblem = null;
    const named = board.name;
    try {
      await deleteBoard(board.id);
      deleteOpen = false;
      await ondeleted();
      toasts.show({ message: m.deleted({ name: named }), tone: 'success' });
      // boardHref resolves the address.
      // eslint-disable-next-line svelte/no-navigation-without-resolve
      await goto(boardHref.list());
    } catch (error) {
      deleteProblem = describeError(error).message;
    } finally {
      busy = false;
    }
  }
</script>

<div class="general">
  {#if problem}<Notice tone="danger">{problem}</Notice>{/if}

  <form class="part" novalidate onsubmit={saveName}>
    <TextField
      label={m.name()}
      bind:value={name}
      required
      maxlength={NAME_MAX}
      autocomplete="off"
      error={errors.name ?? null}
    />
    <div class="buttons">
      <Button
        type="submit"
        size="sm"
        loading={saving === 'name'}
        disabled={name.trim() === board.name}>{m.save()}</Button
      >
    </div>
  </form>

  <div class="part">
    <ProjectPicker
      label={m.projects()}
      bind:chosen
      allow={(project: Project) => can('issue.read', project.id)}
      error={errors.projects ?? null}
    />
    {#if projectsChanged}
      <div class="buttons">
        <Button size="sm" loading={saving === 'projects'} onclick={saveProjects}>{m.save()}</Button>
      </div>
    {/if}
  </div>

  <section class="part" aria-labelledby="{uid}-joining">
    <h3 id="{uid}-joining">{m.joining()}</h3>
    <Checkbox
      label={m.sprintsOn()}
      checked={board.sprints_on}
      disabled={saving !== null}
      onchange={saveSprints}
    />
    {#if errors.sprints_on}<Notice tone="danger">{errors.sprints_on}</Notice>{/if}
    <div onchange={saveMode}>
      <RadioGroup legend={m.modeLegend()} options={modeOptions} bind:value={mode} />
    </div>
    {#if errors.mode}<Notice tone="danger">{errors.mode}</Notice>{/if}
    {#if board.mode === 'auto' && board.sprints_on}
      <Select
        label={m.autoSprint()}
        options={sprintOptions}
        value={board.auto_sprint === null ? '' : String(board.auto_sprint)}
        disabled={saving !== null}
        error={errors.auto_sprint ?? null}
        onchange={saveAutoSprint}
      />
    {/if}
    {#if board.mode === 'query'}
      <QueryBar
        q={board.query ?? ''}
        label={m.boardQuery()}
        problems={board.query_problems}
        onsearch={saveQuery}
      />
      {#if board.query_problems.length > 0}
        <Notice tone="warning">{m.boardQueryProblems()}</Notice>
      {/if}
      {#if errors.query}<Notice tone="danger">{errors.query}</Notice>{/if}
    {/if}
  </section>

  <section class="part" aria-labelledby="{uid}-owner">
    <h3 id="{uid}-owner">{m.owner()}</h3>
    <p>{board.owner.full_name}</p>
    {#if board.rights.transfer}
      <div>
        <Button
          size="sm"
          onclick={() => {
            newOwner = [];
            transferError = null;
            transferOpen = true;
          }}>{m.transfer()}</Button
        >
      </div>
    {/if}
  </section>

  {#if board.rights.share && board.sharing}
    <section class="part" aria-labelledby="{uid}-sharing">
      <h3 id="{uid}-sharing">{m.sharing()}</h3>
      <SharedWithPicker
        legend={m.whoViews()}
        bind:value={view}
        bind:restricted={viewRestricted}
        error={errors.view ?? null}
      />
      <Checkbox label={m.viewReaders()} bind:checked={viewReaders} />
      <SharedWithPicker
        legend={m.whoEdits()}
        bind:value={edit}
        bind:restricted={editRestricted}
        error={errors.edit ?? null}
      />
      <Checkbox label={m.editUpdaters()} bind:checked={editUpdaters} />
      <div class="buttons">
        <Button size="sm" loading={saving === 'sharing'} onclick={saveSharing}>{m.save()}</Button>
      </div>
    </section>
  {/if}

  {#if board.rights.delete}
    <section class="part" aria-labelledby="{uid}-delete">
      <h3 id="{uid}-delete">{m.deleteSection()}</h3>
      <div>
        <Button variant="danger" size="sm" onclick={() => void askDelete()}
          >{m.deleteBoard()}</Button
        >
      </div>
    </section>
  {/if}
</div>

<Dialog bind:open={transferOpen} title={m.transferTitle()} size="sm" dismissible={!busy}>
  <div class="part">
    <PeopleSearch
      label={m.transferPick()}
      mode="single"
      bind:chosen={newOwner}
      exclude={[board.owner.id]}
    />
    {#if transferError}<Notice tone="danger">{transferError}</Notice>{/if}
  </div>
  {#snippet footer()}
    <Button variant="ghost" disabled={busy} onclick={() => (transferOpen = false)}
      >{m.cancel()}</Button
    >
    <Button variant="primary" loading={saving === 'owner'} onclick={() => void transfer()}
      >{m.transferDo()}</Button
    >
  {/snippet}
</Dialog>

<Dialog
  bind:open={deleteOpen}
  title={m.deleteTitle({ name: board.name })}
  size="sm"
  dismissible={!busy}
>
  <p>
    {#if impact}{m.deleteImpact({ sprints: impact.sprints, cards: impact.cards })}{/if}
    {m.deleteForever()}
  </p>
  {#if deleteProblem}<Notice tone="danger">{deleteProblem}</Notice>{/if}
  {#snippet footer()}
    <Button variant="ghost" disabled={busy} onclick={() => (deleteOpen = false)}
      >{m.cancel()}</Button
    >
    <Button variant="danger" loading={busy} onclick={() => void remove()}>{m.deleteBoard()}</Button>
  {/snippet}
</Dialog>

<style>
  .general {
    display: flex;
    flex-direction: column;
    gap: var(--space-6);
  }

  .part {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }

  h3 {
    font-size: var(--text-md);
  }

  .buttons {
    display: flex;
    justify-content: flex-end;
  }
</style>
