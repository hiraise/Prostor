<script lang="ts">
  // Deleting groups (ACC-032): first what ends — memberships and role assignments — then the group
  // that takes their place, so content limited to them stays limited. The choices are radio
  // buttons (with a search when the groups do not fit one page), so it works the same with a
  // keyboard, a screen reader and a finger.
  import { untrack } from 'svelte';

  import type { Group, GroupDeletionImpact, GroupRef } from '#lib/api/schema.gen.ts';
  import { builtinGroups, findGroups, groupName } from '#lib/features/access/index.ts';
  import * as common from '#lib/i18n/messages/common.ts';
  import Button from '#lib/ui/Button.svelte';
  import Dialog from '#lib/ui/Dialog.svelte';
  import Spinner from '#lib/ui/Spinner.svelte';
  import TextField from '#lib/ui/TextField.svelte';
  import { describeError, toasts } from '#lib/ui/toast.svelte.ts';

  import Badge from './Badge.svelte';
  import { deleteGroups, deletionImpact, problemOf, type Problem } from './groups.ts';
  import * as m from './messages.ts';
  import ProblemNotice from './ProblemNotice.svelte';

  interface Props {
    open?: boolean;
    /** The groups to delete; never built-in ones. */
    groups: readonly Pick<Group, 'id' | 'name' | 'builtin'>[];
    ondeleted: (ids: number[]) => unknown;
  }

  let { open = $bindable(false), groups, ondeleted }: Props = $props();

  type Candidate = GroupRef & { description: string };

  const id = $props.id();
  let ids = $state<number[]>([]);
  let impact = $state<GroupDeletionImpact | null>(null);
  let impactProblem = $state<string | null>(null);
  let text = $state('');
  let candidates = $state<Candidate[]>([]);
  let cursor = $state<string | null>(null);
  let searching = $state(false);
  let searched = $state(false);
  let searchProblem = $state<string | null>(null);
  /** More groups than one page: offer a search, and keep it for this opening. */
  let searchable = $state(false);
  let replacement = $state<Candidate | null>(null);
  let busy = $state(false);
  let problem = $state<Problem | null>(null);
  let list: HTMLElement | undefined = $state();
  let impactRun = 0;
  let searchRun = 0;

  const count = $derived(ids.length);
  const firstName = $derived(groups[0] ? groupName(groups[0]) : '');
  // The chosen group stays on the list while the search changes.
  const shown = $derived.by(() => {
    const others = candidates.filter((group) => !ids.includes(group.id));
    if (replacement && !others.some((group) => group.id === replacement?.id)) {
      return [replacement, ...others];
    }
    return others;
  });

  // Every opening starts over with the groups chosen now.
  $effect(() => {
    if (!open) return;
    untrack(() => {
      ids = groups.map((group) => group.id);
      impact = null;
      replacement = null;
      problem = null;
      text = '';
      searchable = false;
      void loadImpact();
    });
  });

  // Search a moment after typing stops.
  $effect(() => {
    if (!open) return;
    const typed = text;
    const timer = setTimeout(() => void search(typed, false), typed ? 250 : 0);
    return () => {
      clearTimeout(timer);
    };
  });

  async function loadImpact(): Promise<void> {
    const run = ++impactRun;
    impactProblem = null;
    try {
      const answer = await deletionImpact(ids);
      if (run === impactRun) impact = answer;
    } catch (error) {
      if (run === impactRun) impactProblem = describeError(error).message;
    }
  }

  async function search(typed: string, more: boolean): Promise<void> {
    const run = ++searchRun;
    searching = true;
    searchProblem = null;
    try {
      const page = await findGroups(typed, more ? cursor : null);
      if (run !== searchRun) return;
      candidates = more ? [...candidates, ...page.items] : page.items;
      cursor = page.next_cursor;
      searched = true;
      if (cursor !== null) searchable = true;
    } catch (error) {
      if (run === searchRun) searchProblem = describeError(error).message;
    } finally {
      if (run === searchRun) searching = false;
    }
  }

  async function confirm(): Promise<void> {
    if (!replacement) {
      problem = { message: m.chooseReplacement(), missing: [] };
      list?.querySelector<HTMLInputElement>('input[type="radio"]')?.focus();
      return;
    }
    busy = true;
    problem = null;
    const deleted = [...ids];
    try {
      await deleteGroups(deleted, replacement.id);
      open = false;
      toasts.show({ tone: 'success', message: m.groupsDeleted({ count: deleted.length }) });
      await ondeleted(deleted);
    } catch (error) {
      problem = problemOf(error, { lastAdministrator: m.noAdministratorLeft });
    } finally {
      busy = false;
    }
  }
</script>

