// The Markdown editor on a desktop and on a phone (step 1.4a): formatting buttons and shortcuts
// (CNT-029), the preview as the server renders it, the list of people to mention (CNT-054), and
// files pasted or dropped into the field (CNT-034, step 1.4b).
import { afterEach, describe, expect, it, vi } from 'vitest';
import { userEvent } from 'vitest/browser';
import { render } from 'vitest-browser-svelte';

import type { MarkdownPreview, Upload, UserRef } from '#lib/api/schema.gen.ts';
import { BASE_LOCALE, setLocale } from '#lib/i18n/locale.svelte.ts';
import { preferences, setPreferencesForTests } from '#lib/session/index.ts';
import { announcement } from '#lib/ui/announce.svelte.ts';

import { expectAccessible, isPhone, smallTargets } from '../../../test/a11y.ts';
import { failure, fakeApi } from '../../../test/fake-api.ts';
import MarkdownEditor from './MarkdownEditor.svelte';

function person(id: number, full_name: string, username: string): UserRef {
  return {
    id,
    full_name,
    username,
    avatar_color: id % 8,
    disabled: false,
    kind: 'person',
    pseudonym: null,
    avatar_url: null,
  };
}

const JOHN = person(57, 'John Smith', 'jsmith');
const JOAN = person(58, 'Joan Lee', 'joan');

function field(screen: Awaited<ReturnType<typeof render>>): HTMLTextAreaElement {
  const element = screen.getByRole('textbox').element();
  if (!(element instanceof HTMLTextAreaElement)) throw new Error('no text field');
  return element;
}

/** Selects `part` of the field's text. */
function select(area: HTMLTextAreaElement, part: string): void {
  const start = area.value.indexOf(part);
  area.focus();
  area.setSelectionRange(start, start + part.length);
}

/** An upload answer for `file`, as the server gives it. */
function uploaded(id: number, file: File): Upload {
  const kind = file.type.startsWith('image/')
    ? 'image'
    : file.type.startsWith('video/')
      ? 'video'
      : file.type === 'application/pdf'
        ? 'pdf'
        : 'other';
  return {
    id,
    name: file.name,
    size: file.size,
    content_type: file.type,
    kind,
    url: `/api/v1/uploads/${String(id)}/${encodeURIComponent(file.name)}`,
    created_at: '2026-10-22T10:00:00Z',
  };
}

/** A fake upload that answers when the test lets it. */
function pendingUploads() {
  const waiting: { file: File; resolve: (upload: Upload) => void; reject: (e: unknown) => void }[] =
    [];
  const upload = vi.fn(
    (file: File) =>
      new Promise<Upload>((resolve, reject) => {
        waiting.push({ file, resolve, reject });
      }),
  );
  return { upload, waiting };
}

function transfer(...files: File[]): DataTransfer {
  const data = new DataTransfer();
  for (const file of files) data.items.add(file);
  return data;
}

function paste(area: HTMLTextAreaElement, data: DataTransfer): boolean {
  return area.dispatchEvent(
    new ClipboardEvent('paste', { clipboardData: data, bubbles: true, cancelable: true }),
  );
}

const PNG = new File(['png'], 'my shot.png', { type: 'image/png' });
const LOG = new File(['log'], 'build [1].log', { type: 'text/plain' });
const CLIP = new File(['mp4'], 'clip.mp4', { type: 'video/mp4' });

afterEach(() => {
  vi.unstubAllGlobals();
  setPreferencesForTests(null);
  setLocale(BASE_LOCALE);
});

