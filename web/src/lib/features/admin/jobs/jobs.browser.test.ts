// The «Задания» screen on a desktop and on a phone (ADM-031, ADM-032; plan step 1.11 §6.2, §7.3):
// the rows newest first with the kind in our words, the state chips and the free-text filter
// (a leading `-` excludes), the cancel confirmation, retry of a failed job, «Показать ещё», and
// the refresh after an action.
import { afterEach, describe, expect, it, vi } from 'vitest';
import { page } from 'vitest/browser';
import { render } from 'vitest-browser-svelte';

import type { AccessMap, AdminJob, AdminJobs, Session, UserRef } from '#lib/api/schema.gen.ts';
import { BASE_LOCALE, setLocale } from '#lib/i18n/locale.svelte.ts';
import { setAccessForTests, setSessionForTests } from '#lib/session/index.ts';
import { toasts } from '#lib/ui/toast.svelte.ts';

import { expectAccessible, isPhone, smallTargets } from '../../../../test/a11y.ts';
import { fakeApi, type FakeRequest } from '../../../../test/fake-api.ts';
import JobsPage from './JobsPage.svelte';

const SESSION: Session = {
  user: {
    id: 1,
    username: 'anna',
    full_name: 'Anna Admin',
    email: 'anna@example.com',
    email_verified: true,
    avatar_color: 0,
    avatar_url: null,
    version: 1,
  },
  remember: false,
  created_at: '2026-11-08T09:00:00Z',
  password_change_required: false,
};

const ADMIN: AccessMap = {
  global: ['system.read', 'system.write'],
  projects: [],
  capabilities: [],
};
const READER: AccessMap = { global: ['system.read'], projects: [], capabilities: [] };

function initiator(name: string): UserRef {
  return {
    id: 7,
    full_name: name,
    username: name.toLowerCase().split(' ')[0] ?? 'x',
    avatar_color: 1,
    avatar_url: null,
    disabled: false,
    kind: 'person',
    pseudonym: null,
  };
}

function job(id: number, extra: Partial<AdminJob> = {}): AdminJob {
  return {
    id,
    kind: 'project.delete',
    state: 'finished',
    initiator: null,
    cancellable: true,
    cancel_requested: false,
    attempts: 0,
    max_attempts: 1,
    progress: { done: 0, total: null },
    status_text: null,
    last_error: null,
    created_at: '2026-11-09T10:00:00Z',
    started_at: '2026-11-09T10:00:01Z',
    finished_at: '2026-11-09T10:00:05Z',
    ...extra,
  };
}

/** The jobs as the fake server holds them; `answers` may change them per route. */
function server(jobs: AdminJob[]) {
  setSessionForTests(SESSION);
  setAccessForTests(ADMIN);
  let state = [...jobs];
  const calls: FakeRequest[] = fakeApi({
    'GET /admin/jobs': (request) => {
      // Newest first, `before` walks to older ids — the shape the real keyset answers with.
      const before = Number(request.query.get('before') ?? '0');
      const q = request.query.get('q');
      const stateFilter = request.query.get('state');
      let rows = state
        .filter((one) => one.id < (before || Number.MAX_SAFE_INTEGER))
        .sort((a, b) => b.id - a.id);
      if (stateFilter) rows = rows.filter((one) => one.state === stateFilter);
      if (q) {
        for (const word of q.split(/\s+/).filter(Boolean)) {
          if (word.startsWith('-')) {
            const rest = word.slice(1);
            rows = rows.filter(
              (one) =>
                !one.kind.includes(rest) &&
                !(one.initiator?.full_name ?? '').toLowerCase().includes(rest.toLowerCase()),
            );
          } else {
            rows = rows.filter(
              (one) =>
                one.kind.includes(word) ||
                (one.initiator?.full_name ?? '').toLowerCase().includes(word.toLowerCase()),
            );
          }
        }
      }
      const items = rows.slice(0, 2);
      const last = items[items.length - 1];
      const answer: AdminJobs = { items, next: rows.length > 2 && last ? last.id : null };
      return { body: answer };
    },
    'POST /admin/jobs/2/cancel': () => {
      state = state.map((one) =>
        one.id === 2 ? { ...one, state: 'cancelled', cancel_requested: false } : one,
      );
      return { body: state.find((one) => one.id === 2) };
    },
    'POST /admin/jobs/3/retry': () => {
      const failed = state.find((one) => one.id === 3) ?? job(3);
      const fresh: AdminJob = { ...failed, id: 9, state: 'queued' };
      state = [fresh, ...state];
      return { status: 201, body: fresh };
    },
  });
  return calls;
}

