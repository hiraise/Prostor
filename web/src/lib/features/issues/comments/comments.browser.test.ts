// Comments on the issue page (step 1.7b-3): the box, edit, delete and restore, the audience, the
// permalink and files.
import { afterEach, describe, expect, it, vi } from 'vitest';
import { userEvent } from 'vitest/browser';
import { render } from 'vitest-browser-svelte';

import type { ActivityPage, Attachment, Comment, Issue, Upload } from '#lib/api/schema.gen.ts';
import { BASE_LOCALE, setLocale } from '#lib/i18n/locale.svelte.ts';
import { setPreferencesForTests } from '#lib/session/index.ts';
import { store } from '#lib/store/index.ts';
import { toasts } from '#lib/ui/toast.svelte.ts';

import { expectAccessible, isPhone } from '../../../../test/a11y.ts';
import { failure, fakeApi, type FakeRequest, type FakeRoute } from '../../../../test/fake-api.ts';
import CommentsHarness from '../../../../test/harness/CommentsHarness.svelte';
import { ActivityStream as Stream } from '../activity/activity.svelte.ts';
import '../content.ts';
import { forgetIssuePages } from '../issues.ts';
import { ANNA, BORIS, comment, commentItem, issue } from '../test-data.ts';

const DEVS = { id: 5, name: 'Developers', builtin: null, team_of: null };

function page(...comments: Comment[]): ActivityPage {
  return {
    items: comments.map((shown) => commentItem(shown.id * 10, shown)).reverse(),
    older: null,
    newer: null,
  };
}

function file(id: number, name: string, rest: Partial<Attachment> = {}): Attachment {
  return {
    id,
    issue: 501,
    comment: 1,
    name,
    size: 1200,
    content_type: 'text/plain',
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
  comments: Comment[],
  routes: Record<string, FakeRoute> = {},
  options: { shown?: Issue; upload?: (file: File) => Promise<Upload> } = {},
) {
  const calls = fakeApi({ 'PATCH /users/me/preferences': { body: {} }, ...routes });
  setPreferencesForTests({ activity_order: 'oldest_first' });
  const shown = options.shown ?? issue();
  // The page's one request puts its comments into the store (openIssue).
  store.put('comment', comments, 'full');
  const stream = new Stream(shown.id, page(...comments));
  const upload = options.upload ?? vi.fn((sent: File) => Promise.resolve(staged(900, sent.name)));
  const screen = await render(CommentsHarness, { issue: shown, stream, author: ANNA, upload });
  return { screen, stream, calls, upload };
}

function sent(calls: FakeRequest[], method: string, path: string): unknown[] {
  return calls.filter((call) => call.method === method && call.path === path).map((c) => c.body);
}

afterEach(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
  store.entities.table('comment').clear();
  forgetIssuePages();
  setPreferencesForTests(null);
  toasts.clear();
  setLocale(BASE_LOCALE);
});

