<script lang="ts">
  // Choosing groups of people (PRJ-048): a search field and the groups as checkboxes, built-in ones
  // first. Project teams are never offered: a team cannot be in another team. Chosen groups stay
  // chosen while the search changes.
  import type { Group } from '#lib/api/schema.gen.ts';
  import { builtinGroups, findGroups, groupName } from '#lib/features/access/index.ts';
  import { getLocale } from '#lib/i18n/locale.svelte.ts';
  import { formatNumber } from '#lib/i18n/message.ts';
  import Button from '#lib/ui/Button.svelte';
  import Spinner from '#lib/ui/Spinner.svelte';
  import TextField from '#lib/ui/TextField.svelte';
  import { describeError } from '#lib/ui/toast.svelte.ts';

  import * as m from './messages.ts';

  interface Props {
    label: string;
    chosen?: Group[];
    /** Groups that cannot be chosen (in the team already). */
    exclude?: readonly number[];
  }

  let { label, chosen = $bindable([]), exclude = [] }: Props = $props();

  let text = $state('');
  let results = $state<Group[]>([]);
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
      const page = await findGroups(text, more ? cursor : null);
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

  $effect(() => {
    const typed = text;
    const timer = setTimeout(() => void load(false), typed ? 250 : 0);
    return () => {
      clearTimeout(timer);
    };
  });

  const shown = $derived(
    results.filter((group) => group.team_of === null && !exclude.includes(group.id)),
  );

  function isChosen(id: number): boolean {
    return chosen.some((group) => group.id === id);
  }

  function toggle(group: Group, on: boolean): void {
    if (on && !isChosen(group.id)) chosen = [...chosen, group];
    else if (!on) chosen = chosen.filter((other) => other.id !== group.id);
  }
</script>

<div class="picker">
  <TextField
    label={m.findGroups()}
    type="search"
    hint={m.findGroupsHint()}
    bind:value={text}
    autocomplete="off"
    spellcheck="false"
  />
  <fieldset class="results" aria-busy={loading}>
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
      <p class="note">{m.noGroupsFound()}</p>
    {:else if !searched}
      <p class="note"><Spinner /></p>
    {/if}
    <ul class="list">
      {#each shown as group (group.id)}
        <li>
          <label class="option">
            <input
              type="checkbox"
              checked={isChosen(group.id)}
              onchange={(event) => {
                toggle(group, event.currentTarget.checked);
              }}
            />
            <span class="text">
              <span class="name">{groupName(group)}</span>
              {#if group.builtin}
                <span class="about">{builtinGroups[group.builtin].description()}</span>
              {/if}
            </span>
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
    max-height: 18rem;
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
    gap: var(--space-3);
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

  .text {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }

  .name {
    overflow-wrap: anywhere;
  }

  .about,
  .note {
    color: var(--color-text-muted);
    font-size: var(--text-sm);
  }

  .danger {
    color: var(--color-danger);
  }
</style>
