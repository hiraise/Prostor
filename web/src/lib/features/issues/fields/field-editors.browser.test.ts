// Editing fields in the field panel, one kind at a time (ISS-019, FLD-002…FLD-014, FLD-043,
// FLD-047, FLD-057): the value shows at once, the request sent, the rollback on failure.
import { afterEach, describe, expect, it } from 'vitest';
import { page, userEvent } from 'vitest/browser';
import { render } from 'vitest-browser-svelte';

import type { HeldValue, IssueField, OptionValue } from '#lib/api/schema.gen.ts';
import {
  setAccessForTests,
  setPreferencesForTests,
  setSessionForTests,
} from '#lib/session/index.ts';
import { store } from '#lib/store/index.ts';
import { announcement } from '#lib/ui/announce.svelte.ts';

import { expectAccessible } from '../../../../test/a11y.ts';
import { failure, fakeApi, type FakeRoute } from '../../../../test/fake-api.ts';
import FieldPanelHarness from '../../../../test/harness/FieldPanelHarness.svelte';
import { ANNA, BORIS, field, issue } from '../test-data.ts';

const ID = 501;

function value(id: number, name: string, rest: Partial<HeldValue> = {}): HeldValue {
  return { id, name, color: null, description: '', archived: false, resolved: false, ...rest };
}

function offered(held: HeldValue, rest: Partial<OptionValue> = {}): OptionValue {
  return { ...held, released: false, release_date: null, ...rest };
}

function options(rest: Record<string, unknown> = {}): FakeRoute {
  return {
    body: { values: [], users: [], groups: [], more: false, can_add_value: false, set: 9, ...rest },
  };
}

/** Answers a change with the issue as the panel shows it now. */
const saved: FakeRoute = () => ({
  body: { issue: store.entities.table('issue').get(ID), messages: [] },
});

async function show(entry: IssueField, routes: Record<string, FakeRoute> = {}) {
  const calls = fakeApi(routes);
  store.entities.table('issue').upsert(issue({ fields: [entry] }), 'full');
  const screen = await render(FieldPanelHarness, { id: ID });
  return { screen, calls };
}

/** The value button of the only field row. */
function valueButton(name: RegExp | string) {
  return page.getByRole('button', { name });
}

function sent(calls: { method: string; path: string; body: unknown }[]) {
  return calls
    .filter((call) => call.method !== 'GET')
    .map(({ method, path, body }) => ({
      method,
      path,
      body,
    }));
}

afterEach(() => {
  store.entities.table('issue').clear();
  setAccessForTests(null);
  setPreferencesForTests(null);
  setSessionForTests(null);
});

setAccessForTests({ global: [], projects: [], capabilities: [] });
setPreferencesForTests({});

describe('FLD-002: string field', () => {
  it('saves on Enter, shows the value at once and announces it', async () => {
    const { calls } = await show(field(16, 'Browser', 'string'), {
      'PUT /issues/501/fields/16': saved,
    });
    await valueButton('No value').click();
    const input = page.getByRole('textbox', { name: 'Browser' });
    await expect.element(input).toHaveFocus();
    await expectAccessible(page.getByRole('dialog').element());
    await userEvent.keyboard('Chrome 130{Enter}');

    await expect.element(valueButton('Chrome 130')).toBeVisible();
    await expect.element(valueButton('Chrome 130')).toHaveFocus();
    expect(sent(calls)).toEqual([
      { method: 'PUT', path: '/issues/501/fields/16', body: { value: 'Chrome 130' } },
    ]);
    await expect.poll(() => announcement('polite')).toMatch(/^Saved: Browser/);
  });

  it('closes on Escape without saving', async () => {
    const { calls } = await show(field(16, 'Browser', 'string', { text: 'Safari' }));
    await valueButton('Safari').click();
    await userEvent.keyboard('{Control>}a{/Control}Edge{Escape}');
    await expect.element(page.getByRole('dialog')).not.toBeInTheDocument();
    await expect.element(valueButton('Safari')).toBeVisible();
    expect(sent(calls)).toEqual([]);
  });
});