describe('CNT-001: the comment box', () => {
  it('posts with Ctrl+Enter or the button; the comment joins the stream', async () => {
    const posted = comment(2, 'Fixed in **main**', {
      html: '<p>Fixed in <strong>main</strong></p>',
    });
    const { screen, calls } = await show([comment(1, 'First')], {
      'POST /issues/501/comments': { status: 201, body: { comment: posted, messages: [] } },
      'GET /issues/501/activity': {
        body: { items: [commentItem(20, posted)], older: null, newer: null },
      },
    });
    // The box rests as one line; a click opens it.
    const start = screen.getByRole('button', { name: 'Write a comment…' });
    await start.click();
    const box = screen.getByRole('form', { name: 'New comment' });
    const field = box.getByRole('textbox', { name: /^Comment/ });

    // «Отправить» appears only once writing has begun.
    expect(box.getByRole('button', { name: 'Send' }).query()).toBeNull();
    await field.fill('Fixed in **main**');
    await expect.element(box.getByRole('button', { name: 'Send' })).toBeVisible();
    await field.click();
    await userEvent.keyboard('{Control>}{Enter}{/Control}');
    await expect
      .poll(() => sent(calls, 'POST', '/issues/501/comments'))
      .toEqual([{ text: 'Fixed in **main**' }]);
    await expect.poll(() => screen.getByRole('article').elements().length).toBe(2);
    // A sent comment folds the box back to its resting line.
    expect(box.query()).toBeNull();
    await expect.element(screen.getByRole('button', { name: 'Write a comment…' })).toBeVisible();
    const cards = screen.getByRole('article');
    expect(cards.elements()).toHaveLength(2);
    expect(cards.nth(1).element().textContent).toContain('Fixed in main');
    expect(calls.find((call) => call.path === '/issues/501/activity')?.query.get('from')).toBe(
      '10',
    );
    await expectAccessible(screen.container);
  });

  it('«Отмена» folds the box back, asking first while a draft exists', async () => {
    const { screen, calls } = await show([]);

    // An empty box folds at once.
    await screen.getByRole('button', { name: 'Write a comment…' }).click();
    await screen
      .getByRole('form', { name: 'New comment' })
      .getByRole('button', {
        name: 'Cancel',
      })
      .click();
    await expect.element(screen.getByRole('button', { name: 'Write a comment…' })).toBeVisible();

    // A draft asks before it is thrown away.
    await screen.getByRole('button', { name: 'Write a comment…' }).click();
    await screen.getByRole('textbox', { name: /^Comment/ }).fill('On second thought');
    await screen
      .getByRole('form', { name: 'New comment' })
      .getByRole('button', {
        name: 'Cancel',
      })
      .click();
    const confirm = screen.getByRole('dialog', { name: 'Discard the draft?' });
    await confirm.getByRole('button', { name: 'Keep writing' }).click();
    await expect
      .element(screen.getByRole('textbox', { name: /^Comment/ }))
      .toHaveValue('On second thought');
    await screen
      .getByRole('form', { name: 'New comment' })
      .getByRole('button', {
        name: 'Cancel',
      })
      .click();
    await screen
      .getByRole('dialog', { name: 'Discard the draft?' })
      .getByRole('button', {
        name: 'Discard',
      })
      .click();
    await expect.element(screen.getByRole('button', { name: 'Write a comment…' })).toBeVisible();
    expect(sent(calls, 'POST', '/issues/501/comments')).toEqual([]);
  });

  it('CNT-018: files alone are a comment', async () => {
    const { screen, calls } = await show([], {
      'POST /issues/501/comments': ({ body }) => ({
        status: 201,
        body: {
          comment: comment(3, '', { files: [file(7, 'log.txt')], ...(body as object) }),
          messages: [],
        },
      }),
      'GET /issues/501/activity': { body: { items: [], older: null, newer: null } },
    });
    await screen.getByRole('button', { name: 'Write a comment…' }).click();
    const field = screen.getByRole('textbox', { name: /^Comment/ });
    const drop = new DataTransfer();
    drop.items.add(new File(['x'], 'log.txt', { type: 'text/plain' }));
    field.element().dispatchEvent(new DragEvent('drop', { dataTransfer: drop, bubbles: true }));
    await expect.element(field).toHaveValue('[log.txt](log.txt)');
    await field.clear();
    await screen.getByRole('button', { name: 'Send' }).click();
    await expect
      .poll(() => sent(calls, 'POST', '/issues/501/comments'))
      .toEqual([{ text: '', uploads: [900] }]);
  });

  it('CNT-054: the box offers the people who see the issue', async () => {
    const { screen, calls } = await show([], {
      'GET /issues/501/mention-candidates': { body: { users: [BORIS] } },
    });
    await screen.getByRole('button', { name: 'Write a comment…' }).click();
    await screen.getByRole('textbox', { name: /^Comment/ }).click();
    await userEvent.keyboard('@bo');
    await expect.element(screen.getByRole('listbox', { name: 'People to mention' })).toBeVisible();
    await expect.poll(() => calls.at(-1)?.query.get('q')).toBe('bo');
  });
});

