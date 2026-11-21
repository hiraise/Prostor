// The files of the issue page (step 1.7b-4; CNT-017, CNT-020, CNT-022, CNT-024, CNT-034, CNT-044):
// the section, the lightbox, the file menu, uploads from anywhere on the page and from editors.
import { afterEach, describe, expect, it, vi } from 'vitest';
import { userEvent } from 'vitest/browser';
import { render } from 'vitest-browser-svelte';

import type { AccessMap, Attachment, Upload } from '#lib/api/schema.gen.ts';
import { BASE_LOCALE, setLocale } from '#lib/i18n/locale.svelte.ts';
import { preferences, setAccessForTests, setPreferencesForTests } from '#lib/session/index.ts';
import { store } from '#lib/store/index.ts';

import { expectAccessible, isPhone, smallTargets } from '../../../../test/a11y.ts';
import { fakeApi, type FakeRequest } from '../../../../test/fake-api.ts';
import { ANNA, issue, issuePage } from '../test-data.ts';
import { forgetIssuePages } from '../issues.ts';
import IssuePage from '../page/IssuePage.svelte';
import type { SendFile } from './files.svelte.ts';

const navigation = vi.hoisted(() => ({ goto: vi.fn(() => Promise.resolve()) }));
vi.mock('$app/navigation', () => navigation);

/** Sees, changes and adds files to project 1. */
const WRITER: AccessMap = {
  global: [],
  projects: [
    {
      project_id: 1,
      permissions: ['project.basic', 'issue.update', 'attachment.create', 'comment.create'],
    },
  ],
  capabilities: [],
};

function attachment(id: number, name: string, rest: Partial<Attachment> = {}): Attachment {
  return {
    id,
    issue: 501,
    comment: null,
    name,
    size: 2048,
    content_type: 'application/octet-stream',
    kind: 'other',
    width: null,
    height: null,
    url: `/api/v1/attachments/${String(id)}/${name}`,
    thumbnail_url: null,
    author: ANNA,
    created_at: '2026-10-21T10:00:00Z',
    audience: { users: [], groups: [] },
    can: { update: true, delete: true },
    version: 1,
    ...rest,
  };
}

const SHOT = attachment(41, 'shot.png', {
  kind: 'image',
  content_type: 'image/png',
  width: 800,
  height: 600,
  thumbnail_url: '/api/v1/attachments/41/thumb',
});
const CLIP = attachment(42, 'clip.webm', { kind: 'video', content_type: 'video/webm' });
const REPORT = attachment(43, 'report.pdf', { kind: 'pdf', content_type: 'application/pdf' });
const LOG = attachment(44, 'build.log', {
  audience: { users: [ANNA], groups: [] },
  can: { update: false, delete: false },
});

function staged(id: number, name: string): Upload {
  return {
    id,
    name,
    size: 10,
    content_type: 'text/plain',
    kind: 'other',
    url: `/api/v1/uploads/${String(id)}/${name}`,
    created_at: '2026-10-24T10:00:00Z',
  };
}

async function show(
  files: Attachment[],
  routes: Parameters<typeof fakeApi>[0] = {},
  upload: SendFile = vi.fn((sent: File) => Promise.resolve(staged(900, sent.name))),
) {
  const shown = issue();
  const calls = fakeApi({
    'PATCH /users/me/preferences': { body: {} },
    'GET /issues/501': { body: shown },
    ...routes,
  });
  setAccessForTests(WRITER);
  setPreferencesForTests({});
  store.put('issue', [shown], 'full');
  const answer = issuePage(shown, { attachments: { attachments: files, next: null } });
  store.put('attachment', files, 'full');
  const screen = await render(IssuePage, {
    issue: shown,
    page: { activity: answer.activity, links: answer.links, attachments: answer.attachments },
    upload,
  });
  return { screen, calls, upload };
}

function sent(calls: FakeRequest[], method: string, path: string): unknown[] {
  return calls
    .filter((call) => call.method === method && call.path === path)
    .map((call) => call.body);
}

function filesOf(...files: File[]): DataTransfer {
  const transfer = new DataTransfer();
  for (const file of files) transfer.items.add(file);
  return transfer;
}

afterEach(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
  store.entities.table('issue').clear();
  store.entities.table('comment').clear();
  store.entities.table('attachment').clear();
  forgetIssuePages();
  setAccessForTests(null);
  setPreferencesForTests(null);
  setLocale(BASE_LOCALE);
});

