<script lang="ts">
  // Choosing the default audience of a project's new issues and articles (PRJ-039): nobody in
  // particular (everyone who can see the project), or one group or project team. Radio buttons
  // with a search, so it works the same with a keyboard, a screen reader and a finger.
  import { untrack } from 'svelte';

  import type { Group, GroupRef } from '#lib/api/schema.gen.ts';
  import { builtinGroups, groupName } from '#lib/features/access/index.ts';
  import * as common from '#lib/i18n/messages/common.ts';
  import Button from '#lib/ui/Button.svelte';
  import Dialog from '#lib/ui/Dialog.svelte';
  import Spinner from '#lib/ui/Spinner.svelte';
  import TextField from '#lib/ui/TextField.svelte';
  import { describeError } from '#lib/ui/toast.svelte.ts';

  import { findAudiences } from '../projects.ts';
  import * as m from './messages.ts';

  interface Props {
    open?: boolean;
    /** The audience the form holds now. */
    current: GroupRef | null;
    onchoose: (audience: GroupRef | null) => void;
  }

  let { open = $bindable(false), current, onchoose }: Props = $props();

  /** Typing pauses this long before the groups are searched. */
  const DEBOUNCE_MS = 250;

  const id = $props.id();
  let text = $state('');
  let found = $state<Group[]>([]);
  let cursor = $state<string | null>(null);
  let searching = $state(false);
  let searched = $state(false);
  let problem = $state<string | null>(null);
  let chosen = $state.raw<GroupRef | null>(null);
  let run = 0;

  // The chosen group stays on the list while the search changes.
  const shown = $derived.by((): GroupRef[] => {
    const refs: GroupRef[] = found;
    if (chosen && !refs.some((group) => group.id === chosen?.id)) return [chosen, ...refs];
    return refs;
  });

  // Every opening starts from the audience the form holds.
  $effect(() => {
    if (!open) return;
    untrack(() => {
      text = '';
      chosen = current;
    });
  });

  // Search a moment after typing stops; at once when the dialog opens.
  $effect(() => {
    if (!open) return;
    const typed = text;
    const timer = setTimeout(() => void search(typed, false), typed ? DEBOUNCE_MS : 0);
    return () => {
      clearTimeout(timer);
    };
  });

  async function search(typed: string, more: boolean): Promise<void> {
    const mine = ++run;
    searching = true;
    problem = null;
    try {
      const page = await findAudiences(typed, more ? cursor : null);
      if (mine !== run) return;
      found = more ? [...found, ...page.items] : page.items;
      cursor = page.next_cursor;
      searched = true;
    } catch (error) {
      if (mine === run) problem = describeError(error).message;
    } finally {
      if (mine === run) searching = false;
    }
  }

  function describe(group: GroupRef): string | null {
    if (group.builtin) return builtinGroups[group.builtin].description();
    const full = found.find((other) => other.id === group.id);
    return full?.description ?? null;
  }

  function confirm(): void {
    onchoose(chosen === null ? null : { ...chosen });
    open = false;
  }
</script>

<Dialog bind:open title={m.audienceTitle()}>
  <div class="content">
    <TextField
      label={m.findAudience()}
      type="search"
      bind:value={text}
      autocomplete="off"
      spellcheck="false"
    />
    <fieldset class="choices" aria-busy={searching}>
      <legend>{m.visibleTo()}</legend>
      <ul class="options">
        <li>
          <label class="option">
            <input
              type="radio"
              name="{id}-audience"
              checked={chosen === null}
              onchange={() => {
                chosen = null;
              }}
            />
            <span class="name">{m.everyone()}</span>
          </label>
        </li>
        {#each shown as group (group.id)}
          {@const description = describe(group)}
          <li>
            <label class="option">
              <input
                type="radio"
                name="{id}-audience"
                checked={chosen?.id === group.id}
                onchange={() => {
                  chosen = {
                    id: group.id,
                    name: group.name,
                    builtin: group.builtin,
                    team_of: group.team_of,
                  };
                }}
              />
              <span class="text">
                <span class="name">{groupName(group)}</span>
                {#if description}<span class="description">{description}</span>{/if}
              </span>
            </label>
          </li>
        {/each}
      </ul>
      {#if problem}
        <p class="note danger">{problem}</p>
      {:else if !searched}
        <p class="note"><Spinner label={common.loading()} /></p>
      {:else if shown.length === 0}
        <p class="note">{m.noAudienceFound()}</p>
      {/if}
      {#if cursor}
        <div>
          <Button variant="ghost" loading={searching} onclick={() => search(text, true)}>
            {m.showMore()}
          </Button>
        </div>
      {/if}
    </fieldset>
  </div>

  {#snippet footer()}
    <Button
      onclick={() => {
        open = false;
      }}
    >
      {common.cancel()}
    </Button>
    <Button variant="primary" onclick={confirm}>{m.choose()}</Button>
  {/snippet}
</Dialog>

<style>
  .content {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
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

  .note {
    color: var(--color-text-muted);
    font-size: var(--text-sm);
  }

  .danger {
    color: var(--color-danger);
  }

  .options {
    display: flex;
    flex-direction: column;
    max-height: 20rem;
    margin: 0;
    padding: 0;
    overflow-y: auto;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    list-style: none;
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
    overflow-wrap: anywhere;
  }

  .description {
    color: var(--color-text-muted);
    font-size: var(--text-sm);
    overflow-wrap: anywhere;
  }
</style>