describe('CNT-003: the audience of a comment', () => {
  it('a restricted comment is marked; the box limits a new one inside the issue', async () => {
    const limited = comment(1, 'Only devs', { audience: { users: [], groups: [DEVS] } });
    const { screen, calls } = await show([limited], {
      'GET /projects/1/audience-candidates': { body: { users: [BORIS], groups: [DEVS] } },
      'POST /issues/501/comments': {
        status: 201,
        body: { comment: comment(2, 'Secret'), messages: [] },
      },
      'GET /issues/501/activity': { body: { items: [], older: null, newer: null } },
    });
    const card = screen.getByRole('article');
    await expect.element(card.getByText(/Developers/)).toBeVisible();
    expect(card.element().classList).toContain('restricted');

    await screen.getByRole('button', { name: 'Write a comment…' }).click();
    const box = screen.getByRole('form', { name: 'New comment' });
    await box.getByRole('button', { name: 'Visibility' }).click();
    await box.getByRole('radio', { name: 'Only the chosen' }).click();
    await box.getByRole('textbox', { name: /^Comment/ }).fill('Secret');
    await box.getByRole('button', { name: 'Send' }).click();
    // Nobody chosen: said, not sent.
    await expect.element(box.getByText('Choose at least one group or person.')).toBeVisible();
    await box.getByRole('checkbox', { name: /Developers/ }).click();
    await box.getByRole('button', { name: 'Send' }).click();
    await expect
      .poll(() => sent(calls, 'POST', '/issues/501/comments'))
      .toEqual([{ text: 'Secret', audience: { users: [], groups: [5] } }]);
  });
});

