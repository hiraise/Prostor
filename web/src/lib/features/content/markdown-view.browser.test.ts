// Rendered Markdown on a desktop and on a phone (step 1.4a): the server's output for the shared
// cases of crates/markup/tests/cases.json (CNT-040…CNT-045), its markers resolved for the viewer
// (CNT-044, CNT-054, CNT-056), code blocks with copy and highlighting (CNT-045) and checklist boxes that
// can be ticked (CNT-041).
import { afterEach, beforeAll, describe, expect, it, vi } from 'vitest';
import { commands, userEvent } from 'vitest/browser';
import { render } from 'vitest-browser-svelte';

import type { FileRef, TextRefs, UserRef } from '#lib/api/schema.gen.ts';
import { BASE_LOCALE, setLocale } from '#lib/i18n/locale.svelte.ts';
import { toasts } from '#lib/ui/toast.svelte.ts';

import { expectAccessible, isPhone, smallTargets } from '../../../test/a11y.ts';
import MarkdownView from './MarkdownView.svelte';

interface Case {
  name: string;
  text: string;
  html?: string;
}

type Cases = Record<string, Case[]>;

let cases: Cases = {};

beforeAll(async () => {
  // Paths of browser commands start at web/.
  cases = JSON.parse(await commands.readFile('../crates/markup/tests/cases.json')) as Cases;
});

/** The server's HTML of a shared case. */
function html(group: string, name: string): string {
  const found = cases[group]?.find((item) => item.name === name)?.html;
  if (found === undefined) throw new Error(`no case ${group}/${name}`);
  return found;
}

const JOHN: UserRef = {
  id: 57,
  full_name: 'John Smith',
  username: 'jsmith',
  avatar_color: 1,
  disabled: false,
  kind: 'person',
  pseudonym: null,
  avatar_url: null,
};

const NO_REFS: TextRefs = { users: [], issues: [], files: [] };

function file(name: string, kind: FileRef['kind'], extra: Partial<FileRef> = {}): FileRef {
  const url = `/test-files/${encodeURIComponent(name)}?sig=x`;
  return { name, kind, size: 1000, width: null, height: null, url, thumbnail_url: null, ...extra };
}

const SHOT = file('shot.png', 'image', {
  width: 1600,
  height: 900,
  thumbnail_url: '/test-files/shot.png/thumbnail?sig=x',
});
const LOG = file('build log.txt', 'other');
const NOTES = file('notes.md', 'other');
const CLIP = file('demo clip.mp4', 'video', { width: 1280, height: 720 });
const SPEC = file('spec.pdf', 'pdf');
const SCHEME = file('схема.svg', 'image');

afterEach(() => {
  vi.restoreAllMocks();
  toasts.clear();
  setLocale(BASE_LOCALE);
});

