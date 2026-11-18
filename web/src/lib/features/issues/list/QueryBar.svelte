<script lang="ts">
  // The issue list's query bar (QRY-074): the context, the query, «Найти» and «Очистить», hide
  // resolved, copying the link and the list's settings (the columns, QRY-078). The query is highlighted by the browser's copy of the language
  // (`lib/query`, loaded on first focus): a layer under the transparent text of the input shows the
  // classes and underlines the problems — syntax ones while typing, the name ones from the assist
  // as the caret moves (2.1 §7). The same assist feeds the completion popup (QRY-056): Ctrl/⌘+Space
  // or ↓ opens it, the insert lands quoted with its suffix, and the problem under the pointer
  // explains itself in a tooltip (QRY-058).
  import Link from '@lucide/svelte/icons/link';
  import { SvelteSet } from 'svelte/reactivity';
  import Search from '@lucide/svelte/icons/search';
  import Settings2 from '@lucide/svelte/icons/settings-2';
  import SquareTerminal from '@lucide/svelte/icons/square-terminal';
  import X from '@lucide/svelte/icons/x';

  import type { QueryAssist, QueryProblem, QueryProblemCode } from '#lib/api/schema.gen.ts';
  import { api } from '#lib/api/client.ts';
  import { loadQueryEngine, type QueryEngine } from '#lib/query/index.ts';
  import { enter, leave, setQueryFocus } from '#lib/shortcuts/registry.svelte.ts';
  import Button from '#lib/ui/Button.svelte';
  import IconButton from '#lib/ui/IconButton.svelte';
  import Menu from '#lib/ui/Menu.svelte';

  import { inserted, move, popupKey, type Completion } from '../commands/completion.ts';
  import * as cmd from '../commands/messages.ts';
  import ContextPicker from './ContextPicker.svelte';
  import type { ListContext, ResolvedContext } from './list-state.svelte.ts';
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
    /** Shows «Применить команду ко всем…» (step 2.1 §6): the count the item names. */
    applyAll?: { count: number } | null;
    onapplyall?: () => void;
    /** Shows the phone's «Выбрать» (step 2.2 §8.1): the list's selection mode. */
    onselect?: () => void;
    /** The selection mode's state, for the entry's label. */
    selecting?: boolean;
    /** Tab enters the list at its first row (step 2.2 §6); false lets the key move on as usual. */
    ontolist?: () => boolean;
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
    applyAll = null,
    onapplyall,
    onselect,
    selecting = false,
    ontolist,
  }: Props = $props();

  // Typing is the bar's own until a search; a new `q` from the address replaces it.
  let text = $derived(q);

  let engine: QueryEngine | null = $state(null);
  let input: HTMLInputElement | undefined = $state();
  let layer: HTMLDivElement | undefined = $state();
  const uid = $props.id();

  // The `/` shortcut focuses the page's query box (step 2.2 §4): this bar answers while it is
  // a visible field — a bar inside a closed panel answers false, and the caller moves on.
  $effect(() => {
    return setQueryFocus(() => {
      if ((input?.offsetParent ?? null) === null) return false;
      input?.focus();
      return true;
    });
  });

  // The assist while the bar holds the focus (QRY-056): debounced, only the last call's answer
  // shows — and is used only while the text is the one it answered for — the marks would point
  // at the wrong places in what is typed since. A failure stays quiet: the bar must not nag.
  let focused = $state(false);
  let caret = $state(0);
  let answered = $state<{ line: string; found: QueryAssist } | null>(null);
  let completion = $state<Completion>({ index: null });
  let assistSeq = 0;

  // The bar's context (step 2.2 §7): its keys stay its own (§4, shipped in 2.1); while the field
  // holds the focus the registration lists them in the shortcut reference's query section.
  $effect(() => {
    if (!focused) return;
    enter('query');
    return () => leave('query');
  });

  $effect(() => {
    if (!focused) return;
    const line = text;
    const at = caret;
    const project = context?.kind === 'project' ? context.id : undefined;
    const sequence = ++assistSeq;
    const timer = setTimeout(() => {
      api
        .get('/queries/assist', { query: { q: line, caret: at, project } })
        .then((found: QueryAssist) => {
          if (sequence !== assistSeq) return;
          answered = { line, found };
        })
        .catch(() => undefined);
    }, 150);
    return () => {
      clearTimeout(timer);
    };
  });

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

  /** One problem as the marks keep it, with the text that names it (QRY-058). `live` problems —
   * the bar's own syntax and assist halves — feed the input's hidden description; the caller's
   * half is described by the element it passes, so it is not repeated. */
  interface Mark {
    error: boolean;
    message: string;
    live: boolean;
  }

  /** Every problem the bar knows of, per character: the WASM syntax half live, the assist's name
   * half for the text it answered for, the caller's server half for the text it checked. */
  const marks: (Mark | null)[] = $derived.by(() => {
    const per: (Mark | null)[] = Array.from({ length: text.length }, () => null);
    const put = (
      problem: { from: number; to: number; severity: string },
      code: QueryProblemCode,
      live: boolean,
    ): void => {
      const error = problem.severity === 'error';
      const message = m.problems[code]();
      // A problem at the end (a missing value) underlines the last character.
      const from = Math.min(problem.from, Math.max(text.length - 1, 0));
      const to = Math.max(problem.to, from + 1);
      for (let i = from; i < to && i < text.length; i += 1) {
        const held = per[i] ?? null;
        if (held === null || (!held.error && error)) per[i] = { error, message, live };
      }
    };
    if (engine && text !== '') {
      for (const problem of engine.analyze(text).problems) put(problem, problem.code, true);
    }
    if (answered !== null && answered.line === text) {
      for (const problem of answered.found.problems) put(problem, problem.code, true);
    }
    if (text === q) {
      for (const problem of problems) put(problem, problem.code, false);
    }
    return per;
  });

  /** The live problems in text order, each named once, for the input's description. */
  const described = $derived.by(() => {
    const seen = new SvelteSet<string>();
    const out: string[] = [];
    for (const mark of marks) {
      if (mark !== null && mark.live && !seen.has(mark.message)) {
        seen.add(mark.message);
        out.push(mark.message);
      }
    }
    return out;
  });

  interface Run {
    text: string;
    class: string;
    problem: 'error' | 'note' | null;
  }

  /** The text cut into runs of one class and one kind of underline. */
  const runs: Run[] = $derived.by(() => {
    const classes: string[] = Array.from({ length: text.length }, () => '');
    if (engine && text !== '') {
      for (const token of engine.analyze(text).tokens) {
        for (let i = token.from; i < token.to && i < text.length; i += 1) classes[i] = token.class;
      }
    }
    const out: Run[] = [];
    for (let i = 0; i < text.length; i += 1) {
      const last = out.at(-1);
      const held = marks[i] ?? null;
      const problem: 'error' | 'note' | null = held === null ? null : held.error ? 'error' : 'note';
      if (last && last.class === classes[i] && last.problem === problem)
        last.text += text.charAt(i);
      else out.push({ text: text.charAt(i), class: classes[i] ?? '', problem });
    }
    return out;
  });

  // ---- Completion (QRY-056, 2.1 §7) ----

  const suggestions = $derived(
    answered !== null && answered.line === text ? answered.found.suggestions : [],
  );
  const active = $derived(
    completion.index === null ? null : (suggestions[completion.index] ?? null),
  );

  function take(suggestionIndex: number): void {
    const suggestion = suggestions[suggestionIndex];
    if (!suggestion || !input) return;
    const next = inserted(text, suggestion);
    const field = input;
    text = next.text;
    completion = { index: null };
    requestAnimationFrame(() => {
      field.focus();
      field.setSelectionRange(next.caret, next.caret);
    });
  }

  function onKeydown(event: KeyboardEvent): void {
    // Tab leaves the bar for the list's first row (§6) — but an open completion owns the key: Tab
    // inserts its row (step 2.1). Shift+Tab walks back as usual.
    if (
      event.key === 'Tab' &&
      completion.index === null &&
      !event.shiftKey &&
      !event.metaKey &&
      !event.ctrlKey &&
      !event.altKey &&
      ontolist?.()
    ) {
      event.preventDefault();
      return;
    }
    // The manual trigger (§7): Ctrl/⌘+Space opens the list at its first row.
    if ((event.ctrlKey || event.metaKey) && event.key === ' ') {
      if (suggestions.length > 0) {
        event.preventDefault();
        completion = { index: 0 };
      }
      return;
    }
    const key = popupKey(event);
    if (completion.index !== null && key !== null) {
      event.preventDefault();
      if (key === 'insert') take(completion.index);
      else
        completion = { index: move(completion.index, key === 'down' ? 1 : -1, suggestions.length) };
      return;
    }
    if (key === 'down' && suggestions.length > 0 && completion.index === null) {
      event.preventDefault();
      completion = { index: 0 };
      return;
    }
    if (event.key === 'Escape' && completion.index !== null) {
      event.preventDefault();
      completion = { index: null };
      return;
    }
    // Enter over an open list takes the row; without one the form searches.
    if (
      event.key === 'Enter' &&
      !event.shiftKey &&
      !event.metaKey &&
      !event.ctrlKey &&
      !event.altKey &&
      completion.index !== null
    ) {
      event.preventDefault();
      take(completion.index);
    }
  }

  // ---- The problem tooltip (QRY-058) ----

  // The bar's font is monospace, so the character under the pointer divides the offset by one
  // measured width. The title then names the problem the pointer is on.
  let charWidth = $state(0);
  let hoverTitle = $state<string | null>(null);

  function measure(): void {
    if (!input) return;
    const style = getComputedStyle(input);
    const context = document.createElement('canvas').getContext('2d');
    if (!context) return;
    context.font = `${style.fontStyle} ${style.fontWeight} ${style.fontSize} ${style.fontFamily}`;
    charWidth = context.measureText('0123456789').width / 10;
  }

  function point(event: MouseEvent): void {
    if (!input || charWidth === 0 || marks.length === 0) {
      hoverTitle = null;
      return;
    }
    const box = input.getBoundingClientRect();
    const style = getComputedStyle(input);
    const offset =
      event.clientX - box.left + input.scrollLeft - parseFloat(style.paddingLeft || '0');
    const at = Math.min(Math.max(Math.floor(offset / charWidth), 0), text.length - 1);
    const held = at >= 0 ? (marks[at] ?? null) : null;
    hoverTitle = held !== null ? held.message : null;
  }

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
      role="combobox"
      aria-label={m.query()}
      aria-describedby={[describedby, described.length > 0 ? `${uid}-marks` : null]
        .filter(Boolean)
        .join(' ') || undefined}
      aria-invalid={marks.some((mark) => mark?.error) ? 'true' : undefined}
      placeholder={m.queryPlaceholder()}
      autocomplete="off"
      autocapitalize="none"
      spellcheck="false"
      title={hoverTitle ?? undefined}
      aria-autocomplete="list"
      aria-expanded={completion.index !== null}
      aria-controls={`${uid}-list`}
      aria-activedescendant={active !== null && completion.index !== null
        ? `${uid}-option-${String(completion.index)}`
        : undefined}
      onfocus={() => {
        wake();
        measure();
        focused = true;
        caret = input?.selectionStart ?? text.length;
      }}
      onblur={() => {
        focused = false;
        completion = { index: null };
      }}
      oninput={() => {
        completion = { index: null };
        caret = input?.selectionStart ?? text.length;
        follow();
      }}
      onkeyup={() => {
        caret = input?.selectionStart ?? text.length;
        follow();
      }}
      onscroll={follow}
      onkeydown={onKeydown}
      onmousemove={point}
      onmouseleave={() => {
        hoverTitle = null;
      }}
    />
    {#if text !== ''}
      <span class="clear">
        <IconButton icon={X} label={m.clear()} size="sm" type="button" onclick={clear} />
      </span>
    {/if}
    {#if suggestions.length > 0 && completion.index !== null}
      <ul
        id="{uid}-list"
        class="popup"
        role="listbox"
        aria-label={cmd.completionLabel()}
        onmousedown={(event) => event.preventDefault()}
      >
        {#each suggestions as suggestion, i (i)}
          <!-- svelte-ignore a11y_click_events_have_key_events -->
          <li
            id="{uid}-option-{i}"
            role="option"
            aria-selected={i === completion.index}
            class="option"
            data-active={i === completion.index || undefined}
            onclick={() => take(i)}
          >
            <span class="insert">{suggestion.text}</span>
            {#if suggestion.description}<span class="describe">{suggestion.description}</span>{/if}
            {#if suggestion.resolved}<span class="describe">{suggestion.resolved}</span>{/if}
            {#if suggestion.own}<span class="mark">{cmd.ownMark()}</span>{/if}
            <span class="group">{cmd.groups[suggestion.group]()}</span>
          </li>
        {/each}
      </ul>
    {/if}
  </div>
  {#if described.length > 0}
    <p id="{uid}-marks" class="sr-only">{described.join('; ')}</p>
  {/if}
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
  {#if oncolumns ?? onapplyall ?? onselect}
    <Menu
      label={m.listSettings()}
      items={[
        ...(onselect
          ? [
              {
                type: 'action' as const,
                label: selecting ? m.stopChoosing() : m.chooseRows(),
                onSelect: onselect,
              },
            ]
          : []),
        ...(onapplyall && applyAll && applyAll.count > 0
          ? [
              {
                type: 'action' as const,
                label: m.applyToAll({ count: applyAll.count }),
                icon: SquareTerminal,
                onSelect: onapplyall,
              },
            ]
          : []),
        ...(oncolumns ? [{ type: 'separator' as const }] : []),
        ...(oncolumns
          ? [{ type: 'action' as const, label: m.columnsMenu(), onSelect: oncolumns }]
          : []),
      ]}
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

  /* The completion popup (QRY-056), the command dialog's list over this bar. */
  .popup {
    position: absolute;
    z-index: 20;
    inset-inline: 0;
    top: calc(100% + var(--space-1));
    max-height: 16rem;
    margin: 0;
    overflow: auto;
    overscroll-behavior: contain;
    padding: var(--space-1);
    list-style: none;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    background: var(--color-surface-raised);
    box-shadow: var(--shadow-lg);
  }

  .option {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-1) var(--space-2);
    border-radius: var(--radius-sm);
    cursor: pointer;
  }

  .option[data-active] {
    background: var(--color-surface-sunken);
  }

  .insert {
    font-family: var(--font-mono);
  }

  .describe,
  .mark {
    color: var(--color-text-muted);
    font-size: var(--text-sm);
  }

  .group {
    margin-inline-start: auto;
    color: var(--color-text-faint);
    font-size: var(--text-sm);
  }

  /* Phones (D-019): the list's touch targets stay at least 44 px. */
  @media (pointer: coarse) {
    .option {
      min-height: 44px;
    }
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
