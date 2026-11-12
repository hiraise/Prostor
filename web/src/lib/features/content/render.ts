// Turns the server's rendered Markdown (D-031) into the page's content. The HTML is already
// sanitized; here its markers become people, issues and file names for this viewer, code blocks
// get a frame with a copy button and highlighting, tables scroll on narrow screens, and checklist
// boxes become usable when the text can be changed. The HTML is parsed in an inert <template>, so
// nothing in it loads (a `tf-file:` picture would) before it is changed. File markers become
// pictures, videos and links to the viewer's signed addresses (CNT-044); every value put into the
// page goes through DOM properties, never through HTML.
import Copy from '@lucide/svelte/icons/copy';
import { mount, unmount } from 'svelte';

import { resolve } from '$app/paths';
import type { FileRef, IssueMention, TextRefs, UserRef } from '#lib/api/schema.gen.ts';
import { personName, personUsername } from '#lib/features/access/index.ts';
import IconButton from '#lib/ui/IconButton.svelte';
import { copyText } from '#lib/ui/copy.ts';

import { markerName } from './files.ts';
import { codeLanguage, highlightCode, isHighlighted } from './highlight.ts';
import * as m from './messages.ts';
import { sourceLine } from './tasks.ts';

export interface RenderOptions {
  refs?: TextRefs | undefined;
  /** Makes checklist boxes usable; a rejected promise puts the box back. */
  ontoggle?: ((line: number, checked: boolean) => unknown) | undefined;
  /** The level a `#` heading of the text gets on the page (the page has its own h1). */
  headingLevel: number;
}

const USER = 'tf-user:';
const ISSUE = 'tf-issue:';

function userHref(id: number): string {
  return resolve('/(app)/users/[id]', { id: String(id) });
}

function issueHref(readableId: string): string {
  // The issue page arrives with step 1.7 at this address.
  return `${resolve('/(app)/issues')}/${encodeURIComponent(readableId)}`;
}

function decoded(value: string): string {
  try {
    return decodeURIComponent(value);
  } catch {
    return value;
  }
}

function replaceWithText(element: Element, text: string): void {
  element.replaceWith(document.createTextNode(text));
}

function mention(link: HTMLAnchorElement, person: UserRef | undefined): void {
  if (!person) {
    replaceWithText(link, link.textContent);
    return;
  }
  link.href = userHref(person.id);
  link.className = 'mention';
  link.textContent = personName(person);
  const username = personUsername(person);
  if (username) link.title = `@${username}`;
}

function issueLink(link: HTMLAnchorElement, issue: IssueMention | undefined): void {
  if (!issue) {
    replaceWithText(link, link.textContent);
    return;
  }
  link.href = issueHref(issue.readable_id);
  link.className = issue.resolved ? 'issue-ref resolved' : 'issue-ref';
  link.title = issue.summary;
  link.textContent = issue.readable_id;
  if (issue.resolved) {
    const marker = document.createElement('span');
    marker.className = 'sr-only';
    marker.textContent = ` (${m.resolved()})`;
    link.append(marker);
  }
}

function enhanceLinks(root: DocumentFragment, refs: TextRefs | undefined): void {
  for (const link of root.querySelectorAll<HTMLAnchorElement>('a[href]')) {
    const href = link.getAttribute('href') ?? '';
    if (href.startsWith(USER)) {
      const id = Number(href.slice(USER.length));
      mention(
        link,
        refs?.users.find((user) => user.id === id),
      );
    } else if (href.startsWith(ISSUE)) {
      const token = decoded(href.slice(ISSUE.length));
      issueLink(
        link,
        refs?.issues.find((issue) => issue.token === token),
      );
    } else if (markerName(href) !== null) {
      fileLink(link, markerName(href) ?? '', refs);
    } else if (/^https?:/i.test(href)) {
      link.target = '_blank';
      const rel = new Set(link.rel.split(/\s+/).filter(Boolean));
      rel.add('noopener');
      rel.add('noreferrer');
      link.rel = [...rel].join(' ');
    }
  }
}

function findFile(refs: TextRefs | undefined, name: string): FileRef | undefined {
  return refs?.files.find((file) => file.name === name);
}