describe('FLD-004: integer and float fields', () => {
  it('refuses fractions in integers and takes a comma in floats', async () => {
    const { calls } = await show(field(17, 'Points', 'integer'), {
      'PUT /issues/501/fields/17': saved,
    });
    await valueButton('No value').click();
    const input = page.getByRole('textbox', { name: 'Points' });
    await expect.element(input).toHaveAttribute('inputmode', 'numeric');
    await userEvent.keyboard('2,5{Enter}');
    await expect.element(input).toHaveAccessibleDescription('Type a whole number.');
    expect(sent(calls)).toEqual([]);
    await userEvent.fill(input, '3');
    await userEvent.keyboard('{Enter}');
    await expect.element(valueButton('3')).toBeVisible();
    expect(sent(calls)).toEqual([
      { method: 'PUT', path: '/issues/501/fields/17', body: { value: 3 } },
    ]);
  });

  it('saves a float typed with a comma', async () => {
    const { calls } = await show(field(18, 'Score', 'float'), {
      'PUT /issues/501/fields/18': saved,
    });
    await valueButton('No value').click();
    await userEvent.keyboard('2,5{Enter}');
    await expect.element(valueButton('2.5')).toBeVisible();
    expect(sent(calls)[0]?.body).toEqual({ value: 2.5 });
  });
});

describe('FLD-005: date field', () => {
  it('reads ISO, sends the date and offers the calendar', async () => {
    const { calls } = await show(field(14, 'Due date', 'date'), {
      'PUT /issues/501/fields/14': saved,
    });
    await valueButton('No value').click();
    await expect.element(page.getByRole('button', { name: 'Pick on the calendar' })).toBeVisible();
    await userEvent.keyboard('2026-11-05{Enter}');
    await expect.element(valueButton(/Nov 5/)).toBeVisible();
    expect(sent(calls)[0]?.body).toEqual({ value: '2026-11-05' });
  });

  it('says what is wrong with a date that does not exist', async () => {
    await show(field(14, 'Due date', 'date'));
    await valueButton('No value').click();
    await userEvent.keyboard('2026-02-30{Enter}');
    await expect
      .element(page.getByRole('textbox', { name: 'Due date' }))
      .toHaveAccessibleDescription(/Type a date\./);
  });
});

describe('FLD-006: date-time field', () => {
  it('sends the instant', async () => {
    const { calls } = await show(field(19, 'Deadline', 'date_time'), {
      'PUT /issues/501/fields/19': saved,
    });
    await valueButton('No value').click();
    await userEvent.keyboard('2026-11-05 10:30{Enter}');
    await expect.element(page.getByRole('dialog')).not.toBeInTheDocument();
    const body = sent(calls)[0]?.body as { value: string };
    expect(body.value).toMatch(/^2026-11-0\dT\d\d:30:00\.000Z$/);
  });
});

describe('FLD-007: period field', () => {
  it('reads a period and sends minutes', async () => {
    const { calls } = await show(field(13, 'Estimation', 'duration', { minutes: 90 }), {
      'PUT /issues/501/fields/13': saved,
    });
    await valueButton('1h 30m').click();
    await expect.element(page.getByRole('textbox', { name: 'Estimation' })).toHaveValue('1h 30m');
    await userEvent.fill(page.getByRole('textbox', { name: 'Estimation' }), '1d 2h');
    await userEvent.keyboard('{Enter}');
    await expect.element(valueButton('1d 2h')).toBeVisible();
    expect(sent(calls)[0]?.body).toEqual({ value: 600 });
  });
});

const MAJOR = value(101, 'Major');
const CRITICAL = value(100, 'Critical', { description: 'Drop everything' });

describe('FLD-008, FLD-043: enumeration field', () => {
  it('chooses a value, shows descriptions and saves at once', async () => {
    const { calls } = await show(field(11, 'Priority', 'enum', { values: [MAJOR] }), {
      'GET /issues/501/fields/11/options': options({
        values: [offered(CRITICAL), offered(MAJOR)],
      }),
      'PUT /issues/501/fields/11': saved,
    });
    await valueButton('Major').click();
    await expect.element(page.getByRole('radio', { name: 'No value' })).toBeVisible();
    // FLD-043: the description is the option's second line.
    await page.getByRole('radio', { name: 'Critical Drop everything' }).click();

    await expect.element(page.getByRole('dialog')).not.toBeInTheDocument();
    await expect.element(valueButton('Critical')).toBeVisible();
    expect(sent(calls)).toEqual([
      { method: 'PUT', path: '/issues/501/fields/11', body: { value: 100 } },
    ]);
  });

  it('rolls back and says why when the server refuses', async () => {
    await show(field(11, 'Priority', 'enum', { values: [MAJOR] }), {
      'GET /issues/501/fields/11/options': options({ values: [offered(CRITICAL)] }),
      'PUT /issues/501/fields/11': failure(403, 'forbidden'),
    });
    await valueButton('Major').click();
    await page.getByRole('radio', { name: /Critical/ }).click();
    await expect.element(valueButton('Major')).toBeVisible();
    await expect.element(valueButton('Major')).toHaveAccessibleDescription(/Priority .+/);
  });
});

