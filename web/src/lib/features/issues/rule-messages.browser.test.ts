// What the built-in rules told the actor (AUT-023, §3.13): the messages of a mutation answer show
// as toasts (AUT-063, AUT-079, AUT-090, AUT-071), a clone's toast opens the copy (AUT-066), and a
// rule-made history record carries the «rule» badge (§3.12). Desktop and phone profiles alike:
// nothing here depends on the pointer.
import { afterEach, describe, expect, it, vi } from 'vitest';
import { page } from 'vitest/browser';
import { render } from 'vitest-browser-svelte';

import type { ChangeValues, HistoryItem, Issue } from '#lib/api/schema.gen.ts';
import { BASE_LOCALE, setLocale } from '#lib/i18n/locale.svelte.ts';
import { store } from '#lib/store/index.ts';
import { toasts } from '#lib/ui/toast.svelte.ts';

import { expectAccessible } from '../../../test/a11y.ts';
import { fakeApi } from '../../../test/fake-api.ts';
import ChangeRecord from './activity/ChangeRecord.svelte';
import { cloneAsDrafts, cloneIssue, resetCloning } from './clone.svelte.ts';
import { postComment } from './comments/comments.ts';
import { sendField } from './fields/save.ts';
import { issue } from './test-data.ts';

const navigation = vi.hoisted(() => ({ goto: vi.fn(() => Promise.resolve()) }));
vi.mock('$app/navigation', () => navigation);

const EMPTY: ChangeValues = {
  texts: [],
  numbers: [],
  values: [],
  users: [],
  groups: [],
  tags: [],
  files: [],
  links: [],
  boards: [],
  place: null,
};

/** The issue whose stream the record belongs to; only its fields are read. */
const ISSUE = { id: 5, fields: [] } as unknown as Issue;

function change(origin: HistoryItem['origin']): HistoryItem {
  return {
    id: 3,
    at: '2026-11-06T10:00:00Z',
    author: {
      id: 7,
      username: 'anna',
      full_name: 'Anna Lee',
      avatar_color: 2,
      avatar_url: null,
      disabled: false,
      kind: 'person',
      pseudonym: null,
    },
    origin,
    silent: false,
    kind: 'field',
    field: {
      id: 2,
      name: 'State',
      kind: 'enum',
      multi: false,
      builtin: null,
      can_be_empty: false,
      empty_label: '',
    },
    removed: EMPTY,
    added: { ...EMPTY, values: [{ id: 4, name: 'Fixed', color: 1, deleted: false }] },
    after_comment: null,
    clone_of: null,
  };
}

afterEach(() => {
  vi.unstubAllGlobals();
  navigation.goto.mockClear();
  setLocale(BASE_LOCALE);
  toasts.clear();
  resetCloning();
  store.entities.table('comment').clear();
  store.entities.table('issue').clear();
});

describe('AUT-063/079/090: сообщения правил — тосты', () => {
  it('a field change toasts each message of the answer, params already names', async () => {
    fakeApi({
      'PUT /issues/5/fields/2': {
        body: {
          issue: { id: 5 },
          messages: [
            {
              rule: 'assignee_visibility',
              code: 'assignee_not_visible',
              params: { assignee: 'Anna Lee' },
            },
            {
              rule: 'component_owner',
              code: 'assigned_to_owner',
              params: { assignee: 'Bob Ray', value: 'Payments' },
            },
          ],
        },
      },
    });
    await sendField(5, 2, { kind: 'set', value: null });
    expect(toasts.items.map((toast) => toast.message)).toEqual([
      'The assignee Anna Lee cannot see this issue',
      'The issue is assigned to the owner of “Payments”: Bob Ray',
    ]);
  });

  it('a “+1” comment toasts the vote it added', async () => {
    fakeApi({
      'POST /issues/5/comments': {
        body: {
          comment: { id: 9, version: 1 },
          messages: [{ rule: 'plus_one_vote', code: 'vote_added', params: {} }],
        },
      },
    });
    const posted = await postComment(5, { text: '+1', uploads: [], audience: null });
    expect(posted.id).toBe(9);
    expect(toasts.items.map((toast) => toast.message)).toEqual(['+1 counts as a vote']);
  });

  it('an unknown code still says something', async () => {
    fakeApi({
      'PUT /issues/5/fields/2': {
        body: { issue: { id: 5 }, messages: [{ rule: 'new_rule', code: 'new_code', params: {} }] },
      },
    });
    await sendField(5, 2, { kind: 'set', value: null });
    expect(toasts.items.map((toast) => toast.message)).toEqual(['A rule ran']);
  });

  it('AUT-071: what the duplicates rule did toasts with the names', async () => {
    fakeApi({
      'PUT /issues/5/fields/2': {
        body: {
          issue: { id: 5 },
          messages: [
            { rule: 'duplicates', code: 'link_reattached', params: { issue: 'WEB-9' } },
            { rule: 'duplicates', code: 'priority_raised', params: { value: 'Major' } },
          ],
        },
      },
    });
    await sendField(5, 2, { kind: 'set', value: null });
    expect(toasts.items.map((toast) => toast.message)).toEqual([
      'Duplicates now point to WEB-9',
      'Priority raised to “Major”',
    ]);
  });
});

describe('AUT-066: клонирование — тосты', () => {
  it('the copy’s toast names it and «Открыть» follows the message’s issue', async () => {
    const copy = issue({ id: 502, readable_id: 'WEB-13', number: 13, summary: 'A copy' });
    fakeApi({
      'POST /issues/501/clone': {
        status: 201,
        body: {
          issue: copy,
          messages: [{ rule: 'clone_issue', code: 'clone_created', params: { issue: 'WEB-13' } }],
        },
      },
    });

    await cloneIssue(issue());

    expect(toasts.items.map((toast) => toast.message)).toEqual(['Copy created: WEB-13']);
    toasts.items[0]?.action?.run();
    expect(navigation.goto).toHaveBeenCalledWith('/issues/WEB-13');
    expect(store.entities.table('issue').get(502)?.summary).toBe('A copy');
  });

  it('a clone as draft toasts per source, without a link', async () => {
    fakeApi({
      'POST /issues/clone-drafts': {
        body: {
          drafts: [issue({ id: 601, draft: true, readable_id: null, number: null })],
          messages: [{ rule: 'clone_issue', code: 'draft_saved', params: { issue: 'WEB-12' } }],
        },
      },
    });

    await cloneAsDrafts([issue()]);

    expect(toasts.items.map((toast) => toast.message)).toEqual(['Draft copied from WEB-12']);
    expect(toasts.items[0]?.action).toBeUndefined();
    expect(store.entities.table('issue').get(601)?.draft).toBe(true);
  });
});

describe('HIST: правило — бейдж записи', () => {
  it('a rule-made record carries the badge; a person-made one does not', async () => {
    const ruleMade = await render(ChangeRecord, { change: change('rule'), issue: ISSUE });
    await expect.element(ruleMade.getByText('rule')).toBeVisible();
    await expectAccessible(ruleMade.container);
    await ruleMade.unmount();

    const byHand = await render(ChangeRecord, { change: change('web'), issue: ISSUE });
    await expect.element(page.getByText('rule')).not.toBeInTheDocument();
    await expectAccessible(byHand.container);
  });
});