/** A name whose file the viewer cannot see, or that is not attached: text marked as missing. */
function missing(label: Node[], name: string, shownName: boolean): HTMLElement {
  const span = document.createElement('span');
  span.className = 'file-missing';
  span.append(...label);
  const note = document.createElement('span');
  note.className = 'file-note';
  note.textContent = ` (${shownName ? m.fileMissing() : m.namedFileMissing({ name })})`;
  span.append(note);
  return span;
}

/** Opens in a new tab what the browser shows itself; other files download. */
function pointAtFile(link: HTMLAnchorElement, file: FileRef): void {
  link.href = file.url;
  link.removeAttribute('rel');
  if (file.kind === 'other') {
    link.download = file.name;
  } else {
    link.target = '_blank';
    link.rel = 'noopener noreferrer';
  }
}

function fileLink(link: HTMLAnchorElement, name: string, refs: TextRefs | undefined): void {
  const file = findFile(refs, name);
  if (!file) {
    const text = link.textContent.trim();
    link.replaceWith(missing([...link.childNodes], name, text === name || text === ''));
    return;
  }
  pointAtFile(link, file);
  link.classList.add('file-link');
  if (link.textContent.trim() === '') link.textContent = file.name;
}

function picture(image: HTMLImageElement, file: FileRef, name: string): void {
  image.src = file.thumbnail_url ?? file.url;
  image.alt = image.alt || name;
  if (file.width !== null && file.height !== null) {
    image.width = file.width;
    image.height = file.height;
  }
  image.loading = 'lazy';
  image.decoding = 'async';
  // Inside a link of the text the picture keeps that link; otherwise it opens the full file.
  if (image.closest('a')) return;
  const link = document.createElement('a');
  link.className = 'file-image';
  pointAtFile(link, file);
  image.replaceWith(link);
  link.append(image);
}

function video(image: HTMLImageElement, file: FileRef, name: string): void {
  const player = document.createElement('video');
  player.className = 'file-video';
  player.controls = true;
  player.preload = 'metadata';
  player.src = file.url;
  player.setAttribute('aria-label', image.alt || name);
  if (file.width !== null && file.height !== null) {
    player.width = file.width;
    player.height = file.height;
  }
  // Shown only by browsers that cannot play it.
  const fallback = document.createElement('a');
  fallback.href = file.url;
  fallback.textContent = name;
  player.append(fallback);
  image.replaceWith(player);
}

function fileImage(image: HTMLImageElement, name: string, refs: TextRefs | undefined): void {
  const file = findFile(refs, name);
  const label = image.alt || name;
  if (!file) {
    image.replaceWith(missing([document.createTextNode(label)], name, label === name));
  } else if (file.kind === 'image') {
    picture(image, file, name);
  } else if (file.kind === 'video' && !image.closest('a')) {
    video(image, file, name);
  } else if (image.closest('a')) {
    // A player or a second link cannot sit inside a link.
    replaceWithText(image, label);
  } else {
    // A PDF or another file written as a picture: a link to it.
    const link = document.createElement('a');
    link.className = 'file-link';
    link.textContent = label;
    pointAtFile(link, file);
    image.replaceWith(link);
  }
}

function enhanceImages(root: DocumentFragment, refs: TextRefs | undefined): void {
  for (const image of root.querySelectorAll('img')) {
    const name = markerName(image.getAttribute('src') ?? '');
    if (name !== null) {
      fileImage(image, name, refs);
    } else {
      image.loading = 'lazy';
      image.decoding = 'async';
    }
  }
}

function shiftHeadings(root: DocumentFragment, level: number): void {
  if (level === 1) return;
  for (const heading of root.querySelectorAll('h1, h2, h3, h4, h5, h6')) {
    const own = Number(heading.tagName.slice(1));
    const shifted = Math.min(6, own + level - 1);
    const replacement = document.createElement(`h${String(shifted)}`);
    replacement.append(...heading.childNodes);
    heading.replaceWith(replacement);
  }
}

function frameTables(root: DocumentFragment): void {
  for (const table of root.querySelectorAll('table')) {
    const frame = document.createElement('div');
    frame.className = 'table-frame';
    // Scrolls sideways on narrow screens, so the keyboard must reach it (WCAG 2.1.1).
    frame.tabIndex = 0;
    frame.setAttribute('role', 'region');
    frame.setAttribute('aria-label', m.table());
    for (const header of table.querySelectorAll('thead th')) header.setAttribute('scope', 'col');
    table.replaceWith(frame);
    frame.append(table);
  }
}