describe('FLD-009, FLD-057: state field', () => {
  it('a field that cannot be empty has no empty choice', async () => {
    const open = value(201, 'Open');
    const fixed = value(202, 'Fixed', { resolved: true });
    const { calls } = await show(field(20, 'State', 'state', { values: [open], required: true }), {
      'GET /issues/501/fields/20/options': options({ values: [offered(open), offered(fixed)] }),
      'PUT /issues/501/fields/20': saved,
    });
    await valueButton('Open').click();
    await expect.element(page.getByRole('radio', { name: 'Fixed' })).toBeVisible();
    expect(page.getByRole('radio', { name: 'No value' }).query()).toBeNull();
    await page.getByRole('radio', { name: 'Fixed' }).click();
    await expect.element(valueButton('Fixed')).toBeVisible();
    expect(sent(calls)[0]?.body).toEqual({ value: 202 });
  });

  it('an empty required field says so', async () => {
    await show(field(20, 'State', 'state', { required: true }));
    await expect.element(valueButton('Required field')).toBeVisible();
  });
});

describe('FLD-010: user field', () => {
  it('offers people with the viewer marked, and the empty choice', async () => {
    setSessionForTests({
      user: { ...ANNA, email: null, email_verified: false, version: 1 },
      remember: false,
      created_at: '2026-10-22T09:00:00Z',
      password_change_required: false,
    });
    const { calls } = await show(
      field(12, 'Assignee', 'user', { users: [BORIS], empty_label: 'Unassigned' }),
      {
        'GET /issues/501/fields/12/options': options({ users: [ANNA, BORIS] }),
        'PUT /issues/501/fields/12': saved,
      },
    );
    await valueButton(/Boris Lead/).click();
    await expect.element(page.getByRole('radio', { name: /Anna Admin.*me/ })).toBeVisible();
    await page.getByRole('radio', { name: 'Unassigned' }).click();
    await expect.element(valueButton('Unassigned')).toBeVisible();
    expect(sent(calls)[0]?.body).toEqual({ value: null });
  });
});

describe('FLD-011: group field', () => {
  it('chooses a group', async () => {
    const team = { id: 31, name: 'Designers', builtin: null, team_of: null };
    const { calls } = await show(field(21, 'Reviewed by', 'group'), {
      'GET /issues/501/fields/21/options': options({ groups: [team] }),
      'PUT /issues/501/fields/21': saved,
    });
    await valueButton('No value').click();
    await page.getByRole('radio', { name: 'Designers' }).click();
    await expect.element(valueButton('Designers')).toBeVisible();
    expect(sent(calls)[0]?.body).toEqual({ value: 31 });
  });
});

describe('FLD-012: component field', () => {
  it('adds and removes values of a multi-valued field at once and stays open', async () => {
    const ui = value(301, 'UI');
    const api = value(302, 'API');
    const entry = field(22, 'Component', 'owned', { values: [ui] });
    entry.field.multi = true;
    const { calls } = await show(entry, {
      'GET /issues/501/fields/22/options': options({ values: [offered(ui), offered(api)] }),
      'POST /issues/501/fields/22/add': saved,
      'POST /issues/501/fields/22/remove': saved,
    });
    await valueButton('UI').click();
    await page.getByRole('checkbox', { name: 'API' }).click();
    await expect.element(page.getByRole('checkbox', { name: 'API' })).toBeChecked();
    await page.getByRole('checkbox', { name: 'UI' }).click();
    await expect.element(page.getByRole('checkbox', { name: 'UI' })).not.toBeChecked();
    await expect.element(page.getByRole('dialog')).toBeVisible();
    expect(sent(calls)).toEqual([
      { method: 'POST', path: '/issues/501/fields/22/add', body: { ids: [302] } },
      { method: 'POST', path: '/issues/501/fields/22/remove', body: { ids: [301] } },
    ]);
    await userEvent.keyboard('{Escape}');
    await expect.element(valueButton('API')).toBeVisible();
  });
});