describe('MarkdownEditor', () => {
  it('CNT-029: the toolbar formats the selection as one undoable step and moves by arrows', async () => {
    const screen = await render(MarkdownEditor, { label: 'Description', value: 'hello world' });
    const area = field(screen);

    select(area, 'world');
    await screen.getByRole('button', { name: 'Bold' }).click();
    expect(area.value).toBe('hello **world**');
    expect(area.value.slice(area.selectionStart, area.selectionEnd)).toBe('world');
    // The browser's undo takes the whole step back.
    await userEvent.keyboard('{ControlOrMeta>}z{/ControlOrMeta}');
    expect(area.value).toBe('hello world');

    select(area, 'hello world');
    await screen.getByRole('button', { name: 'Checklist' }).click();
    expect(area.value).toBe('- [ ] hello world');

    // One stop in the tab order; arrows, Home and End move inside.
    const toolbar = screen.getByRole('toolbar', { name: 'Formatting of “Description”' });
    const buttons = toolbar.getByRole('button').elements();
    expect(buttons.map((button) => button.getAttribute('aria-label'))).toEqual([
      'Heading',
      'Bold',
      'Italic',
      'Strikethrough',
      'Inline code',
      'Link',
      'Quote',
      'Bulleted list',
      'Numbered list',
      'Checklist',
      'Code block',
      'Table',
    ]);
    expect(buttons.filter((button) => button.tabIndex === 0)).toHaveLength(1);
    buttons[0]?.focus();
    await userEvent.keyboard('{ArrowRight}');
    expect(document.activeElement?.getAttribute('aria-label')).toBe('Bold');
    await userEvent.keyboard('{End}');
    expect(document.activeElement?.getAttribute('aria-label')).toBe('Table');
    await userEvent.keyboard('{ArrowRight}');
    expect(document.activeElement?.getAttribute('aria-label')).toBe('Heading');
    await userEvent.keyboard('{ArrowLeft}');
    expect(document.activeElement?.getAttribute('aria-label')).toBe('Table');
    expect(buttons.filter((button) => button.tabIndex === 0).map((b) => b.ariaLabel)).toEqual([
      'Table',
    ]);
    await expect
      .element(screen.getByRole('button', { name: 'Bold' }))
      .toHaveAttribute('aria-keyshortcuts', 'Control+B Meta+B');

    expect(smallTargets(screen.container, isPhone() ? 44 : 32)).toEqual([]);
    await expectAccessible(screen.container);
  });

  it('CNT-029: the toolbar hides and shows, remembered as a personal setting (UI-018)', async () => {
    const first = await render(MarkdownEditor, { label: 'Description' });
    await first.getByRole('button', { name: 'Hide formatting buttons' }).click();
    expect(first.getByRole('toolbar').query()).toBeNull();
    expect(preferences.value('editor_toolbar')).toBe(false);
    await first.unmount();

    const second = await render(MarkdownEditor, { label: 'Description' });
    expect(second.getByRole('toolbar').query()).toBeNull();
    await second.getByRole('button', { name: 'Show formatting buttons' }).click();
    await expect.element(second.getByRole('toolbar')).toBeVisible();
    expect(preferences.chosen('editor_toolbar')).toBeUndefined();
  });

  it('CNT-029: shortcuts for bold, italic, code, link and strikethrough; Ctrl+Enter submits', async () => {
    const onsubmit = vi.fn();
    const screen = await render(MarkdownEditor, {
      label: 'Description',
      value: 'one two three four five',
      onsubmit,
    });
    const area = field(screen);

    select(area, 'one');
    await userEvent.keyboard('{Control>}b{/Control}');
    select(area, 'two');
    await userEvent.keyboard('{Meta>}i{/Meta}');
    select(area, 'three');
    await userEvent.keyboard('{Control>}e{/Control}');
    select(area, 'four');
    await userEvent.keyboard('{Control>}k{/Control}');
    expect(area.selectionStart).toBe(area.value.indexOf('[four](') + '[four]('.length);
    select(area, 'five');
    await userEvent.keyboard('{Control>}{Shift>}x{/Shift}{/Control}');
    expect(area.value).toBe('**one** _two_ `three` [four]() ~~five~~');

    expect(onsubmit).not.toHaveBeenCalled();
    await userEvent.keyboard('{Control>}{Enter}{/Control}');
    expect(onsubmit).toHaveBeenCalledOnce();
    expect(area.value).toBe('**one** _two_ `three` [four]() ~~five~~');
  });

  it('CNT-029: the preview shows the text as the server renders it', async () => {
    const calls = fakeApi({
      'POST /markdown': ({ body }) => {
        const { text } = body as MarkdownPreview;
        if (text.includes('broken')) return failure(500, 'internal');
        return {
          body: {
            html: `<h1>${text.replace(/^# /, '')}</h1>\n`,
            refs: { users: [], issues: [], files: [] },
          },
        };
      },
    });
    const screen = await render(MarkdownEditor, {
      label: 'Description',
      profile: 'basic',
      hint: 'Shown on the overview.',
    });

    await expect
      .element(screen.getByText('Shown on the overview. Markdown is supported.'))
      .toBeVisible();
    const tabs = screen.getByRole('tablist', { name: 'Description: text or preview' });
    const write = tabs.getByRole('tab', { name: 'Text' });
    const preview = tabs.getByRole('tab', { name: 'Preview' });
    await expect.element(write).toHaveAttribute('aria-selected', 'true');

    await preview.click();
    await expect.element(screen.getByText('Nothing to preview yet.')).toBeVisible();
    expect(calls).toHaveLength(0);

    await write.click();
    await screen.getByRole('textbox', { name: /^Description/ }).fill('# Plans');
    await preview.click();
    // Headings sit under the field's own label.
    await expect.element(screen.getByRole('heading', { level: 3, name: 'Plans' })).toBeVisible();
    expect(calls.map((call) => call.body)).toEqual([
      { text: '# Plans', profile: 'basic', issue: null },
    ]);
    expect(screen.getByRole('textbox', { name: /^Description/ }).query()).toBeNull();

    // Arrows switch between the tabs; the same text is not asked for again.
    (preview.element() as HTMLElement).focus();
    await userEvent.keyboard('{ArrowLeft}');
    await expect.element(write).toHaveAttribute('aria-selected', 'true');
    await expect.element(write).toHaveFocus();
    await userEvent.keyboard('{ArrowRight}');
    await expect.element(screen.getByRole('heading', { name: 'Plans' })).toBeVisible();
    expect(calls).toHaveLength(1);
    await expectAccessible(screen.container);

    await write.click();
    await screen.getByRole('textbox', { name: /^Description/ }).fill('broken');
    await preview.click();
    await expect.element(screen.getByText('Something went wrong', { exact: false })).toBeVisible();
  });

  it('CNT-054: people to mention, chosen by keyboard or by tap', async () => {
    const calls = fakeApi({
      'GET /issues/7/mention-candidates': ({ query }) => ({
        body: {
          users: [JOHN, JOAN].filter((user) => {
            const typed = (query.get('q') ?? '').toLowerCase();
            return (
              user.username.startsWith(typed) || user.full_name.toLowerCase().startsWith(typed)
            );
          }),
        },
      }),
    });
    const screen = await render(MarkdownEditor, { label: 'Description', issue: 7 });
    const textbox = screen.getByRole('textbox', { name: /^Description/ });
    await textbox.click();

    await userEvent.keyboard('Hi @jo');
    const list = screen.getByRole('listbox', { name: 'People to mention' });
    await expect.element(list).toBeVisible();
    // Under load the list may show before the last request is read: wait for it.
    await expect.poll(() => calls.at(-1)?.query.get('q')).toBe('jo');
    const options = list.getByRole('option');
    await expect.element(options.first()).toHaveAttribute('aria-selected', 'true');
    await expect
      .element(textbox)
      .toHaveAttribute('aria-activedescendant', options.first().element().id);
    await expect.element(screen.getByText('2 people found')).toBeInTheDocument();

    await userEvent.keyboard('{ArrowDown}');
    await expect.element(options.nth(1)).toHaveAttribute('aria-selected', 'true');
    await expect
      .element(textbox)
      .toHaveAttribute('aria-activedescendant', options.nth(1).element().id);
    await expectAccessible(screen.container);
    await userEvent.keyboard('{Enter}');
    await expect.element(list).not.toBeInTheDocument();
    await expect.element(textbox).toHaveValue('Hi @joan ');

    await userEvent.keyboard('and @js');
    await expect.element(list).toBeVisible();
    await list.getByRole('option', { name: /John Smith/ }).click();
    await expect.element(textbox).toHaveValue('Hi @joan and @jsmith ');
    await expect.element(textbox).toHaveFocus();

    await userEvent.keyboard('@j');
    await expect.element(list).toBeVisible();
    if (isPhone()) {
      for (const option of list.getByRole('option').elements()) {
        expect(option.getBoundingClientRect().height).toBeGreaterThanOrEqual(44);
      }
    }
    await userEvent.keyboard('{Escape}');
    await expect.element(list).not.toBeInTheDocument();
    await expect.element(textbox).toHaveValue('Hi @joan and @jsmith @j');
  });

  it('CNT-045: the language of a code block, from the button or after a typed fence', async () => {
    fakeApi({});
    const screen = await render(MarkdownEditor, { label: 'Description' });
    const textbox = screen.getByRole('textbox', { name: /^Description/ });
    await textbox.click();

    await screen.getByRole('button', { name: 'Code block' }).click();
    const list = screen.getByRole('listbox', { name: 'Code language' });
    await expect.element(list).toBeVisible();
    await expect.element(textbox).toHaveFocus();
    await userEvent.keyboard('ru');
    await expect.element(list.getByRole('option')).toHaveLength(2);
    await expect.element(screen.getByText('2 languages')).toBeInTheDocument();
    await userEvent.keyboard('{ArrowDown}{Enter}');
    await expect.element(list).not.toBeInTheDocument();
    await expect.element(textbox).toHaveValue('```rust\n\n```');
    // The caret is on the code line.
    await userEvent.keyboard('fn main()');
    await expect.element(textbox).toHaveValue('```rust\nfn main()\n```');

    // Typed at the start of a line, chosen by tap; a closing fence offers nothing.
    await userEvent.keyboard('{ArrowDown}{End}{Enter}{Enter}```');
    await expect.element(list).toBeVisible();
    if (isPhone()) {
      for (const option of list.getByRole('option').elements()) {
        expect(option.getBoundingClientRect().height).toBeGreaterThanOrEqual(44);
      }
    }
    await expectAccessible(screen.container);
    await list.getByRole('option', { name: 'bash' }).click();
    await expect.element(list).not.toBeInTheDocument();
    await expect.element(textbox).toHaveFocus();
    await userEvent.keyboard('{Enter}ls{Enter}```');
    await new Promise((done) => setTimeout(done, 50));
    expect(list.query()).toBeNull();
    await expect.element(textbox).toHaveValue('```rust\nfn main()\n```\n\n```bash\nls\n```');

    // Esc declines: auto-detection stays, the list does not come back on that fence.
    await userEvent.keyboard('{Enter}{Enter}~~~');
    await expect.element(list).toBeVisible();
    await userEvent.keyboard('{Escape}');
    await expect.element(list).not.toBeInTheDocument();
    await userEvent.keyboard('{ArrowLeft}{ArrowRight}');
    expect(list.query()).toBeNull();
  });

  it('CNT-054: no mention list without an issue or in the basic profile', async () => {
    const calls = fakeApi({});
    const screen = await render(MarkdownEditor, {
      label: 'Description',
      issue: 7,
      profile: 'basic',
    });
    await screen.getByRole('textbox', { name: /^Description/ }).click();
    await userEvent.keyboard('@jo');
    await new Promise((done) => setTimeout(done, 300));
    expect(calls).toHaveLength(0);
    expect(screen.getByRole('listbox').query()).toBeNull();
  });

  it('CNT-029: read-only text has no formatting; Russian texts', async () => {
    setLocale('ru');
    const screen = await render(MarkdownEditor, {
      label: 'Описание',
      value: 'Текст',
      readonly: true,
      error: 'Слишком длинно',
    });

    expect(screen.getByRole('toolbar').query()).toBeNull();
    await expect.element(screen.getByRole('tab', { name: 'Текст' })).toBeVisible();
    await expect.element(screen.getByRole('tab', { name: 'Просмотр' })).toBeVisible();
    await expect.element(screen.getByText('Поддерживается Markdown.')).toBeVisible();
    const textbox = screen.getByRole('textbox', { name: /^Описание/ });
    await expect.element(textbox).toHaveAttribute('readonly');
    await expect.element(textbox).toHaveAttribute('aria-invalid', 'true');
    await expect.element(textbox).toHaveAccessibleDescription(/Слишком длинно/);
    await expectAccessible(screen.container);
  });

  it('CNT-034: a pasted picture is uploaded and named at the caret, with a status line', async () => {
    const { upload, waiting } = pendingUploads();
    const calls = fakeApi({
      'POST /markdown': () => ({
        body: { html: '<p>ok</p>\n', refs: { users: [], issues: [], files: [] } },
      }),
    });
    const screen = await render(MarkdownEditor, {
      label: 'Description',
      value: 'See  please',
      upload,
    });
    const area = field(screen);
    await expect
      .element(screen.getByRole('textbox', { name: /^Description/ }))
      .toHaveAccessibleDescription(/Paste or drop files to attach them\./);
    area.focus();
    area.setSelectionRange(4, 4);

    expect(paste(area, transfer(PNG))).toBe(false);
    expect(upload).toHaveBeenCalledWith(PNG);
    await expect.element(screen.getByText('Uploading 1 file…')).toBeVisible();
    expect(announcement('polite')).toContain('Uploading 1 file…');

    waiting[0]?.resolve(uploaded(31, PNG));
    await expect
      .element(screen.getByRole('textbox', { name: /^Description/ }))
      .toHaveValue('See ![](my%20shot.png) please');
    expect(area.selectionStart).toBe('See ![](my%20shot.png)'.length);
    await expect.element(screen.getByText('Uploading 1 file…')).not.toBeInTheDocument();
    expect(announcement('polite')).toContain('1 file added');
    // The browser's undo takes the insertion back as one step.
    await userEvent.keyboard('{ControlOrMeta>}z{/ControlOrMeta}');
    expect(area.value).toBe('See  please');
    await userEvent.keyboard('{ControlOrMeta>}{Shift>}z{/Shift}{/ControlOrMeta}');

    // The preview shows the staged upload before it is attached.
    await screen.getByRole('tab', { name: 'Preview' }).click();
    await expect.element(screen.getByText('ok')).toBeVisible();
    expect(calls.at(-1)?.body).toEqual({
      text: area.value,
      profile: 'full',
      issue: null,
      uploads: [31],
    });
    expect(smallTargets(screen.container, isPhone() ? 44 : 32)).toEqual([]);
    await expectAccessible(screen.container);
  });

  it('CNT-034: dropped files are uploaded together; pictures and videos show, others link', async () => {
    const { upload, waiting } = pendingUploads();
    const calls = fakeApi({
      'POST /markdown': () => ({
        body: { html: '<p>ok</p>\n', refs: { users: [], issues: [], files: [] } },
      }),
    });
    const screen = await render(MarkdownEditor, { label: 'Description', value: 'Logs: ', upload });
    const area = field(screen);
    area.focus();
    area.setSelectionRange(6, 6);
    const frame = area.closest('.frame');
    if (!frame) throw new Error('no frame');

    const data = transfer(LOG, CLIP);
    expect(
      frame.dispatchEvent(
        new DragEvent('dragover', { dataTransfer: data, bubbles: true, cancelable: true }),
      ),
    ).toBe(false);
    await vi.waitFor(() => {
      expect(frame.classList.contains('dragging')).toBe(true);
    });
    area.dispatchEvent(
      new DragEvent('drop', { dataTransfer: data, bubbles: true, cancelable: true }),
    );
    await vi.waitFor(() => {
      expect(frame.classList.contains('dragging')).toBe(false);
    });
    await expect.element(screen.getByText('Uploading 2 files…')).toBeVisible();

    waiting[1]?.resolve(uploaded(41, CLIP));
    // Nothing goes into the text until the whole drop is uploaded.
    await new Promise((done) => setTimeout(done, 50));
    expect(area.value).toBe('Logs: ');
    waiting[0]?.resolve(uploaded(40, LOG));
    await expect
      .element(screen.getByRole('textbox', { name: /^Description/ }))
      .toHaveValue('Logs: [build \\[1\\].log](build%20%5B1%5D.log)\n![](clip.mp4)');

    await screen.getByRole('tab', { name: 'Preview' }).click();
    await expect.element(screen.getByText('ok')).toBeVisible();
    expect((calls.at(-1)?.body as MarkdownPreview).uploads).toEqual([40, 41]);
  });

  it('CNT-034: a failed upload says why under the field; the others still go in', async () => {
    const { upload, waiting } = pendingUploads();
    const screen = await render(MarkdownEditor, { label: 'Description', upload });
    const area = field(screen);
    area.focus();

    paste(area, transfer(PNG, LOG));
    waiting[0]?.reject({ code: 'internal', message: 'boom' });
    waiting[1]?.resolve(uploaded(7, LOG));
    const problem = screen.getByText('“my shot.png” was not uploaded.', { exact: false });
    await expect.element(problem).toBeVisible();
    expect(announcement('assertive')).toContain('“my shot.png” was not uploaded.');
    expect(area.value).toBe('[build \\[1\\].log](build%20%5B1%5D.log)');
    await expectAccessible(screen.container);

    // The next upload clears the old problem.
    paste(area, transfer(CLIP));
    await expect.element(problem).not.toBeInTheDocument();
    waiting[2]?.resolve(uploaded(8, CLIP));
    await vi.waitFor(() => {
      expect(area.value).toContain('![](clip.mp4)');
    });
  });

  it('CNT-034: an address pasted over a selection links it; copied text is never uploaded', async () => {
    const upload = vi.fn();
    const screen = await render(MarkdownEditor, {
      label: 'Description',
      value: 'Read the docs first',
      upload,
    });
    const area = field(screen);

    select(area, 'the docs');
    const address = new DataTransfer();
    address.setData('text/plain', 'https://example.com/guide');
    expect(paste(area, address)).toBe(false);
    expect(area.value).toBe('Read [the docs](https://example.com/guide) first');

    // Text with a picture of itself (a spreadsheet) pastes as text.
    const cells = transfer(PNG);
    cells.setData('text/plain', 'a\tb');
    expect(paste(area, cells)).toBe(true);
    // Without a selection an address pastes as it is.
    area.setSelectionRange(0, 0);
    expect(paste(area, address)).toBe(true);
    expect(upload).not.toHaveBeenCalled();
  });

  it('CNT-034: without an upload, in the basic profile or read-only, files are not taken', async () => {
    const upload = vi.fn();
    for (const props of [
      { label: 'Description' },
      { label: 'Description', upload, profile: 'basic' as const },
      { label: 'Description', upload, readonly: true },
    ]) {
      const screen = await render(MarkdownEditor, props);
      const area = field(screen);
      expect(paste(area, transfer(PNG))).toBe(true);
      expect(
        area.dispatchEvent(
          new DragEvent('dragover', {
            dataTransfer: transfer(PNG),
            bubbles: true,
            cancelable: true,
          }),
        ),
      ).toBe(true);
      expect(screen.getByText('Paste or drop files', { exact: false }).query()).toBeNull();
      await screen.unmount();
    }
    expect(upload).not.toHaveBeenCalled();

    // The link over a selection works without uploads.
    const screen = await render(MarkdownEditor, { label: 'Description', value: 'site' });
    const area = field(screen);
    select(area, 'site');
    const address = new DataTransfer();
    address.setData('text/plain', 'https://example.com');
    paste(area, address);
    expect(area.value).toBe('[site](https://example.com)');
  });

  it('CNT-034: Russian texts of uploads', async () => {
    setLocale('ru');
    const { upload, waiting } = pendingUploads();
    const screen = await render(MarkdownEditor, { label: 'Описание', upload });
    const area = field(screen);
    await expect
      .element(screen.getByText('Файлы можно вставить или перетащить сюда.', { exact: false }))
      .toBeVisible();
    area.focus();
    paste(area, transfer(PNG, CLIP));
    await expect.element(screen.getByText('Загружаются 2 файла…')).toBeVisible();
    waiting[0]?.reject(new Error('no'));
    waiting[1]?.reject(new Error('no'));
    await expect
      .element(screen.getByText('Файл «my shot.png» не загружен.', { exact: false }))
      .toBeVisible();
  });
});
