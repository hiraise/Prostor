<script lang="ts">
  // A rendered Markdown text (CNT-038, CNT-040…CNT-045, CNT-054, CNT-056): the server's sanitized
  // HTML with its markers turned into people, issues and file names for this viewer (render.ts).
  // With `ontoggle` its checklist boxes can be ticked (CNT-041); the caller saves the change and
  // a rejected promise puts the box back.
  import type { Attachment } from 'svelte/attachments';

  import type { TextRefs } from '#lib/api/schema.gen.ts';

  import { renderMarkdown } from './render.ts';

  interface Props {
    html: string;
    refs?: TextRefs;
    ontoggle?: (line: number, checked: boolean) => unknown;
    /** The level a `#` heading of the text gets on the page; default 2, under the page's h1. */
    headingLevel?: number;
    class?: string;
  }

  let { html, refs, ontoggle, headingLevel = 2, class: className }: Props = $props();

  const content: Attachment<HTMLElement> = (element) =>
    renderMarkdown(element, html, { refs, ontoggle, headingLevel });
</script>

<div class={['markdown', className]} {@attach content}></div>

<style>
  .markdown {
    min-width: 0;
    overflow-wrap: anywhere;
  }

  .markdown > :global(:first-child) {
    margin-top: 0;
  }

  .markdown > :global(:last-child) {
    margin-bottom: 0;
  }

  .markdown :global(:is(p, ul, ol, blockquote, .table-frame, .code-block, hr)) {
    margin: 0 0 var(--space-3);
  }

  .markdown :global(:is(h2, h3, h4, h5, h6)) {
    margin: var(--space-5) 0 var(--space-2);
    font-weight: var(--weight-semibold);
    line-height: var(--leading-tight);
  }

  .markdown :global(h2) {
    font-size: var(--text-xl);
  }

  .markdown :global(h3) {
    font-size: var(--text-lg);
  }

  .markdown :global(:is(h4, h5, h6)) {
    font-size: var(--text-md);
  }

  .markdown :global(h6) {
    color: var(--color-text-muted);
  }

  .markdown :global(:is(ul, ol)) {
    padding-inline-start: var(--space-6);
  }

  .markdown :global(li > :is(ul, ol)) {
    margin: var(--space-1) 0 0;
  }

  .markdown :global(li + li) {
    margin-top: var(--space-1);
  }

  .markdown :global(li.task) {
    list-style: none;
  }

  /* The box sits where the bullet would; its frame is a full touch target on phones. */
  .markdown :global(.task-box) {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 1.25rem;
    height: 1.25rem;
    margin-inline: calc(-1 * var(--space-6)) var(--space-1);
    vertical-align: -0.2em;
  }

  .markdown :global(.task-box input) {
    width: 1rem;
    height: 1rem;
    margin: 0;
    accent-color: var(--color-ink);
  }

  .markdown :global(.task-box input:enabled) {
    cursor: pointer;
  }

  /* Touch screens: a 44 px frame; the item's line grows to hold it. */
  @media (pointer: coarse) {
    .markdown :global(.task-box) {
      width: var(--control-sm);
      height: var(--control-sm);
      margin-inline: calc(-1 * var(--control-sm)) 0;
      vertical-align: middle;
    }

    .markdown :global(.task-box input) {
      width: 1.25rem;
      height: 1.25rem;
    }
  }

  .markdown :global(blockquote) {
    padding-inline-start: var(--space-3);
    border-inline-start: 3px solid var(--color-border-control);
    color: var(--color-text-muted);
  }

  .markdown :global(hr) {
    border: none;
    border-top: 1px solid var(--color-border);
  }

  .markdown :global(del) {
    color: var(--color-text-muted);
  }

  .markdown :global(img) {
    max-width: 100%;
    height: auto;
    border-radius: var(--radius-sm);
  }

  /* Files of the text (CNT-044): pictures open the full file; videos play in place. */
  .markdown :global(a.file-image) {
    display: inline-block;
    max-width: 100%;
    vertical-align: top;
  }

  .markdown :global(a.file-image img) {
    display: block;
  }

  .markdown :global(video.file-video) {
    display: block;
    max-width: 100%;
    height: auto;
    border-radius: var(--radius-sm);
    background: var(--color-surface-sunken);
  }

  /* A name whose file is not there: muted, with the reason written out. */
  .markdown :global(.file-missing) {
    color: var(--color-text-muted);
  }

  .markdown :global(.file-note) {
    font-size: var(--text-sm);
  }

  .markdown :global(:not(pre) > code) {
    padding: 0.1em 0.3em;
    border-radius: var(--radius-sm);
    background: var(--code-bg);
    color: var(--code-text);
  }

  /* People and issues named in the text. */
  .markdown :global(a.mention) {
    font-weight: var(--weight-medium);
    text-decoration: none;
  }

  .markdown :global(a.mention:hover) {
    text-decoration: underline;
  }

  .markdown :global(a.issue-ref) {
    font-family: var(--font-mono);
    font-size: 0.95em;
  }

  /* A resolved issue is struck through and says "resolved" to screen readers (design §10). */
  .markdown :global(a.issue-ref.resolved) {
    text-decoration-line: line-through;
  }

  .markdown :global(.table-frame) {
    max-width: 100%;
    overflow-x: auto;
  }

  .markdown :global(table) {
    border-collapse: collapse;
    font-variant-numeric: tabular-nums;
  }

  .markdown :global(:is(th, td)) {
    padding: var(--space-1) var(--space-3);
    border: 1px solid var(--color-border);
    text-align: start;
    vertical-align: top;
  }

  .markdown :global(th) {
    background: var(--color-surface-sunken);
    font-weight: var(--weight-semibold);
  }

  .markdown :global(:is(th, td)[align='center']) {
    text-align: center;
  }

  .markdown :global(:is(th, td)[align='right']) {
    text-align: end;
  }

  .markdown :global(.code-block) {
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    background: var(--code-bg);
    color: var(--code-text);
  }

  .markdown :global(.code-head) {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2);
    padding: var(--space-0_5) var(--space-1) 0 var(--space-3);
  }

  .markdown :global(.code-language) {
    color: var(--color-text-muted);
    font-family: var(--font-mono);
    font-size: var(--text-xs);
  }

  .markdown :global(pre) {
    margin: 0;
    padding: 0 var(--space-3) var(--space-3);
    overflow-x: auto;
    line-height: var(--leading-normal);
    tab-size: 4;
  }

  .markdown :global(pre code) {
    font-size: var(--text-sm);
  }

  /* Highlighting (CNT-045): highlight.js classes on the code tokens of theme/tokens.css. */
  .markdown :global(:is(.hljs-keyword, .hljs-selector-tag, .hljs-built_in, .hljs-doctag)) {
    color: var(--code-keyword);
  }

  .markdown :global(:is(.hljs-string, .hljs-regexp, .hljs-template-tag, .hljs-char\.escape)) {
    color: var(--code-string);
  }

  .markdown :global(:is(.hljs-number, .hljs-literal, .hljs-symbol, .hljs-variable\.constant_)) {
    color: var(--code-number);
  }

  .markdown :global(:is(.hljs-comment, .hljs-quote)) {
    color: var(--code-comment);
  }

  .markdown
    :global(
      :is(.hljs-title, .hljs-section, .hljs-title\.function_, .hljs-title\.class_, .hljs-name)
    ) {
    color: var(--code-title);
  }

  .markdown :global(:is(.hljs-type, .hljs-class, .hljs-params, .hljs-variable\.language_)) {
    color: var(--code-type);
  }

  .markdown
    :global(
      :is(.hljs-attr, .hljs-attribute, .hljs-property, .hljs-selector-class, .hljs-selector-id)
    ) {
    color: var(--code-attribute);
  }

  .markdown :global(:is(.hljs-meta, .hljs-meta .hljs-keyword, .hljs-tag, .hljs-subst)) {
    color: var(--code-meta);
  }

  .markdown :global(.hljs-addition) {
    background: var(--color-success-bg);
    color: var(--color-success);
  }

  .markdown :global(.hljs-deletion) {
    background: var(--color-danger-bg);
    color: var(--color-danger);
  }

  .markdown :global(.hljs-emphasis) {
    font-style: italic;
  }

  .markdown :global(.hljs-strong) {
    font-weight: var(--weight-semibold);
  }
</style>