describe('FLD-013, FLD-047: version field', () => {
  it('shows whether a version is released and when it is due', async () => {
    const v1 = value(401, '1.0');
    const v2 = value(402, '2.0');
    await show(field(23, 'Fix versions', 'version'), {
      'GET /issues/501/fields/23/options': options({
        values: [offered(v1, { released: true }), offered(v2, { release_date: '2026-12-01' })],
      }),
    });
    await valueButton('No value').click();
    await expect.element(page.getByRole('radio', { name: '1.0 released' })).toBeVisible();
    await expect.element(page.getByRole('radio', { name: /^2\.0 release Dec 1/ })).toBeVisible();
  });
});

describe('FLD-014: build field', () => {
  it('chooses a build', async () => {
    const build = value(501, '2026.10.1');
    const { calls } = await show(field(24, 'Fixed in build', 'build'), {
      'GET /issues/501/fields/24/options': options({ values: [offered(build)] }),
      'PUT /issues/501/fields/24': saved,
    });
    await valueButton('No value').click();
    await page.getByRole('radio', { name: '2026.10.1' }).click();
    await expect.element(valueButton('2026.10.1')).toBeVisible();
    expect(sent(calls)[0]?.body).toEqual({ value: 501 });
  });
});

describe('ISS-019: inline editing', () => {
  it('lists a held archived value, marked and removable, and never offers it', async () => {
    const old = value(103, 'Legacy', { archived: true });
    const entry = field(25, 'Tags of area', 'enum', { values: [old, MAJOR] });
    entry.field.multi = true;
    const { calls } = await show(entry, {
      'GET /issues/501/fields/25/options': options({ values: [offered(MAJOR)] }),
      'POST /issues/501/fields/25/remove': saved,
    });
    await valueButton(/Legacy/).click();
    const archived = page.getByRole('checkbox', { name: 'Legacy archived' });
    await expect.element(archived).toBeChecked();
    await expect.element(page.getByRole('checkbox', { name: 'Major' })).toBeVisible();
    expect(page.getByRole('checkbox').elements()).toHaveLength(2);
    await archived.click();
    await expect.element(archived).not.toBeInTheDocument();
    await expect.element(page.getByRole('searchbox')).toHaveFocus();
    expect(sent(calls)[0]).toEqual({
      method: 'POST',
      path: '/issues/501/fields/25/remove',
      body: { ids: [103] },
    });
  });

  it('adds a typed name as a new value and chooses it', async () => {
    const blocker = { ...offered(value(109, 'Blocker')), owner: null, start_date: null };
    const { calls } = await show(field(11, 'Priority', 'enum', { values: [MAJOR] }), {
      'GET /issues/501/fields/11/options': ({ query }) =>
        options({ values: query.get('q') ? [] : [offered(MAJOR)], can_add_value: true }) as {
          body: unknown;
        },
      'POST /value-sets/9/values': {
        body: {
          id: 9,
          name: 'Priorities',
          values: [{ ...blocker, assembled_on: null, version: 1 }],
        },
      },
      'PUT /issues/501/fields/11': saved,
    });
    await valueButton('Major').click();
    await userEvent.keyboard('Blocker');
    await page.getByRole('button', { name: 'Add “Blocker”' }).click();
    await expect.element(valueButton('Blocker')).toBeVisible();
    expect(sent(calls)).toEqual([
      { method: 'POST', path: '/value-sets/9/values', body: { name: 'Blocker' } },
      { method: 'PUT', path: '/issues/501/fields/11', body: { value: 109 } },
    ]);
  });

  it('a field the viewer may not change has no button', async () => {
    await show(field(11, 'Priority', 'enum', { values: [MAJOR], editable: false }));
    await expect.element(page.getByText('Major')).toBeVisible();
    expect(page.getByRole('button', { name: 'Major' }).query()).toBeNull();
  });
});