describe('CNT-004: editing a comment', () => {
  it('CNT-041: ticks a checklist box of the rendered text and saves it', async () => {
    const { screen, calls } = await show(
      [
        comment(1, 'Plan:\n- [ ] write\n', {
          html: [
            '<p>Plan:</p>',
            '<ul>',
            '<li data-sourcepos="2:1-2:11"><input type="checkbox" disabled=""> write</li>',
            '</ul>',
            '',
          ].join('\n'),
        }),
      ],
      {
        'PATCH /comments/1': ({ body }) => {
          const text = (body as { text: string }).text;
          const ticked = text.includes('- [x]');
          return {
            body: comment(1, text, {
              version: 2,
              html: [
                '<p>Plan:</p>',
                '<ul>',
                `<li data-sourcepos="2:1-2:11"><input type="checkbox"${ticked ? ' checked=""' : ''} disabled=""> write</li>`,
                '</ul>',
                '',
              ].join('\n'),
            }),
          };
        },
      },
    );
    const card = screen.getByRole('article');
    const box = card.getByRole('checkbox', { name: 'write' });
    await expect.element(box).toBeEnabled();
    await box.click();
    await expect.element(box).toBeChecked();
    expect(sent(calls, 'PATCH', '/comments/1')).toEqual([
      { text: 'Plan:\n- [x] write\n', audience: { users: [], groups: [] }, version: 1 },
    ]);
  });
  it('from the menu or with F2; Esc cancels; saved on its version', async () => {
    const { screen, calls } = await show([comment(1, 'Typo hree')], {
      'PATCH /comments/1': ({ body }) => ({
        body: comment(1, 'Typo here', {
          version: 2,
          updated_at: '2026-10-24T10:00:00Z',
          updater: ANNA,
          ...(body as object),
        }),
      }),
    });
    const card = screen.getByRole('article');
    await card.getByRole('button', { name: 'Comment actions' }).click();
    await screen.getByRole('menuitem', { name: 'Edit' }).click();
    const field = card.getByRole('textbox', { name: /^Comment/ });
    await expect.element(field).toHaveValue('Typo hree');
    await userEvent.keyboard('{Escape}');
    await expect.element(field).not.toBeInTheDocument();
    await expect.element(card).toHaveFocus();

    await userEvent.keyboard('{F2}');
    await card.getByRole('textbox', { name: /^Comment/ }).fill('Typo here');
    await userEvent.keyboard('{Control>}{Enter}{/Control}');
    await expect.element(card.getByText('edited')).toBeVisible();
    expect(sent(calls, 'PATCH', '/comments/1')).toEqual([
      { text: 'Typo here', audience: { users: [], groups: [] }, version: 1 },
    ]);
  });

  it('a comment saved meanwhile shows its text; «Save my version» saves over it', async () => {
    let attempts = 0;
    const theirs = comment(1, 'Their text', { html: '<p>Their text</p>', version: 2 });
    const { screen, calls } = await show([comment(1, 'Old')], {
      'PATCH /comments/1': ({ body }) =>
        ++attempts === 1
          ? failure(409, 'conflict')
          : { body: comment(1, 'Mine', { version: 3, ...(body as object) }) },
      'GET /comments/1': { body: theirs },
    });
    const card = screen.getByRole('article');
    (card.element() as HTMLElement).focus();
    await userEvent.keyboard('{F2}');
    await card.getByRole('textbox', { name: /^Comment/ }).fill('Mine');
    await card.getByRole('button', { name: 'Save' }).click();
    await expect
      .element(card.getByText('someone else saved this comment', { exact: false }))
      .toBeVisible();
    await expect.element(card.getByText('Their text')).toBeVisible();
    await expect.element(card.getByRole('textbox', { name: /^Comment/ })).toHaveValue('Mine');
    await card.getByRole('button', { name: 'Save my version' }).click();
    await expect.element(card.getByRole('textbox')).not.toBeInTheDocument();
    expect(sent(calls, 'PATCH', '/comments/1').map((body) => (body as Comment).version)).toEqual([
      1, 2,
    ]);
  });

  it('without the right there is no edit, and F2 does nothing', async () => {
    const { screen } = await show([
      comment(1, 'Theirs', {
        author: BORIS,
        can: { update: false, delete: false, restore: false },
      }),
    ]);
    const card = screen.getByRole('article');
    await card.getByRole('button', { name: 'Comment actions' }).click();
    await expect.element(screen.getByRole('menuitem', { name: 'Copy link' })).toBeVisible();
    expect(screen.getByRole('menuitem', { name: 'Edit' }).query()).toBeNull();
    expect(screen.getByRole('menuitem', { name: 'Delete' }).query()).toBeNull();
    await userEvent.keyboard('{Escape}');
    (card.element() as HTMLElement).focus();
    await userEvent.keyboard('{F2}');
    expect(card.getByRole('textbox').query()).toBeNull();
  });
});

describe('CNT-005: deleting and restoring', () => {
  it('a deleted comment is a placeholder with «Restore»', async () => {
    const placeholder = comment(1, '', {
      html: '',
      deleted: { at: '2026-10-24T10:00:00Z', by: ANNA },
      can: { update: false, delete: false, restore: true },
      version: 2,
    });
    const { screen, calls } = await show([comment(1, 'Wrong issue')], {
      'DELETE /comments/1': { status: 204 },
      'GET /comments/1': { body: placeholder },
      'POST /comments/1/restore': { body: comment(1, 'Wrong issue', { version: 3 }) },
    });
    const card = screen.getByRole('article');
    await card.getByRole('button', { name: 'Comment actions' }).click();
    await screen.getByRole('menuitem', { name: 'Delete' }).click();
    await expect.element(card.getByText('Comment deleted')).toBeVisible();
    expect(card.getByText('Wrong issue').query()).toBeNull();
    await expectAccessible(screen.container);

    await card.getByRole('button', { name: 'Restore' }).click();
    await expect.element(card.getByText('Wrong issue')).toBeVisible();
    expect(
      calls.filter((call) => call.path.startsWith('/comments/1')).map((c) => c.method),
    ).toEqual(['DELETE', 'GET', 'POST']);
  });
});

