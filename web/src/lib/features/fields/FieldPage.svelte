<script lang="ts">
  import { brand } from '#lib/i18n/messages/brand.ts';
  // One field of the instance (FLD-016…FLD-025): its name and aliases, shared by every project
  // using it; what projects start with when they add it (FLD-019) and whether new projects get it
  // (FLD-020); its values; the projects using it; and deleting it with the impact shown (FLD-023).
  import Lock from '@lucide/svelte/icons/lock';
  import Sparkles from '@lucide/svelte/icons/sparkles';
  import Trash2 from '@lucide/svelte/icons/trash-2';
  import { untrack } from 'svelte';
  import { resolve } from '$app/paths';

  import type { Field, FieldBoards, FieldDeletionImpact, UsedIn } from '#lib/api/schema.gen.ts';
  import { boardHref } from '#lib/features/boards/index.ts';
  import { projectHref } from '#lib/features/projects/index.ts';
  import * as common from '#lib/i18n/messages/common.ts';
  import { can, canSeeFields } from '#lib/session/index.ts';
  import type { EntityHandle } from '#lib/store/index.ts';
  import Button from '#lib/ui/Button.svelte';
  import Dialog from '#lib/ui/Dialog.svelte';
  import ErrorState from '#lib/ui/ErrorState.svelte';
  import ForbiddenState from '#lib/ui/ForbiddenState.svelte';
  import NotFoundState from '#lib/ui/NotFoundState.svelte';
  import Notice from '#lib/ui/Notice.svelte';
  import PageHeader from '#lib/ui/PageHeader.svelte';
  import Spinner from '#lib/ui/Spinner.svelte';
  import { toasts } from '#lib/ui/toast.svelte.ts';

  import Badge from './Badge.svelte';
  import FieldBaseForm from './FieldBaseForm.svelte';
  import FieldStartingForm from './FieldStartingForm.svelte';
  import {
    deleteField,
    fieldBoards,
    fieldDeletionImpact,
    fieldProjects,
    forgetField,
  } from './fields.ts';
  import * as m from './messages.ts';
  import { refusalText, typeLabel } from './problems.ts';

  interface Props {
    /** The field; `null` when it does not exist or is hidden. */
    field: EntityHandle<Field> | null;
    /** The field was deleted: leave its page. */
    ondeleted: () => unknown;
    home?: string;
  }

  let { field: handle, ondeleted, home }: Props = $props();

  const uid = $props.id();
  let usedIn = $state<UsedIn | null>(null);
  let projectsError = $state<unknown>(null);
  let boards = $state<FieldBoards | null>(null);
  let boardsError = $state<unknown>(null);
  let deleteOpen = $state(false);
  let impact = $state<FieldDeletionImpact | null>(null);
  let impactError = $state<string | null>(null);
  let deleteBusy = $state(false);
  let deleteProblem = $state<string | null>(null);

  const field = $derived(handle?.current);
  const fieldId = $derived(handle?.id ?? null);

  async function loadProjects(): Promise<void> {
    const id = fieldId;
    if (id === null) return;
    projectsError = null;
    try {
      usedIn = await fieldProjects(id);
    } catch (error) {
      projectsError = error;
    }
  }

  async function loadBoards(): Promise<void> {
    const id = fieldId;
    if (id === null) return;
    boardsError = null;
    try {
      boards = await fieldBoards(id);
    } catch (error) {
      boardsError = error;
    }
  }

  // The projects and boards using the field, again when another field opens.
  $effect(() => {
    if (fieldId === null) return;
    untrack(() => {
      usedIn = null;
      boards = null;
      void loadProjects();
      void loadBoards();
    });
  });

  /** How one board uses the field, in the board settings' own words (BRD-022). */
  function usesOf(use: FieldBoards['items'][number]): string[] {
    const parts: string[] = [];
    if (use.column) parts.push(m.boardUses.column());
    if (use.lane) parts.push(m.boardUses.lane());
    if (use.color) parts.push(m.boardUses.color());
    if (use.estimation) parts.push(m.boardUses.estimation());
    if (use.original_estimation) parts.push(m.boardUses.original_estimation());
    if (use.cards) parts.push(m.boardUses.cards());
    return parts;
  }

  /** What deleting takes away: projects, issue values (FLD-023), boards (BRD-022), and for good. */
  function impactText(found: FieldDeletionImpact): string {
    const parts = [
      found.projects > 0 ? m.deleteFieldImpact({ count: found.projects }) : m.deleteFieldUnused(),
    ];
    if (found.issues > 0) parts.push(m.deleteFieldIssues({ count: found.issues }));
    else if (found.projects > 0) parts.push(m.deleteFieldNoIssues());
    if (found.boards > 0) parts.push(m.deleteFieldBoards({ count: found.boards }));
    parts.push(m.cannotUndo());
    return parts.join(' ');
  }

  async function askToDelete(): Promise<void> {
    const id = fieldId;
    if (id === null) return;
    impact = null;
    impactError = null;
    deleteProblem = null;
    deleteOpen = true;
    try {
      impact = await fieldDeletionImpact(id);
    } catch (error) {
      impactError = refusalText(error);
    }
  }

  async function confirmDelete(): Promise<void> {
    const id = fieldId;
    if (id === null) return;
    deleteBusy = true;
    deleteProblem = null;
    try {
      await deleteField(id);
      deleteOpen = false;
      toasts.show({ tone: 'success', message: m.fieldDeleted() });
      await ondeleted();
      forgetField(id);
    } catch (error) {
      deleteProblem = refusalText(error);
    } finally {
      deleteBusy = false;
    }
  }
