<script lang="ts">
  // The issue list's query bar (QRY-074): the context, the query, «Найти» and «Очистить», hide
  // resolved, copying the link and the list's settings (the columns, QRY-078). The query is highlighted by the browser's copy of the language
  // (`lib/query`, loaded on first focus): a layer under the transparent text of the input shows the
  // classes and underlines the problems — syntax ones while typing, the server's (names) for the
  // text it checked.
  import Link from '@lucide/svelte/icons/link';
  import Search from '@lucide/svelte/icons/search';
  import Settings2 from '@lucide/svelte/icons/settings-2';
  import X from '@lucide/svelte/icons/x';

  import type { QueryProblem } from '#lib/api/schema.gen.ts';
  import { loadQueryEngine, type QueryEngine } from '#lib/query/index.ts';
  import Button from '#lib/ui/Button.svelte';
  import IconButton from '#lib/ui/IconButton.svelte';
  import Menu from '#lib/ui/Menu.svelte';

  import ContextPicker from './ContextPicker.svelte';
  import type { ListContext, ResolvedContext } from './list-state.ts';
  import * as m from './messages.ts';

  interface Props {
    /** The query as the list shows it. */
    q: string;
    /** Name of the search region; the issue list's by default. */
    label?: string;
    /** The context picker; left out where the context is fixed (a board's header filter). */
    context?: ResolvedContext;
    hideResolved?: boolean;
    /** Problems the server found in `q`. */
    problems: readonly QueryProblem[];
    /** Id of the element that describes the problems, for the input. */
    describedby?: string;
    onsearch: (q: string) => void;
    oncontext?: (context: ListContext) => void;
    /** Shows the hide-resolved toggle. */
    onhideresolved?: (hide: boolean) => void;
    /** Shows the copy-link button. */
    oncopylink?: () => void;
    /** Shows the list settings with the column chooser. */
    oncolumns?: () => void;
  }

  let {
    q,
    label,
    context,
    hideResolved = false,
    problems,
    describedby,
    onsearch,
    oncontext,
    onhideresolved,
    oncopylink,
    oncolumns,
  }: Props = $props();

  // Typing is the bar's own until a search; a new `q` from the address replaces it.
  let text = $derived(q);

  let engine: QueryEngine | null = $state(null);
  let input: HTMLInputElement | undefined = $state();
  let layer: HTMLDivElement | undefined = $state();

  function wake(): void {
    if (engine) return;
    loadQueryEngine()
      .then((loaded) => {
        engine = loaded;
      })
      .catch((error: unknown) => {
        console.warn('The query language did not load', error);
      });
  }

  interface Run {
    text: string;
    class: string;
    problem: 'error' | 'note' | null;
  }

  /** The text cut into runs of one class and one kind of underline. */
  const runs: Run[] = $derived.by(() => {
    if (!engine || text === '') return [];
    const found = engine.analyze(text);
    const classes: string[] = Array.from({ length: text.length }, () => '');
    for (const token of found.tokens) {
      for (let i = token.from; i < token.to && i < text.length; i += 1) classes[i] = token.class;
    }
    const marks: ('error' | 'note' | null)[] = Array.from({ length: text.length }, () => null);
    const all = text === q ? [...found.problems, ...problems] : found.problems;
    for (const problem of all) {
      // A problem at the end (a missing value) underlines the last character.
      const from = Math.min(problem.from, Math.max(text.length - 1, 0));
      const to = Math.max(problem.to, from + 1);
      for (let i = from; i < to && i < text.length; i += 1) {
        if (marks[i] !== 'error') marks[i] = problem.severity;
      }
    }
    const out: Run[] = [];
    for (let i = 0; i < text.length; i += 1) {
      const last = out.at(-1);
      if (last && last.class === classes[i] && last.problem === marks[i])
        last.text += text.charAt(i);
      else out.push({ text: text.charAt(i), class: classes[i] ?? '', problem: marks[i] ?? null });
    }
    return out;
  });

  function follow(): void {
    if (input && layer) layer.scrollLeft = input.scrollLeft;
  }

  $effect(() => {
    if (runs.length > 0) follow();
  });

  function submit(event: SubmitEvent): void {
    event.preventDefault();
    onsearch(text);
  }

  function clear(): void {
    text = '';
    input?.focus();
    if (q !== '') onsearch('');
  }
</script>

