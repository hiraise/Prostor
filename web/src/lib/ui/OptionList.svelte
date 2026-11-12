<script lang="ts" generics="T">
  // Choosing from a list (design §7): a search field and the options as radio buttons (one value)
  // or checkboxes (several), never a hover-only combobox. Keyboard: ArrowDown from the search goes
  // to the options; arrows, Home and End move between them without choosing; Space or Enter
  // chooses; ArrowUp on the first goes back to the search. The number of matches is announced.
  // The caller loads the options for `query` and decides what a choice does.
  import { tick, type Snippet } from 'svelte';

  import * as common from '#lib/i18n/messages/common.ts';

  import { announce } from './announce.svelte.ts';
  import Spinner from './Spinner.svelte';

  interface Props {
    /** The legend of the options. */
    label: string;
    mode?: 'single' | 'multiple';
    options: readonly T[];
    key: (option: T) => string | number;
    checked: (option: T) => boolean;
    /** Options shown but not changeable (the last value of a field that cannot be empty). */
    disabled?: (option: T) => boolean;
    onchange: (option: T, on: boolean) => void;
    /** The search text; no search field when `undefined`. */
    query?: string;
    searchLabel?: string;
    loading?: boolean;
    /** A failed load, said instead of the options. */
    problem?: string | null;
    option: Snippet<[T]>;
    /** A second line under an option (a value's description, a username). */
    secondary?: Snippet<[T]>;
    /** A row after the options, such as «Add "X"». */
    action?: Snippet;
  }

  let {
    label,
    mode = 'single',
    options,
    key,
    checked,
    disabled,
    onchange,
    query = $bindable(),
    searchLabel = common.search(),
    loading = false,
    problem = null,
    option,
    secondary,
    action,
  }: Props = $props();

  const id = $props.id();
  let list: HTMLUListElement | undefined = $state();
  let search: HTMLInputElement | undefined = $state();

  // Announce the number of matches once a search has answered.
  let announcedFor: string | undefined;
  $effect(() => {
    const count = options.length;
    if (loading || query === undefined || query === '' || query === announcedFor) return;
    announcedFor = query;
    announce(count === 0 ? common.nothingFound() : common.found({ count }));
  });

  function inputs(): HTMLInputElement[] {
    return list ? [...list.querySelectorAll<HTMLInputElement>('input:not([disabled])')] : [];
  }

  function focusAt(index: number): void {
    const all = inputs();
    if (all.length === 0) return;
    all[Math.min(Math.max(index, 0), all.length - 1)]?.focus();
  }

  // An option that leaves the list with the change (a removed archived value) hands focus to the
  // search, so it never drops to the page.
  function changed(item: T, input: HTMLInputElement): void {
    onchange(item, input.checked);
    void tick().then(() => {
      if (!input.isConnected && list?.isConnected) (search ?? inputs()[0])?.focus();
    });
  }

  function handleSearchKeydown(event: KeyboardEvent): void {
    if (event.key === 'ArrowDown') {
      event.preventDefault();
      focusAt(0);
    } else if (event.key === 'Enter') {
      // A form around the list must not submit from the search.
      event.preventDefault();
    }
  }

  function handleListKeydown(event: KeyboardEvent): void {
    const target = event.target;
    if (!(target instanceof HTMLInputElement)) return;
    const all = inputs();
    const current = all.indexOf(target);
    switch (event.key) {
      case 'ArrowDown':
      case 'ArrowRight':
        event.preventDefault();
        focusAt(current + 1);
        break;
      case 'ArrowUp':
      case 'ArrowLeft':
        event.preventDefault();
        if (current === 0 && search) search.focus();
        else focusAt(current - 1);
        break;
      case 'Home':
        event.preventDefault();
        focusAt(0);
        break;
      case 'End':
        event.preventDefault();
        focusAt(all.length - 1);
        break;
      case 'Enter':
        event.preventDefault();
        target.click();
        break;
    }
  }
</script>

<div class="options">
  {#if query !== undefined}
    <input
      bind:this={search}
      bind:value={query}
      class="search"
      type="search"
      aria-label={searchLabel}
      placeholder={searchLabel}
      aria-controls="{id}-list"
      autocomplete="off"
      autocapitalize="none"
      spellcheck="false"
      onkeydown={handleSearchKeydown}
    />
  {/if}
  <fieldset class="choices" aria-busy={loading}>
    <legend class="legend">{label}</legend>
    {#if problem}
      <p class="note danger">{problem}</p>
    {:else if loading && options.length === 0}
      <p class="note"><Spinner /></p>
    {:else if options.length === 0}
      <p class="note">{common.nothingFound()}</p>
    {/if}
    <!-- Arrow keys are handled for the inputs inside. -->
    <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
    <ul bind:this={list} id="{id}-list" class="list" onkeydown={handleListKeydown}>
      {#each options as item (key(item))}
        <li>
          <label class="option">
            <input
              type={mode === 'single' ? 'radio' : 'checkbox'}
              name={id}
              checked={checked(item)}
              disabled={disabled?.(item) ?? false}
              onchange={(event) => {
                changed(item, event.currentTarget);
              }}
              onclick={(event) => {
                // Choosing the chosen radio again still closes a single-value picker.
                if (mode === 'single' && checked(item)) {
                  event.preventDefault();
                  onchange(item, true);
                }
              }}
            />
            <span class="text">
              <span class="main">{@render option(item)}</span>
              {#if secondary}<span class="secondary">{@render secondary(item)}</span>{/if}
            </span>
          </label>
        </li>
      {/each}
    </ul>
    {#if action}<div class="action">{@render action()}</div>{/if}
  </fieldset>
  {#if loading && options.length > 0}
    <span class="sr-only">{common.loading()}</span>
  {/if}
</div>

<style>
  .options {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    min-width: 0;
  }

  .search {
    width: 100%;
    min-height: var(--control-md);
    padding: 0 var(--space-3);
    border: 1px solid var(--color-border-control);
    border-radius: var(--radius-md);
    background: var(--color-surface);
    color: var(--color-text);
    font: inherit;
    /* 16 px on touch screens keeps mobile Safari from zooming into the field. */
    font-size: max(var(--text-md), 1rem);
  }

  @media (pointer: fine) {
    .search {
      font-size: var(--text-md);
    }
  }

  .search:focus-visible {
    outline-offset: 0;
    border-color: var(--color-focus);
  }

  .choices {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    min-width: 0;
    margin: 0;
    padding: 0;
    border: none;
  }

  .legend {
    margin-bottom: var(--space-1);
    padding: 0;
    color: var(--color-text-muted);
    font-size: var(--text-sm);
  }

  .list {
    display: flex;
    flex-direction: column;
    max-height: 18rem;
    margin: 0;
    padding: 0;
    overflow-y: auto;
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
    padding: var(--space-1) var(--space-2);
    border-radius: var(--radius-sm);
    cursor: pointer;
  }

  .option:hover,
  .option:focus-within {
    background: var(--color-hover);
  }

  input[type='radio'],
  input[type='checkbox'] {
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

  .main {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    min-width: 0;
    overflow-wrap: anywhere;
  }

  .secondary {
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

  .sr-only {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip-path: inset(50%);
    white-space: nowrap;
  }
</style>