<Dialog bind:open title={m.deleteTitle({ count, name: firstName })} dismissible={!busy}>
  <div class="content">
    {#if problem}<ProblemNotice {problem} />{/if}

    {#if count > 1}
      <ul class="names" aria-label={m.groupsToDelete()}>
        {#each groups as group (group.id)}
          <li>{groupName(group)}</li>
        {/each}
      </ul>
    {/if}

    <section class="impact" aria-labelledby="{id}-impact">
      <h3 id="{id}-impact">{m.whatEnds()}</h3>
      {#if impact}
        <ul>
          <li>{m.membershipsEnd({ count: impact.memberships, groups: count })}</li>
          <li>{m.assignmentsRevoked({ count: impact.assignments })}</li>
        </ul>
      {:else if impactProblem}
        <p class="danger">{impactProblem}</p>
        <div><Button onclick={loadImpact}>{common.retry()}</Button></div>
      {:else}
        <Spinner label={common.loading()} />
      {/if}
    </section>

    <div class="replacement">
      <!-- A search only when the groups do not fit one page; the dialog does not open on it. -->
      {#if searchable}
        <TextField
          label={m.findReplacement()}
          type="search"
          bind:value={text}
          autocomplete="off"
          spellcheck="false"
        />
      {/if}
      <fieldset class="choices" aria-describedby="{id}-hint" aria-busy={searching}>
        <legend>{m.replacement()}</legend>
        <p id="{id}-hint" class="hint">{m.replacementHint()}</p>
        {#if searchProblem}
          <p class="note danger">{searchProblem}</p>
        {:else if searched && shown.length === 0}
          <p class="note">{m.noOtherGroups()}</p>
        {:else if !searched}
          <p class="note"><Spinner /></p>
        {/if}
        <ul class="options" bind:this={list}>
          {#each shown as group (group.id)}
            <li>
              <label class="option">
                <input
                  type="radio"
                  name="{id}-replacement"
                  checked={replacement?.id === group.id}
                  onchange={() => {
                    replacement = group;
                    problem = null;
                  }}
                />
                <span class="text">
                  <span class="name">
                    {groupName(group)}
                    {#if group.builtin}<Badge tone="info">{m.builtinBadge()}</Badge>{/if}
                  </span>
                  {#if group.builtin}
                    <span class="description">{builtinGroups[group.builtin].description()}</span>
                  {:else if group.description}
                    <span class="description">{group.description}</span>
                  {/if}
                </span>
              </label>
            </li>
          {/each}
        </ul>
        {#if cursor}
          <div>
            <Button variant="ghost" loading={searching} onclick={() => search(text, true)}>
              {m.showMore()}
            </Button>
          </div>
        {/if}
      </fieldset>
    </div>

    <p class="final">{m.cannotUndo()}</p>
  </div>

  {#snippet footer()}
    <Button
      disabled={busy}
      onclick={() => {
        open = false;
      }}
    >
      {common.cancel()}
    </Button>
    <Button variant="danger" loading={busy} disabled={!impact} onclick={confirm}>
      {m.deleteGroups({ count })}
    </Button>
  {/snippet}
</Dialog>

<style>
  .content {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }

  .names {
    margin: 0;
    padding-left: var(--space-5);
    overflow-wrap: anywhere;
  }

  .impact {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
  }

  .impact ul {
    margin: 0;
    padding-left: var(--space-5);
  }

  h3 {
    font-size: var(--text-md);
    font-weight: var(--weight-semibold);
  }

  .replacement {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }

  .choices {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    min-width: 0;
    margin: 0;
    padding: 0;
    border: none;
  }

  legend {
    margin-bottom: var(--space-1);
    padding: 0;
    font-weight: var(--weight-semibold);
  }

  .hint,
  .note {
    color: var(--color-text-muted);
    font-size: var(--text-sm);
  }

  .options {
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

  .options:empty {
    display: none;
  }

  li + li .option {
    border-top: 1px solid var(--color-border);
  }

  .option {
    display: flex;
    align-items: flex-start;
    gap: var(--space-3);
    min-height: var(--control-md);
    padding: var(--space-2) var(--space-3);
    cursor: pointer;
  }

  .option:hover {
    background: var(--color-hover);
  }

  input {
    flex: none;
    width: 1rem;
    height: 1rem;
    margin: 0.15em 0 0;
    accent-color: var(--color-ink);
  }

  @media (pointer: coarse) {
    input {
      width: 1.25rem;
      height: 1.25rem;
    }
  }

  .text {
    display: flex;
    flex-direction: column;
    gap: var(--space-0_5);
    min-width: 0;
  }

  .name {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-2);
    overflow-wrap: anywhere;
  }

  .description {
    color: var(--color-text-muted);
    font-size: var(--text-sm);
    overflow-wrap: anywhere;
  }

  .danger {
    color: var(--color-danger);
  }

  .final {
    color: var(--color-text-muted);
    font-size: var(--text-sm);
  }
</style>