afterEach(() => {
  vi.unstubAllGlobals();
  setSessionForTests(null);
  setAccessForTests(null);
  setLocale(BASE_LOCALE);
  toasts.clear();
});

describe('JobsPage', () => {
  it('ADM-031: newest first, the kind in our words, the target, the initiator and the state', async () => {
    server([
      job(1, { kind: 'mail.send' }),
      job(2, { state: 'queued', kind: 'project.delete', initiator: initiator('Dana Doe') }),
      job(3, {
        state: 'failed',
        kind: 'issues.fields',
        last_error: 'the set is gone',
        attempts: 3,
        max_attempts: 3,
      }),
      job(4, {
        state: 'running',
        kind: 'content.render',
        progress: { done: 12, total: 34 },
        status_text: 'TF-12: fix the login form',
      }),
      job(5, { state: 'cancelled', kind: 'files.remove' }),
    ]);

    const screen = await render(JobsPage);

    // The first page holds the two newest; the oldest is pages away.
    await expect.element(screen.getByText('File removal')).toBeVisible();
    await expect.element(screen.getByText('Text re-rendering')).toBeVisible();
    await expect.element(screen.getByText('TF-12: fix the login form')).toBeVisible();
    await expect.element(screen.getByText('12 of 34')).toBeVisible();
    expect(screen.getByText('Email').query()).toBeNull();

    // «Показать ещё» walks to the older rows.
    await screen.getByRole('button', { name: 'Show more' }).click();
    await expect.element(screen.getByText('Field values in issues')).toBeVisible();
    await expect.element(screen.getByText('the set is gone')).toBeVisible();
    await expect.element(screen.getByText('Dana Doe')).toBeVisible();
    await screen.getByRole('button', { name: 'Show more' }).click();
    await expect.element(screen.getByText('Email')).toBeVisible();
    // A failed row says why; the attempt counter is for the ones still going.
    expect(screen.getByText('try 4 of 3').query()).toBeNull();
  });

  it('ADM-032: the state chips and the free-text filter ride the request; a leading `-` excludes', async () => {
    const calls = server([
      job(1, { kind: 'mail.send', initiator: initiator('Dana Doe') }),
      job(2, { state: 'queued', kind: 'project.delete' }),
    ]);

    const screen = await render(JobsPage);
    await expect.element(screen.getByText('Email')).toBeVisible();
    await expect.element(screen.getByText('Project deletion')).toBeVisible();

    await screen.getByRole('button', { name: 'Queued', exact: true }).click();
    expect(
      calls
        .filter((call) => call.path === '/admin/jobs')
        .at(-1)
        ?.query.get('state'),
    ).toBe('queued');
    await expect.element(screen.getByText('Project deletion')).toBeVisible();
    expect(screen.getByText('Email').query()).toBeNull();

    await screen.getByRole('button', { name: 'All', exact: true }).click();
    await screen.getByRole('searchbox', { name: 'Filter jobs' }).fill('mail');
    await vi.waitFor(() => {
      expect(
        calls
          .filter((call) => call.path === '/admin/jobs')
          .at(-1)
          ?.query.get('q'),
      ).toBe('mail');
    });
    await expect.element(screen.getByText('Email')).toBeVisible();

    // A word with a leading minus excludes: the mail row goes, the deletion row stays.
    await screen.getByRole('searchbox', { name: 'Filter jobs' }).fill('-mail');
    await expect.element(screen.getByText('Project deletion')).toBeVisible();
    expect(screen.getByText('Email').query()).toBeNull();
    if (isPhone()) expect(await smallTargets(screen.container)).toEqual([]);
  });

  it('ADM-031: cancelling asks, then refreshes the list; retry of a failed job queues a fresh one', async () => {
    const calls = server([
      job(1, { kind: 'mail.send' }),
      job(2, { state: 'queued', kind: 'project.delete' }),
      job(3, { state: 'failed', kind: 'issues.fields' }),
    ]);

    const screen = await render(JobsPage);

    // A queued job may be cancelled; the dialog says what that means, and No keeps it.
    await screen.getByRole('button', { name: 'Cancel' }).click();
    const dialog = page.getByRole('dialog', { name: 'Cancel the job?' });
    await expect
      .element(dialog.getByText('The job has not started yet; it will not run.'))
      .toBeVisible();
    await dialog.getByRole('button', { name: 'Cancel', exact: true }).click();
    expect(calls.filter((call) => call.path === '/admin/jobs/2/cancel')).toHaveLength(0);

    await screen.getByRole('button', { name: 'Cancel' }).click();
    await dialog.getByRole('button', { name: 'Cancel job' }).click();
    expect(calls.filter((call) => call.path === '/admin/jobs/2/cancel')).toHaveLength(1);
    expect(toasts.items.at(-1)?.message).toBe('The job is cancelled');

    // Retry of the failed job queues a fresh one.
    await screen.getByRole('button', { name: 'Retry' }).click();
    expect(calls.filter((call) => call.path === '/admin/jobs/3/retry')).toHaveLength(1);
    expect(toasts.items.at(-1)?.message).toBe('A fresh job is queued');
    await screen.getByRole('button', { name: 'Show more' }).click();
    await expect.element(screen.getByLabelText('Cancelled')).toBeVisible();
  });

  it('ADM-031: a finished job has no actions; a running one that cannot be stopped has none either', async () => {
    server([job(2, { state: 'running', cancellable: false })]);

    const screen = await render(JobsPage);

    await expect.element(screen.getByText('Project deletion')).toBeVisible();
    expect(screen.getByRole('button', { name: 'Cancel' }).query()).toBeNull();
    expect(screen.getByRole('button', { name: 'Retry' }).query()).toBeNull();
  });

  it('ADM-031: an empty list says so; a reader without system.write sees no actions', async () => {
    setSessionForTests(SESSION);
    setAccessForTests(READER);
    fakeApi({ 'GET /admin/jobs': () => ({ body: { items: [], next: null } }) });

    const screen = await render(JobsPage);

    await expect
      .element(
        screen.getByText(
          'No jobs yet. Long work — deleting a project, sending letters — will appear here.',
        ),
      )
      .toBeVisible();
    await expect
      .element(screen.getByText('Only system administrators can act on jobs.'))
      .toBeVisible();
    // Reading is theirs too: the refresh stays, the actions do not.
    await expect.element(screen.getByRole('button', { name: 'Refresh' })).toBeVisible();
  });

  it('ADM-031: the screen is accessible on both profiles; Russian texts', async () => {
    setLocale('ru');
    server([job(1, { kind: 'mail.send' })]);

    const screen = await render(JobsPage);

    await expect.element(screen.getByText('Письмо')).toBeVisible();
    await expect.element(screen.getByRole('button', { name: 'В очереди' })).toBeVisible();
    await expect.element(screen.getByLabelText('Завершено')).toBeVisible();
    await expect.element(screen.getByText('Запустил')).toBeVisible();
    await expect.element(screen.getByText('Сам сервер')).toBeVisible();
    expect(await smallTargets(screen.container, isPhone() ? 44 : 32)).toEqual([]);
    await expectAccessible(screen.container);
  });
});