describe('CNT-007: the link of a comment', () => {
  it('«Copy link» copies the address that opens the issue at the comment', async () => {
    const writeText = vi.spyOn(navigator.clipboard, 'writeText').mockResolvedValue(undefined);
    const { screen } = await show([comment(1, 'Here')]);
    await screen.getByRole('button', { name: 'Comment actions' }).click();
    await screen.getByRole('menuitem', { name: 'Copy link' }).click();
    await expect
      .poll(() => writeText.mock.calls[0]?.[0])
      .toBe(`${location.origin}/issues/WEB-12?comment=1`);
  });
});

describe('CNT-018: files of a comment', () => {
  it('shows them; files dropped on the card are added to it', async () => {
    const picture = file(8, 'shot.png', { kind: 'image', thumbnail_url: '/thumbs/8.png' });
    const { screen, calls } = await show(
      [comment(1, 'See the files', { files: [file(7, 'log.txt'), picture] })],
      {
        'PATCH /comments/1': {
          body: comment(1, 'See the files', {
            files: [file(7, 'log.txt'), picture, file(9, 'more.txt')],
            version: 2,
          }),
        },
      },
    );
    const files = screen.getByRole('list', { name: 'Files of the comment' });
    await expect.element(files.getByRole('link', { name: 'log.txt' })).toBeVisible();
    await expect.element(files.getByRole('img', { name: 'shot.png' })).toBeVisible();

    const drop = new DataTransfer();
    drop.items.add(new File(['x'], 'more.txt', { type: 'text/plain' }));
    screen
      .getByRole('article')
      .element()
      .dispatchEvent(new DragEvent('drop', { dataTransfer: drop, bubbles: true }));
    await expect.element(files.getByRole('link', { name: 'more.txt' })).toBeVisible();
    expect(sent(calls, 'PATCH', '/comments/1')).toEqual([{ add_uploads: [900], version: 1 }]);
    if (isPhone()) {
      for (const link of files.getByRole('link').elements()) {
        expect(link.getBoundingClientRect().height).toBeGreaterThanOrEqual(44);
      }
    }
  });
});

describe('CNT-056: references in a comment', () => {
  it('links and readable issue IDs are links; unreadable IDs stay text', async () => {
    const { screen } = await show([
      comment(1, 'Fixed in WEB-12, see A_1-7 and https://x.example', {
        html: '<p>Fixed in <a href="tf-issue:WEB-12" rel="noopener noreferrer nofollow">WEB-12</a>, see <a href="tf-issue:A_1-7" rel="noopener noreferrer nofollow">A_1-7</a> and <a href="https://x.example" rel="noopener noreferrer nofollow">https://x.example</a></p>\n',
        refs: {
          users: [],
          files: [],
          issues: [
            { token: 'WEB-12', id: 4, readable_id: 'WEB-12', summary: 'Login', resolved: false },
          ],
        },
      }),
    ]);
    const card = screen.getByRole('article');
    await expect
      .element(card.getByRole('link', { name: 'WEB-12' }))
      .toHaveAttribute('href', '/issues/WEB-12');
    await expect.element(card.getByRole('link', { name: 'https://x.example' })).toBeVisible();
    expect(card.getByRole('link', { name: 'A_1-7' }).query()).toBeNull();
    expect(card.element().textContent).toContain('see A_1-7 and');
  });
});

describe('CNT-001: Russian texts', () => {
  it('the box and the actions speak Russian', async () => {
    setLocale('ru');
    const { screen } = await show([comment(1, 'Текст')]);
    await screen.getByRole('button', { name: 'Написать комментарий…' }).click();
    await screen.getByRole('textbox', { name: /^Комментарий/ }).fill('Черновик');
    await expect.element(screen.getByRole('button', { name: 'Отправить' })).toBeVisible();
    await screen.getByRole('button', { name: 'Действия с комментарием' }).click();
    await expect
      .element(screen.getByRole('menuitem', { name: 'Скопировать ссылку' }))
      .toBeVisible();
  });
});