describe('MarkdownView', () => {
  it('CNT-040: emphasis, headings under the page’s own, breaks, rules, quotes and code', async () => {
    const screen = await render(MarkdownView, {
      html: [
        html('cnt_040', 'emphasis and strong'),
        html('cnt_040', 'atx headings'),
        html('cnt_040', 'quotes'),
        html('cnt_040', 'thematic breaks'),
        html('cnt_040', 'raw html is text'),
      ].join(''),
    });

    const root = screen.container;
    expect(root.querySelector('em')?.textContent).toBe('a');
    expect(root.querySelector('strong')?.textContent).toBe('c');
    expect(root.querySelector('del')?.textContent).toBe('f');
    expect(root.querySelector('p code')?.textContent).toBe('g');
    // `#` is a level-2 heading here; levels past 6 stay at 6.
    await expect.element(screen.getByRole('heading', { level: 2, name: 'One' })).toBeVisible();
    await expect.element(screen.getByRole('heading', { level: 3, name: 'Two' })).toBeVisible();
    await expect.element(screen.getByRole('heading', { level: 6, name: 'Six' })).toBeVisible();
    expect(screen.getByRole('heading', { level: 1 }).query()).toBeNull();
    expect(root.querySelectorAll('blockquote blockquote')).toHaveLength(1);
    expect(root.querySelectorAll('hr')).toHaveLength(3);
    await expect.element(screen.getByText('<b>bold</b>')).toBeVisible();
    await expectAccessible(root);
  });

  it('CNT-041: lists, nested and numbered, and checklists that only show their state', async () => {
    const screen = await render(MarkdownView, {
      html:
        html('cnt_041', 'ordered lists with a start') +
        html('cnt_041', 'checklists keep their lines'),
    });

    expect(screen.container.querySelector('ol')?.getAttribute('start')).toBe('3');
    const open = screen.getByRole('checkbox', { name: 'open' });
    await expect.element(open).not.toBeChecked();
    await expect.element(open).toBeDisabled();
    await expect.element(screen.getByRole('checkbox', { name: 'done' })).toBeChecked();
    await expect.element(screen.getByRole('checkbox', { name: 'nested' })).not.toBeChecked();
    await expectAccessible(screen.container);
  });

  it('CNT-042: tables keep their alignment and scroll sideways on their own', async () => {
    const screen = await render(MarkdownView, {
      html: html('cnt_042', 'tables with alignment'),
    });

    const frame = screen.getByRole('region', { name: 'Table' });
    await expect.element(frame).toBeVisible();
    await expect.element(frame.getByRole('columnheader', { name: 'C' })).toBeVisible();
    expect(screen.getByRole('cell', { name: 'c' }).element().getAttribute('align')).toBe('right');
    expect(frame.element().scrollWidth).toBeLessThanOrEqual(frame.element().clientWidth + 1);
    await expectAccessible(screen.container);
  });

  it('CNT-043: links open in a new tab without passing the page on; pictures load lazily', async () => {
    const screen = await render(MarkdownView, {
      html:
        html('cnt_043', 'inline links with titles') +
        html('cnt_043', 'bare urls and emails') +
        html('cnt_043', 'image by url'),
    });

    const site = screen.getByRole('link', { name: 'site' });
    await expect.element(site).toHaveAttribute('href', 'https://example.com');
    await expect.element(site).toHaveAttribute('target', '_blank');
    expect(site.element().getAttribute('rel')?.split(' ')).toEqual(
      expect.arrayContaining(['noopener', 'noreferrer', 'nofollow']),
    );
    const mail = screen.getByRole('link', { name: 'team@example.com' });
    expect(mail.element().getAttribute('target')).toBeNull();
    expect(screen.container.querySelector('img')?.getAttribute('loading')).toBe('lazy');
  });

  it('CNT-044: pictures show their thumbnail inside a link to the full file; other files download', async () => {
    const screen = await render(MarkdownView, {
      // The shared case without its paragraph of bare links, which are too close to tap.
      html:
        html('cnt_044', 'image and link to files').replace(/> <a.*<\/p>/, '></p>') +
        '<p>Output in <a href="tf-file:build%20log.txt" rel="noopener noreferrer nofollow">log</a>' +
        ', notes in <a href="tf-file:notes.md" rel="noopener noreferrer nofollow">ref</a>.</p>\n' +
        '<p><a href="https://x.example" rel="noopener noreferrer nofollow">' +
        '<img src="tf-file:shot.png" alt=""></a></p>\n',
      refs: { ...NO_REFS, files: [SHOT, LOG, NOTES] },
    });

    const full = screen.getByRole('link', { name: 'shot', exact: true });
    await expect.element(full).toHaveAttribute('href', SHOT.url);
    await expect.element(full).toHaveAttribute('target', '_blank');
    expect(full.element().hasAttribute('download')).toBe(false);
    const picture = full.element().querySelector('img');
    expect(picture?.getAttribute('src')).toBe(SHOT.thumbnail_url);
    expect(picture?.getAttribute('width')).toBe('1600');
    expect(picture?.getAttribute('height')).toBe('900');
    expect(picture?.getAttribute('loading')).toBe('lazy');
    // A wide picture fits the column, with its proportions known before it loads.
    const box = picture?.getBoundingClientRect();
    expect(box?.width).toBeLessThanOrEqual(screen.container.getBoundingClientRect().width + 0.5);
    expect(Math.round(((box?.height ?? 0) / (box?.width ?? 1)) * 16)).toBe(9);

    const log = screen.getByRole('link', { name: 'log' });
    await expect.element(log).toHaveAttribute('href', LOG.url);
    await expect.element(log).toHaveAttribute('download', 'build log.txt');
    expect(log.element().getAttribute('target')).toBeNull();
    await expect.element(screen.getByRole('link', { name: 'ref' })).toHaveAttribute('download');

    // A picture inside a link of the text keeps that link and is not wrapped again.
    const site = screen.getByRole('link', { name: 'shot.png' });
    await expect.element(site).toHaveAttribute('href', 'https://x.example');
    expect(site.element().querySelector('img')?.getAttribute('src')).toBe(SHOT.thumbnail_url);
    expect(screen.container.querySelectorAll('a a')).toHaveLength(0);
    expect(document.documentElement.scrollWidth).toBeLessThanOrEqual(window.innerWidth);
    await expectAccessible(screen.container);
  });

  it('CNT-044: videos play in place; PDFs open in a new tab; pictures without a thumbnail use the file', async () => {
    const screen = await render(MarkdownView, {
      html:
        '<p><img src="tf-file:demo%20clip.mp4" alt=""></p>\n' +
        '<p><img src="tf-file:spec.pdf" alt="Spec"> and <a href="tf-file:spec.pdf" ' +
        'rel="noopener noreferrer nofollow">the spec</a></p>\n' +
        '<p><img src="tf-file:%D1%81%D1%85%D0%B5%D0%BC%D0%B0.svg" alt=""></p>\n',
      refs: { ...NO_REFS, files: [CLIP, SPEC, SCHEME] },
    });

    const player = screen.container.querySelector('video');
    expect(player?.getAttribute('src')).toBe(CLIP.url);
    expect(player?.controls).toBe(true);
    expect(player?.getAttribute('preload')).toBe('metadata');
    expect(player?.getAttribute('aria-label')).toBe('demo clip.mp4');
    expect(player?.getAttribute('width')).toBe('1280');
    expect((player?.getBoundingClientRect().width ?? 0) - 0.5).toBeLessThanOrEqual(
      screen.container.getBoundingClientRect().width,
    );

    for (const name of ['Spec', 'the spec']) {
      const link = screen.getByRole('link', { name });
      await expect.element(link).toHaveAttribute('href', SPEC.url);
      await expect.element(link).toHaveAttribute('target', '_blank');
      expect(link.element().hasAttribute('download')).toBe(false);
    }

    const scheme = screen.getByRole('link', { name: 'схема.svg' });
    await expect.element(scheme).toHaveAttribute('href', SCHEME.url);
    const image = scheme.element().querySelector('img');
    expect(image?.getAttribute('src')).toBe(SCHEME.url);
    expect(image?.hasAttribute('width')).toBe(false);
    await expectAccessible(screen.container);
  });

  it('CNT-044: a file the viewer cannot see shows its name marked as missing', async () => {
    const screen = await render(MarkdownView, {
      html:
        html('cnt_044', 'image and link to files') +
        '<p><img src="tf-file:lost%20one.png" alt=""></p>\n',
      refs: { ...NO_REFS, files: [NOTES] },
    });

    expect(screen.container.querySelector('img')).toBeNull();
    expect(screen.getByRole('link').elements()).toHaveLength(1);
    expect(screen.container.textContent.replace(/\s+/g, ' ').trim()).toBe(
      'shot (file “shot.png” not found) log (file “build log.txt” not found) ref ' +
        'lost one.png (file not found)',
    );
    const note = screen.getByText('(file not found)').element();
    expect(note.checkVisibility()).toBe(true);
    await expectAccessible(screen.container);

    setLocale('ru');
    const ru = await render(MarkdownView, {
      html: '<p><img src="tf-file:shot.png" alt=""></p>\n',
      refs: NO_REFS,
    });
    expect(ru.container.textContent.trim()).toBe('shot.png (файл не найден)');
  });

  it('CNT-045: code blocks name their language, highlight it and copy it', async () => {
    const writeText = vi.spyOn(navigator.clipboard, 'writeText').mockResolvedValue(undefined);
    const screen = await render(MarkdownView, {
      html:
        html('cnt_045', 'language class') +
        html('cnt_045', 'aliases and symbols') +
        '<pre><code class="language-text">fn plain() {}\n</code></pre>\n',
    });

    const rust = screen.getByRole('region', { name: 'Code, rust' });
    await expect.element(rust).toBeVisible();
    await expect.element(screen.getByRole('region', { name: 'Code, vb.net' })).toBeVisible();
    await expect.element(screen.getByRole('region', { name: 'Code, c++' })).toBeVisible();
    // Highlighting loads on the first code block.
    await vi.waitFor(() => {
      expect(rust.element().querySelector('.hljs-keyword')?.textContent).toBe('fn');
    }, 10_000);
    await vi.waitFor(() => {
      expect(
        screen
          .getByRole('region', { name: 'Code, vb.net' })
          .element()
          .querySelector('.hljs-keyword'),
      ).not.toBeNull();
    });
    // `text` stays plain.
    const plain = screen.getByRole('region', { name: 'Code, text' }).element();
    expect(plain.querySelector('[class^="hljs-"]')).toBeNull();

    const copies = screen.getByRole('button', { name: 'Copy code' });
    expect(copies.elements()).toHaveLength(4);
    await copies.first().click();
    expect(writeText).toHaveBeenCalledWith('fn main() {}');
    expect(toasts.items.map((toast) => toast.message)).toContain('Code copied');
    expect(smallTargets(screen.container, isPhone() ? 44 : 32)).toEqual([]);
    await expectAccessible(screen.container);
  });

  it('CNT-054: mentions show the person’s name and lead to them; unknown ones stay text', async () => {
    const screen = await render(MarkdownView, {
      html: html('cnt_054', 'known mention') + html('cnt_054', 'longest login wins'),
      refs: { users: [JOHN], issues: [], files: [] },
    });

    const john = screen.getByRole('link', { name: 'John Smith' });
    await expect.element(john).toHaveAttribute('href', '/users/57');
    await expect.element(john).toHaveAttribute('title', '@jsmith');
    // People of the second case are not in the references: their text stays.
    await expect.element(screen.getByText('@ann.lee and @ann')).toBeVisible();
    expect(screen.getByRole('link').elements()).toHaveLength(1);
  });

  it('CNT-056: issue IDs lead to issues the viewer may see; resolved ones are struck through', async () => {
    const screen = await render(MarkdownView, {
      html: html('cnt_056', 'tokens'),
      refs: {
        users: [],
        files: [],
        issues: [
          {
            token: 'WEB-12',
            id: 4,
            readable_id: 'SITE-12',
            summary: 'Login fails',
            resolved: true,
          },
        ],
      },
    });

    const links = screen.getByRole('link', { name: 'SITE-12 (resolved)' });
    expect(links.elements()).toHaveLength(2);
    await expect.element(links.first()).toHaveAttribute('href', '/issues/SITE-12');
    await expect.element(links.first()).toHaveAttribute('title', 'Login fails');
    expect(getComputedStyle(links.first().element()).textDecorationLine).toBe('line-through');
    // Not visible to the viewer, or unknown: plain text.
    expect(screen.container.textContent).toContain('see A_1-7 and');
    await expectAccessible(screen.container);
  });

  it('CNT-054, CNT-056: markers without references stay text', async () => {
    const screen = await render(MarkdownView, {
      html: html('cnt_054', 'known mention') + html('cnt_056', 'tokens'),
      refs: NO_REFS,
    });

    expect(screen.getByRole('link').query()).toBeNull();
    expect(screen.container.textContent).toContain('ask @jsmith please');
    expect(screen.container.textContent).toContain('Fixed in WEB-12, see A_1-7 and WEB-12.');
  });

  it('CNT-041: with a handler the boxes can be ticked, and a failed save puts them back', async () => {
    let fail = false;
    const ontoggle = vi.fn(() => (fail ? Promise.reject(new Error('no')) : Promise.resolve()));
    const screen = await render(MarkdownView, {
      html: html('cnt_041', 'checklists keep their lines'),
      ontoggle,
    });

    const open = screen.getByRole('checkbox', { name: 'open' });
    await expect.element(open).toBeEnabled();
    await open.click();
    expect(ontoggle).toHaveBeenLastCalledWith(1, true);
    await expect.element(open).toBeChecked();

    fail = true;
    await screen.getByRole('checkbox', { name: 'done' }).click();
    expect(ontoggle).toHaveBeenLastCalledWith(2, false);
    await expect.element(screen.getByRole('checkbox', { name: 'done' })).toBeChecked();

    // Keyboard: Space on the focused box.
    fail = false;
    const nested = screen.getByRole('checkbox', { name: 'nested' });
    (nested.element() as HTMLInputElement).focus();
    await userEvent.keyboard(' ');
    expect(ontoggle).toHaveBeenLastCalledWith(3, true);
    await expect.element(nested).toBeChecked();
  });

  it('CNT-040: a long text reads well on a phone and has no small targets', async () => {
    const screen = await render(MarkdownView, {
      html: Object.values(cases)
        .flat()
        // One-letter links side by side are too close to tap (a writing problem, not ours).
        .filter((item) => item.name !== 'paths, anchors and urls are not files')
        .map((item) => item.html ?? '')
        .join(''),
      refs: { users: [JOHN], issues: [], files: [] },
      ontoggle: () => undefined,
    });

    expect(document.documentElement.scrollWidth).toBeLessThanOrEqual(window.innerWidth);
    if (isPhone()) {
      for (const box of screen.getByRole('checkbox').elements()) {
        const frame = box.closest('label')?.getBoundingClientRect();
        expect(frame?.width).toBeGreaterThanOrEqual(44);
        expect(frame?.height).toBeGreaterThanOrEqual(44);
      }
    }
    // Links inside sentences are exempt from target sizes (WCAG 2.5.8); buttons are not.
    for (const button of screen.getByRole('button').elements()) {
      const box = button.getBoundingClientRect();
      expect(Math.min(box.width, box.height)).toBeGreaterThanOrEqual(isPhone() ? 44 : 24);
    }
    await expectAccessible(screen.container);
  });

  it('CNT-045: Russian names for code and tables', async () => {
    setLocale('ru');
    const screen = await render(MarkdownView, {
      html: html('cnt_045', 'language class') + html('cnt_042', 'tables without outer pipes'),
    });

    await expect.element(screen.getByRole('region', { name: 'Код, rust' })).toBeVisible();
    await expect.element(screen.getByRole('region', { name: 'Таблица' })).toBeVisible();
    await expect.element(screen.getByRole('button', { name: 'Скопировать код' })).toBeVisible();
  });
});