interface CodeBlock {
  code: HTMLElement;
  language: string | null;
  actions: HTMLElement;
}

function frameCode(root: DocumentFragment): CodeBlock[] {
  const blocks: CodeBlock[] = [];
  for (const pre of root.querySelectorAll('pre')) {
    const code = pre.querySelector('code');
    if (!code) continue;
    const language = codeLanguage(code);
    const frame = document.createElement('div');
    frame.className = 'code-block';
    const head = document.createElement('div');
    head.className = 'code-head';
    const label = document.createElement('span');
    label.className = 'code-language';
    label.textContent = language ?? '';
    const actions = document.createElement('span');
    actions.className = 'code-actions';
    head.append(label, actions);
    pre.replaceWith(frame);
    // Long lines scroll sideways, so the keyboard must reach the code (WCAG 2.1.1).
    pre.tabIndex = 0;
    pre.setAttribute('role', 'region');
    pre.setAttribute('aria-label', language ? m.codeIn({ language }) : m.code());
    frame.append(head, pre);
    blocks.push({ code, language, actions });
  }
  return blocks;
}

/** A list item's own text, without its nested lists. */
function itemText(item: Element): string {
  let text = '';
  for (const node of item.childNodes) {
    if (node instanceof Element && (node.tagName === 'UL' || node.tagName === 'OL')) continue;
    text += node.textContent ?? '';
  }
  return text.replace(/\s+/g, ' ').trim();
}

function enableTasks(root: DocumentFragment, ontoggle: RenderOptions['ontoggle']): void {
  for (const box of root.querySelectorAll<HTMLInputElement>('li > input[type="checkbox"]')) {
    const item = box.parentElement;
    if (!item) continue;
    item.classList.add('task');
    box.setAttribute('aria-label', itemText(item));
    // The whole frame around the box takes taps (D-019).
    const frame = document.createElement('label');
    frame.className = 'task-box';
    box.replaceWith(frame);
    frame.append(box);
    const line = sourceLine(item.getAttribute('data-sourcepos'));
    if (!ontoggle || line === null) continue;
    box.disabled = false;
    box.removeAttribute('disabled');
    box.addEventListener('change', () => {
      const checked = box.checked;
      Promise.resolve()
        .then(() => ontoggle(line, checked))
        .catch(() => {
          box.checked = !checked;
        });
    });
  }
}

/** Puts `html` into `container`; returns a function that removes what it mounted. */
export function renderMarkdown(
  container: HTMLElement,
  html: string,
  options: RenderOptions,
): () => void {
  const template = document.createElement('template');
  template.innerHTML = html;
  const root = template.content;
  enhanceLinks(root, options.refs);
  enhanceImages(root, options.refs);
  shiftHeadings(root, options.headingLevel);
  frameTables(root);
  enableTasks(root, options.ontoggle);
  const blocks = frameCode(root);
  // A box ticked from the keyboard keeps the focus when the saved text comes back.
  const focused = document.activeElement;
  const focusedLine =
    focused instanceof HTMLInputElement && container.contains(focused)
      ? focused.closest('li')?.getAttribute('data-sourcepos')
      : null;
  container.replaceChildren(root);
  if (focusedLine) {
    container
      .querySelector<HTMLInputElement>(
        `li[data-sourcepos="${CSS.escape(focusedLine)}"] > .task-box > input`,
      )
      ?.focus();
  }

  const mounted = blocks.map(({ code, language, actions }) => {
    const text = code.textContent;
    if (isHighlighted(language)) {
      // Without highlighting the code is still readable: nothing to report.
      highlightCode(code, language).catch(() => undefined);
    }
    return mount(IconButton, {
      target: actions,
      props: {
        icon: Copy,
        label: m.copyCode(),
        size: 'md',
        onclick: () => {
          void copyText(text.replace(/\n$/, ''), m.codeCopied());
        },
      },
    });
  });
  return () => {
    for (const component of mounted) void unmount(component);
  };
}