describe('CNT-020: the attachments section', () => {
  it('shows thumbnails and kind icons with names, locks restricted files and folds', async () => {
    const { screen } = await show([SHOT, CLIP, REPORT, LOG]);

    const section = screen.getByRole('region', { name: /Files/ });
    const list = section.getByRole('list', { name: '4 files' });
    await expect.element(list).toBeVisible();
    await expect.element(list.getByText('shot.png')).toBeVisible();
    expect(
      list.getByRole('button', { name: 'Open shot.png' }).element().querySelector('img'),
    ).not.toBeNull();
    await expect.element(list.getByRole('button', { name: 'Open clip.webm' })).toBeVisible();
    // PDFs open in a new tab, other files download.
    const pdf = list.getByRole('link', { name: 'Open report.pdf' });
    await expect.element(pdf).toHaveAttribute('target', '_blank');
    const log = list.getByRole('link', { name: 'Open build.log' });
    await expect.element(log).toHaveAttribute('download', 'build.log');
    await expect
      .element(list.getByRole('img', { name: 'Visible to Anna Admin and the reporter' }))
      .toBeInTheDocument();
    await expectAccessible(section.element());
    if (isPhone()) expect(await smallTargets(section.element())).toEqual([]);

    const toggle = section.getByRole('button', { name: 'Files 4' });
    await expect.element(toggle).toHaveAttribute('aria-expanded', 'true');
    await toggle.click();
    await expect.element(section.getByText('shot.png')).not.toBeVisible();
    await expect.element(toggle).toHaveAttribute('aria-expanded', 'false');
    expect(preferences.value('attachments_collapsed')).toBe(true);
  });

  it('stays away entirely while there are no files', async () => {
    const { screen } = await show([]);

    expect(screen.getByRole('region', { name: 'Files' }).query()).toBeNull();
    // No empty header, no attach button: attaching starts from «Ещё», a drop or a paste,
    // and the section appears with the first file.
    expect(screen.getByText('No files yet.').query()).toBeNull();
    expect(screen.getByRole('button', { name: 'Attach files' }).query()).toBeNull();
  });

  it('a reader without the right sees the files but no attach action', async () => {
    fakeApi({});
    setAccessForTests({
      global: [],
      projects: [{ project_id: 1, permissions: ['project.basic'] }],
      capabilities: [],
    });
    setPreferencesForTests({});
    const shown = issue({ can: { ...issue().can, update: false } });
    const screen = await render(IssuePage, {
      issue: shown,
      page: {
        activity: issuePage().activity,
        links: issuePage().links,
        attachments: { attachments: [REPORT], next: null },
      },
    });

    await expect.element(screen.getByText('report.pdf')).toBeVisible();
    expect(screen.getByRole('button', { name: 'Attach files' }).elements()).toHaveLength(0);
  });
});

describe('CNT-022: preview', () => {
  it('opens pictures and videos in the lightbox, pages with buttons and arrows', async () => {
    const { screen } = await show([SHOT, REPORT, CLIP]);

    await screen.getByRole('button', { name: 'Open shot.png' }).click();
    const dialog = screen.getByRole('dialog', { name: 'shot.png' });
    await expect.element(dialog).toBeVisible();
    await expect
      .element(dialog.getByRole('img', { name: 'shot.png' }))
      .toHaveAttribute('src', '/api/v1/attachments/41/shot.png');
    await expect.element(dialog.getByText('1 of 2')).toBeVisible();
    await expect
      .element(dialog.getByRole('link', { name: 'Download' }))
      .toHaveAttribute('download', 'shot.png');

    await dialog.getByRole('button', { name: 'Next' }).click();
    const video = screen.getByRole('dialog', { name: 'clip.webm' });
    await expect.element(video).toBeVisible();
    expect(video.element().querySelector('video[controls]')).not.toBeNull();

    await userEvent.keyboard('{ArrowRight}');
    await expect.element(screen.getByRole('dialog', { name: 'shot.png' })).toBeVisible();
    await userEvent.keyboard('{Escape}');
    await expect.element(screen.getByRole('dialog')).not.toBeInTheDocument();
  });
});

describe('CNT-020, CNT-024: the file menu', () => {
  it('renames, shows details and deletes after confirmation', async () => {
    const { screen, calls } = await show([REPORT], {
      'PATCH /attachments/43': ({ body }) => ({
        body: { ...REPORT, name: (body as { name: string }).name, version: 2 },
      }),
      'DELETE /attachments/43': { status: 204 },
    });

    const actions = screen.getByRole('button', { name: 'Actions with report.pdf' });
    await actions.click();
    await screen.getByRole('menuitem', { name: 'Details' }).click();
    const details = screen.getByRole('dialog', { name: 'report.pdf' });
    await expect.element(details.getByText('2 kB')).toBeVisible();
    await expect.element(details.getByText('application/pdf')).toBeVisible();
    await details.getByRole('button', { name: 'Close' }).last().click();

    await actions.click();
    await screen.getByRole('menuitem', { name: 'Rename' }).click();
    const rename = screen.getByRole('dialog', { name: 'Rename the file' });
    await rename.getByRole('textbox', { name: 'Name' }).fill('final.pdf');
    await rename.getByRole('button', { name: 'Save' }).click();
    await expect.element(screen.getByText('final.pdf')).toBeVisible();
    expect(sent(calls, 'PATCH', '/attachments/43')).toEqual([{ name: 'final.pdf', version: 1 }]);

    await screen.getByRole('button', { name: 'Actions with final.pdf' }).click();
    await screen.getByRole('menuitem', { name: 'Delete' }).click();
    const confirm = screen.getByRole('dialog', { name: 'Delete final.pdf?' });
    await confirm.getByRole('button', { name: 'Delete' }).click();
    // The last file gone, the section leaves with it.
    await expect.element(screen.getByText('final.pdf')).not.toBeInTheDocument();
    expect(screen.getByRole('region', { name: /Files/ }).query()).toBeNull();
    expect(sent(calls, 'DELETE', '/attachments/43')).toHaveLength(1);
    // The texts are asked again, so names of the file show it as missing.
    await expect.poll(() => sent(calls, 'GET', '/issues/501')).toHaveLength(2);
  });

  it('files of others without rights offer only download and details', async () => {
    const { screen } = await show([LOG]);

    await screen.getByRole('button', { name: 'Actions with build.log' }).click();
    await expect.element(screen.getByRole('menuitem', { name: 'Download' })).toBeVisible();
    await expect.element(screen.getByRole('menuitem', { name: 'Details' })).toBeVisible();
    expect(screen.getByRole('menuitem', { name: 'Rename' }).elements()).toHaveLength(0);
    expect(screen.getByRole('menuitem', { name: 'Delete' }).elements()).toHaveLength(0);
  });
});