</script>

<svelte:head>
  <title>{field?.name ?? m.fieldsTitle()} · {brand()}</title>
</svelte:head>

{#if !canSeeFields()}
  <ForbiddenState {home} />
{:else if !field}
  <NotFoundState {home} />
{:else}
  <PageHeader title={field.name}>
    {#snippet actions()}
      {#if can('system.write')}
        <Button variant="danger" icon={Trash2} onclick={askToDelete}>{m.deleteField()}</Button>
      {/if}
    {/snippet}
  </PageHeader>

  <div class="page">
    <div class="summary">
      <span class="type">{typeLabel(field)}</span>
      {#if field.builtin}<Badge icon={Lock} text={m.builtIn()} />{/if}
      {#if field.auto_attach}<Badge icon={Sparkles} text={m.autoAttachBadge()} />{/if}
    </div>

    {#key field.id}
      <FieldBaseForm {field} />
      <FieldStartingForm {field} />
    {/key}

    <section class="section" aria-labelledby="{uid}-values">
      <h2 id="{uid}-values">{m.values()}</h2>
      {#if field.set}
        <p>
          <a
            class="link"
            href={resolve('/(app)/admin/value-sets/[id]', { id: String(field.set.id) })}
            >{m.openSet({ name: field.set.name })}</a
          >
        </p>
      {:else if field.kind === 'group'}
        <p class="note">{m.groupFieldNote()}</p>
      {:else}
        <p class="note">{m.simpleFieldNote()}</p>
      {/if}
    </section>

    <section class="section" aria-labelledby="{uid}-projects">
      <h2 id="{uid}-projects">{m.projectsTitle()}</h2>
      {#if projectsError}
        <ErrorState error={projectsError} level={3} onretry={loadProjects} />
      {:else if !usedIn}
        <Spinner label={common.loading()} />
      {:else if usedIn.items.length === 0 && usedIn.hidden === 0}
        <p class="note">{m.noProjects()}</p>
      {:else}
        {#if usedIn.items.length > 0}
          <ul class="projects" aria-label={m.projectsOfField({ name: field.name })}>
            {#each usedIn.items as project (project.id)}
              <li>
                <!-- Hrefs come from resolve() in projectHref. -->
                <!-- eslint-disable-next-line svelte/no-navigation-without-resolve -->
                <a class="link" href={projectHref.overview(project)}>{project.name}</a>
                <span class="key">{project.key}</span>
              </li>
            {/each}
          </ul>
        {/if}
        {#if usedIn.hidden > 0}<p class="note">{m.usedInHidden({ count: usedIn.hidden })}</p>{/if}
      {/if}
    </section>

    <section class="section" aria-labelledby="{uid}-boards">
      <h2 id="{uid}-boards">{m.boardsTitle()}</h2>
      {#if boardsError}
        <ErrorState error={boardsError} level={3} onretry={loadBoards} />
      {:else if !boards}
        <Spinner label={common.loading()} />
      {:else if boards.items.length === 0 && boards.others === 0}
        <p class="note">{m.noFieldBoards()}</p>
      {:else}
        {#if boards.items.length > 0}
          <ul class="boards" aria-label={m.fieldBoardsLabel({ name: field.name })}>
            {#each boards.items as use (use.board.id)}
              {@const uses = usesOf(use)}
              <li>
                <!-- Hrefs come from resolve() in boardHref. -->
                <!-- eslint-disable-next-line svelte/no-navigation-without-resolve -->
                <a class="link" href={boardHref.board(use.board.id)}>{use.board.name}</a>
                {#if uses.length > 0}<span class="uses">{uses.join(', ')}</span>{/if}
              </li>
            {/each}
          </ul>
        {/if}
        {#if boards.others > 0}
          <p class="note">{m.fieldBoardsHidden({ count: boards.others })}</p>
        {/if}
      {/if}
    </section>
  </div>

  <Dialog
    bind:open={deleteOpen}
    title={m.deleteFieldTitle({ name: field.name })}
    size="sm"
    dismissible={!deleteBusy}
  >
    <div class="confirm">
      {#if deleteProblem}<Notice tone="danger">{deleteProblem}</Notice>{/if}
      {#if impactError}
        <Notice tone="danger">{impactError}</Notice>
      {:else if !impact}
        <Spinner label={common.loading()} />
      {:else}
        <p>{impactText(impact)}</p>
      {/if}
    </div>
    {#snippet footer()}
      <Button
        disabled={deleteBusy}
        onclick={() => {
          deleteOpen = false;
        }}
      >
        {common.cancel()}
      </Button>
      <Button
        variant="danger"
        loading={deleteBusy}
        disabled={!impact && !impactError}
        onclick={confirmDelete}
      >
        {m.deleteField()}
      </Button>
    {/snippet}
  </Dialog>
{/if}

<style>
  .page {
    display: flex;
    flex-direction: column;
    gap: var(--space-8);
    max-width: var(--content-max);
    padding: var(--space-2) var(--space-6) var(--space-12);
  }

  .summary {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-2);
  }

  .type {
    color: var(--color-text-muted);
  }

  .section {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }

  h2 {
    font-size: var(--text-lg);
  }

  .note {
    color: var(--color-text-muted);
    font-size: var(--text-sm);
  }

  .link {
    display: inline-flex;
    align-items: center;
    min-height: var(--control-md);
    min-width: var(--control-md);
    color: var(--color-link);
  }

  .projects {
    display: flex;
    flex-direction: column;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .projects li {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-2);
  }

  .boards {
    display: flex;
    flex-direction: column;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .boards li {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-2);
  }

  .uses {
    color: var(--color-text-muted);
    font-size: var(--text-sm);
  }

  .key {
    color: var(--color-text-muted);
    font-family: var(--font-mono);
    font-size: var(--text-sm);
  }

  .confirm {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }

  @media (width < 48rem) {
    .page {
      padding: var(--space-2) var(--space-4) var(--space-8);
    }
  }
</style>
