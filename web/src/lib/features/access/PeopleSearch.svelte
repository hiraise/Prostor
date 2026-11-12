<script lang="ts">
  // Finding and choosing people (ACC-070): a search field and the matches as checkboxes (several
  // people) or radio buttons (one), so it works the same with a keyboard, a screen reader and a
  // finger. Chosen people stay chosen while the search changes.
  import type { AccountSummary } from '#lib/api/schema.gen.ts';
  import { getLocale } from '#lib/i18n/locale.svelte.ts';
  import { formatNumber } from '#lib/i18n/message.ts';
  import Button from '#lib/ui/Button.svelte';
  import Spinner from '#lib/ui/Spinner.svelte';
  import TextField from '#lib/ui/TextField.svelte';
  import { describeError } from '#lib/ui/toast.svelte.ts';

  import * as m from './messages.ts';
  import { findPeople, type AccountState } from './people.ts';
  import PersonName from './PersonName.svelte';

  interface Props {
    /** The legend of the choices. */
    label: string;
    mode?: 'single' | 'multiple';
    chosen?: AccountSummary[];
    /** People who cannot be chosen (members already, the account being deleted). */
    exclude?: readonly number[];
    /** Which accounts to offer; enabled ones by default. */
    accounts?: AccountState;
  }

  let {
    label,
    mode = 'multiple',
    chosen = $bindable([]),
    exclude = [],
    accounts = 'enabled',
  }: Props = $props();

  const name = $props.id();
  let text = $state('');
  let results = $state<AccountSummary[]>([]);
  let cursor = $state<string | null>(null);
  let loading = $state(false);
  let problem = $state<string | null>(null);
  let searched = $state(false);
  let controller: AbortController | null = null;

  async function load(more: boolean): Promise<void> {
    controller?.abort();
    const current = new AbortController();
    controller = current;
    loading = true;
    problem = null;
    try {
      const page = await findPeople(text, {
        state: accounts,
        cursor: more ? cursor : null,
        signal: current.signal,
      });
      if (current.signal.aborted) return;
      results = more ? [...results, ...page.items] : page.items;
      cursor = page.next_cursor;
      searched = true;
    } catch (error) {
      if (current.signal.aborted) return;
      problem = describeError(error).message;
    } finally {
      if (controller === current) loading = false;
    }
  }

  // Search a moment after typing stops.
  $effect(() => {
    // Read here so the effect runs again when the text changes.
    const typed = text;
    const timer = setTimeout(() => {
      if (typed === text) void load(false);
    }, 250);
    return () => {
      clearTimeout(timer);
    };
  });

  const shown = $derived(results.filter((person) => !exclude.includes(person.id)));

  function isChosen(id: number): boolean {
    return chosen.some((person) => person.id === id);
  }

  function toggle(person: AccountSummary, on: boolean): void {
    if (mode === 'single') {
      chosen = on ? [person] : [];
    } else if (on && !isChosen(person.id)) {
      chosen = [...chosen, person];
    } else if (!on) {
      chosen = chosen.filter((other) => other.id !== person.id);
    }
  }
</script>

<div class="search">
  <TextField
    label={m.searchPeople()}
    type="search"
    hint={m.searchPeopleHint()}
    bind:value={text}
    autocomplete="off"
    autocapitalize="none"
    spellcheck="false"
  />
  <fieldset class="results" aria-busy={loading}>
    <legend class="legend">
      {label}
      {#if mode === 'multiple' && chosen.length > 0}
        <span class="count"
          >· {m.chosenCount({ count: formatNumber(getLocale(), chosen.length) })}</span
        >
      {/if}
    </legend>
    {#if problem}
      <p class="note danger">{problem}</p>
    {:else if searched && shown.length === 0}
      <p class="note">{m.noPeopleFound()}</p>
    {:else if !searched}
      <p class="note"><Spinner /></p>
    {/if}
    <ul class="list">
      {#each shown as person (person.id)}
        <li>
          <label class="option">
            <input
              type={mode === 'single' ? 'radio' : 'checkbox'}
              {name}
              checked={isChosen(person.id)}
              onchange={(event) => {
                toggle(person, event.currentTarget.checked);
              }}
            />
            <PersonName {person} withUsername />
            {#if person.email}<span class="email">{person.email}</span>{/if}
          </label>
        </li>
      {/each}
    </ul>
    {#if cursor}
      <div>
        <Button size="sm" variant="ghost" {loading} onclick={() => load(true)}
          >{m.showMore()}</Button
        >
      </div>
    {/if}
  </fieldset>
</div>

<style>
  .search {
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
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-1) var(--space-3);
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
    width: 1rem;
    height: 1rem;
    margin: 0;
    accent-color: var(--color-ink);
  }

  .email {
    margin-left: auto;
    color: var(--color-text-muted);
    font-size: var(--text-sm);
    overflow-wrap: anywhere;
  }

  .note {
    color: var(--color-text-muted);
    font-size: var(--text-sm);
  }

  .danger {
    color: var(--color-danger);
  }
</style>