describe('CNT-017: uploads', () => {
  it('a drop anywhere on the page attaches the files', async () => {
    const attached = attachment(45, 'notes.txt');
    const { screen, calls } = await show([], {
      'POST /issues/501/attachments': {
        status: 201,
        body: { attachments: [attached], next: null },
      },
    });

    const transfer = filesOf(new File(['x'], 'notes.txt', { type: 'text/plain' }));
    document.body.dispatchEvent(
      new DragEvent('dragover', { dataTransfer: transfer, bubbles: true, cancelable: true }),
    );
    await expect.element(screen.getByText('Drop to attach')).toBeInTheDocument();
    document.body.dispatchEvent(
      new DragEvent('drop', { dataTransfer: transfer, bubbles: true, cancelable: true }),
    );

    await expect.element(screen.getByText('notes.txt')).toBeVisible();
    expect(screen.getByText('Drop to attach').elements()).toHaveLength(0);
    expect(sent(calls, 'POST', '/issues/501/attachments')).toEqual([{ uploads: [900] }]);
  });

  it('a paste outside the editors attaches; a failed upload says why', async () => {
    const upload = vi.fn(() => Promise.reject(new Error('The disk is full')));
    const { screen, calls } = await show([], {}, upload);

    document.body.dispatchEvent(
      new ClipboardEvent('paste', {
        clipboardData: filesOf(new File(['x'], 'pasted.png', { type: 'image/png' })),
        bubbles: true,
        cancelable: true,
      }),
    );

    await expect.element(screen.getByText(/pasted\.png was not attached/)).toBeVisible();
    expect(sent(calls, 'POST', '/issues/501/attachments')).toEqual([]);
    await screen.getByRole('button', { name: 'Dismiss' }).click();
    expect(screen.getByText(/pasted\.png was not attached/).elements()).toHaveLength(0);
  });

  it('«Attach files» in the actions menu opens the file picker', async () => {
    const pick = vi.spyOn(HTMLInputElement.prototype, 'click').mockImplementation(() => undefined);
    const { screen } = await show([]);

    await screen.getByRole('button', { name: 'More' }).click();
    await screen.getByRole('menuitem', { name: 'Attach files' }).click();
    expect(pick).toHaveBeenCalled();
  });
});

describe('CNT-034, CNT-044: files in the texts', () => {
  it('a file dropped into the description in edit mode is attached and named as the issue has it', async () => {
    const attached = attachment(46, 'shot (2).png', { kind: 'image' });
    const { screen, calls } = await show([SHOT], {
      'POST /issues/501/attachments': {
        status: 201,
        body: { attachments: [attached], next: null },
      },
    });

    await screen.getByRole('button', { name: 'Edit' }).click();
    const field = screen.getByRole('textbox', { name: 'Description' });
    await expect.element(field).toBeVisible();
    field.element().dispatchEvent(
      new DragEvent('drop', {
        dataTransfer: filesOf(new File(['x'], 'shot.png', { type: 'image/png' })),
        bubbles: true,
        cancelable: true,
      }),
    );

    await expect
      .poll(() => (field.element() as HTMLTextAreaElement).value)
      .toContain('![](shot%20%282%29.png)');
    expect(sent(calls, 'POST', '/issues/501/attachments')).toEqual([{ uploads: [900] }]);
    // The page did not take the same drop a second time.
    await expect.element(screen.getByRole('button', { name: 'Open shot (2).png' })).toBeVisible();
  });

  it('«Attachment» in the editor names a file of the issue at the caret', async () => {
    const { screen } = await show([REPORT]);

    await screen.getByRole('button', { name: 'Edit' }).click();
    const field = screen.getByRole('textbox', { name: 'Description' });
    await field.fill('');
    await screen.getByRole('button', { name: 'Attachment' }).first().click();
    await screen.getByRole('menuitem', { name: 'report.pdf' }).click();
    await expect.element(field).toHaveValue('[report.pdf](report.pdf)');
  });
});