<form class="bar" role="search" aria-label={label ?? m.searchBar()} onsubmit={submit}>
  {#if context && oncontext}<ContextPicker {context} onchoose={oncontext} />{/if}
  <div class="query" class:lit={runs.length > 0}>
    <div class="layer" bind:this={layer} aria-hidden="true">
      {#each runs as run, i (i)}<span
          class="run"
          data-class={run.class || undefined}
          data-problem={run.problem ?? undefined}>{run.text}</span
        >{/each}
    </div>
    <input
      bind:this={input}
      bind:value={text}
      type="text"
      aria-label={m.query()}
      aria-describedby={describedby}
      aria-invalid={problems.some((p) => p.severity === 'error') && text === q ? 'true' : undefined}
      placeholder={m.queryPlaceholder()}
      autocomplete="off"
      autocapitalize="none"
      spellcheck="false"
      onfocus={wake}
      oninput={follow}
      onscroll={follow}
      onkeyup={follow}
    />
    {#if text !== ''}
      <span class="clear">
        <IconButton icon={X} label={m.clear()} size="sm" type="button" onclick={clear} />
      </span>
    {/if}
  </div>
  <Button type="submit" variant="primary" icon={Search}>{m.find()}</Button>
  {#if onhideresolved}
    <Button
      type="button"
      variant="ghost"
      aria-pressed={hideResolved}
      onclick={() => {
        onhideresolved(!hideResolved);
      }}
    >
      <span class="box" aria-hidden="true" data-on={hideResolved || undefined}></span>
      {m.hideResolved()}
    </Button>
  {/if}
  {#if oncopylink}
    <IconButton icon={Link} label={m.copyLink()} type="button" onclick={oncopylink} />
  {/if}
  {#if oncolumns}
    <Menu
      label={m.listSettings()}
      items={[{ type: 'action', label: m.columnsMenu(), onSelect: oncolumns }]}
      placement={{ side: 'bottom', align: 'end' }}
    >
      {#snippet trigger(props)}
        <IconButton icon={Settings2} label={m.listSettings()} type="button" {...props} />
      {/snippet}
    </Menu>
  {/if}
</form>

<style>
  .bar {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-2);
  }

  .query {
    position: relative;
    display: flex;
    flex: 1 1 20rem;
    min-width: 0;
  }

  .layer,
  input {
    box-sizing: border-box;
    width: 100%;
    height: var(--control-md);
    padding: 0 calc(var(--control-sm) + var(--space-2)) 0 var(--space-2);
    font-family: var(--font-mono);
    /* 16 px on touch screens keeps mobile Safari from zooming into the field. */
    font-size: max(var(--text-md), 1rem);
    line-height: var(--control-md);
    letter-spacing: normal;
    white-space: pre;
  }

  @media (pointer: fine) {
    .layer,
    input {
      font-size: var(--text-md);
    }
  }

  .layer {
    position: absolute;
    inset: 0;
    overflow: hidden;
    border: 1px solid transparent;
    color: var(--color-text);
    pointer-events: none;
  }

  input {
    position: relative;
    border: 1px solid var(--color-border-control);
    border-radius: var(--radius-md);
    background: var(--color-surface);
    color: var(--color-text);
  }

  .lit input {
    background: transparent;
    color: transparent;
    caret-color: var(--color-text);
  }

  .lit .layer {
    background: var(--color-surface);
    border-radius: var(--radius-md);
  }

  input::placeholder {
    color: var(--color-text-faint);
  }

  input:hover {
    border-color: var(--color-text-muted);
  }

  input:focus-visible {
    outline-offset: 0;
    border-color: var(--color-focus);
  }

  input[aria-invalid='true'] {
    border-color: var(--color-danger);
  }

  .run[data-class='attribute'] {
    color: var(--color-link);
  }

  .run[data-class='keyword'] {
    color: var(--color-brand);
    font-weight: var(--weight-medium);
  }

  .run[data-class='operator'],
  .run[data-class='punctuation'] {
    color: var(--color-text-muted);
  }

  .run[data-class='error'] {
    color: var(--color-danger);
  }

  .run[data-problem] {
    text-decoration: underline wavy;
    text-decoration-skip-ink: none;
    text-underline-offset: 0.25em;
  }

  .run[data-problem='error'] {
    text-decoration-color: var(--color-danger);
  }

  .run[data-problem='note'] {
    text-decoration-color: var(--color-warning);
  }

  .clear {
    position: absolute;
    inset-block: 0;
    inset-inline-end: var(--space-1);
    display: flex;
    align-items: center;
  }

  .box {
    display: inline-block;
    width: 1em;
    height: 1em;
    border: 1px solid var(--color-border-control);
    border-radius: var(--radius-sm);
    background: var(--color-surface);
  }

  .box[data-on] {
    border-color: var(--color-ink);
    background: var(--color-ink);
    box-shadow: inset 0 0 0 2px var(--color-surface);
  }
</style>
