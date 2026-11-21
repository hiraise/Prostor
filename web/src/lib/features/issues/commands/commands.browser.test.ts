// The command dialog (step 2.1 §6, CMD-027…CMD-030): the preview and its marks while typing,
// problems blocking the apply with their quick fixes, completion at the caret, the comment with
// its visibility, silent for those who may, the delete confirm step, the recent lines, and the
// bulk flow with its progress and the silent-by-default prompt. Every case runs on a desktop and
// on a phone (D-019).
import { afterEach, beforeAll, describe, expect, it, vi } from 'vitest';
import { userEvent } from 'vitest/browser';
import { render } from 'vitest-browser-svelte';

import type { AccessMap, CommandAssist, Issue, Session } from '#lib/api/schema.gen.ts';
import { BASE_LOCALE, setLocale } from '#lib/i18n/locale.svelte.ts';
import { active, resetShortcutsForTests } from '#lib/shortcuts/registry.svelte.ts';
import {
  setAccessForTests,
  setPreferencesForTests,
  setSessionForTests,
} from '#lib/session/index.ts';
import { toasts } from '#lib/ui/toast.svelte.ts';

import { expectAccessible, isPhone, smallTargets } from '../../../../test/a11y.ts';
import { fakeApi, type FakeRequest, type FakeRoute } from '../../../../test/fake-api.ts';
import { ANNA, issue } from '../test-data.ts';
import CommandDialog from './CommandDialog.svelte';
import { recentCommands } from './recent.svelte.ts';

const TARGET = issue();

const SESSION = {
  created_at: '2026-11-14T09:00:00Z',
  password_change_required: false,
  remember: false,
  user: { ...ANNA, email: null, email_verified: true, version: 1 },
} as unknown as Session;

const WITH_SILENT: AccessMap = {
  global: [],
  projects: [{ project_id: 1, permissions: ['project.basic', 'issue.silent'] }],
  capabilities: [],
};

interface Shown {
  screen: Awaited<ReturnType<typeof render>>;
  calls: FakeRequest[];
}

/** The assist's answer the fake server gives; the preview and the popup build on it. */
function assist(answer: Partial<CommandAssist>): FakeRoute {
  return {
    body: {
      operations: [],
      problems: [],
      suggestions: [],
      ...answer,
    },
  };
}

interface BulkTarget {
  query: string;
  project: number | null;
  hideResolved: boolean;
  count: number;
}

async function show(
  routes: Record<string, FakeRoute> = {},
  props: { preset?: string; issues?: Issue[]; bulk?: BulkTarget | null } = {},
): Promise<Shown> {
  const calls = fakeApi({
    'PATCH /users/me/preferences': { body: {} },
    'POST /commands/assist': assist({}),
    ...routes,
  });
  setSessionForTests(SESSION);
  setAccessForTests(WITH_SILENT);
  const screen = await render(CommandDialog, {
    props: {
      open: true,
      preset: props.preset,
      issues: props.issues ?? [TARGET],
      bulk: props.bulk ?? null,
      onclose: () => undefined,
    },
  });
  return { screen, calls };
}

function assistCalls(calls: FakeRequest[]): FakeRequest[] {
  return calls.filter((call) => call.path === '/commands/assist');
}

function applyCalls(calls: FakeRequest[]): FakeRequest[] {
  return calls.filter((call) => call.path === '/commands');
}

beforeAll(() => {
  setLocale(BASE_LOCALE);
});

afterEach(() => {
  vi.unstubAllGlobals();
  setAccessForTests(null);
  setSessionForTests(null);
  setPreferencesForTests(null);
  recentCommands.reset();
  resetShortcutsForTests();
  toasts.clear();
});

describe('CMD-028: preview and problems while typing', () => {
  it('shows the operations as rows with their marks, and the removals of a plain set', async () => {
    const { screen } = await show({
      'POST /commands/assist': assist({
        operations: [
          {
            kind: 'set_field',
            start: 0,
            end: 18,
            names: ['Fix versions', '2025.3'],
            mode: 'set',
            removals: ['2025.1', '2025.2'],
          },
          { kind: 'tag', start: 19, end: 27, names: ['bug'], mode: 'add', removals: [] },
        ],
      }),
    });
    const rows = screen.getByRole('region', { name: 'Preview' });
    await expect.element(rows).toBeVisible();
    // Phones fold the preview to its count; open it before reading the rows.
    if (isPhone()) {
      await userEvent.click(screen.getByRole('button', { name: /change/ }));
    }
    await expect.element(screen.getByText('Fix versions')).toBeVisible();
    await expect.element(screen.getByText('+2025.3')).toBeVisible();
    await expect.element(screen.getByText('−2025.1')).toBeVisible();
    await expect.element(screen.getByText('−2025.2')).toBeVisible();
    await expect.element(screen.getByText('+bug')).toBeVisible();
  });

  it('says what a problem is and blocks the apply until the line resolves', async () => {
    const { screen } = await show(
      {
        'POST /commands/assist': assist({
          problems: [
            {
              code: 'cmd_unknown_value',
              start: 0,
              end: 5,
              params: { field: 'State' },
              quick_fix: { name: 'Broke', field: 3, projects: [1] },
            },
          ],
        }),
      },
      { preset: 'state Broke' },
    );
    const apply = screen.getByRole('button', { name: 'Apply' });
    await expect.element(apply).toBeDisabled();
    await expect.element(screen.getByText(/No such value in/)).toBeVisible();

    // The quick fix excuses the problem: the apply makes the value first (§5).
    await userEvent.click(screen.getByRole('button', { name: /Create “Broke”/ }));
    await expect.element(apply).toBeEnabled();
  });

  it('sends the comment and its visibility with the apply', async () => {
    const { screen, calls } = await show({
      'POST /commands/assist': assist({
        operations: [{ kind: 'comment', start: 0, end: 7, names: [], mode: null, removals: [] }],
      }),
      'POST /commands': {
        body: {
          operations: [],
          problems: [],
          issues: [{ issue: 501, ok: true, problem: null, messages: [] }],
        },
      },
    });
    await userEvent.fill(screen.getByRole('combobox', { name: 'Command' }), 'comment');
    const comment = screen.getByRole('textbox', { name: 'Comment' });
    await userEvent.fill(comment, 'Done in 2025.3');
    const apply = screen.getByRole('button', { name: 'Apply' });
    await expect.element(apply).toBeEnabled();
    await userEvent.click(apply);
    const sent = applyCalls(calls)[0];
    expect(sent?.body).toMatchObject({
      text: 'comment',
      issues: [501],
      comment: 'Done in 2025.3',
      comment_visibility: null,
      silent: false,
      create_missing: false,
    });
  });
});

describe('CMD-029: completion at the caret', () => {
  const SUGGESTED = assist({
    suggestions: [
      {
        text: 'tag bug',
        description: '',
        group: 'recent',
        from: 0,
        to: 1,
        matched_from: 0,
        matched_to: 0,
        caret: 8,
        suffix: ' ',
        present: false,
        resolved: null,
        own: false,
      },
      {
        text: 'Fix versions',
        description: '',
        group: 'attribute',
        from: 0,
        to: 1,
        matched_from: 0,
        matched_to: 0,
        caret: 13,
        suffix: ' ',
        present: false,
        resolved: null,
        own: false,
      },
    ],
  });

  it('arrows move and Tab inserts the chosen line', async () => {
    const { screen, calls } = await show({ 'POST /commands/assist': SUGGESTED });
    const line = screen.getByRole('combobox', { name: 'Command' });
    await userEvent.type(line, 't');
    await vi.waitFor(() => {
      expect(assistCalls(calls).at(-1)?.body).toMatchObject({ text: 't' });
    });
    await userEvent.keyboard('[ArrowDown]');
    await expect.element(screen.getByRole('listbox', { name: 'Suggestions' })).toBeVisible();
    await expect
      .element(screen.getByRole('option', { name: /tag bug/ }))
      .toHaveAttribute('aria-selected', 'true');
    await userEvent.keyboard('[Tab]');
    await expect.element(line).toHaveValue('tag bug ');
  });

  it('marks values already on the issue and labels the groups', async () => {
    const shown = await show({
      'POST /commands/assist': assist({
        suggestions: [
          {
            text: '2025.3',
            description: 'Fix versions',
            group: 'value',
            from: 0,
            to: 0,
            matched_from: 0,
            matched_to: 0,
            caret: 6,
            suffix: ' ',
            present: true,
            resolved: null,
            own: false,
          },
        ],
      }),
    });
    const { screen, calls } = shown;
    await userEvent.type(screen.getByRole('combobox', { name: 'Command' }), '2');
    await vi.waitFor(() => {
      expect(assistCalls(calls).at(-1)?.body).toMatchObject({ text: '2' });
    });
    await userEvent.keyboard('[ArrowDown]');
    await expect.element(screen.getByText('already on the issue')).toBeVisible();
    await expect.element(screen.getByText('Values')).toBeVisible();
  });
});

describe('CMD-023/CMD-025: applying', () => {
  it('applies on Enter, toasts the result and remembers the line', async () => {
    const { screen, calls } = await show(
      {
        'POST /commands': {
          body: {
            operations: [],
            problems: [],
            issues: [
              { issue: 501, ok: true, problem: null, messages: [] },
              {
                issue: 502,
                ok: false,
                problem: { code: 'cmd_no_permission', detail: '' },
                messages: [],
              },
            ],
          },
        },
      },
      { preset: 'tag bug' },
    );
    const line = screen.getByRole('combobox', { name: 'Command' });
    await expect.element(screen.getByRole('button', { name: 'Apply' })).toBeEnabled();
    await userEvent.click(line);
    await userEvent.keyboard('[Enter]');
    await vi.waitFor(() => {
      expect(applyCalls(calls).length).toBe(1);
    });
    expect(applyCalls(calls)[0]?.body).toMatchObject({ text: 'tag bug', issues: [501] });
    expect(recentCommands.last()).toBe('tag bug');
    expect(toasts.items.some((toast) => toast.message.includes('Command applied to 1'))).toBe(true);
  });

  it('keeps the silent button to those with the right, and sends silent', async () => {
    const { screen, calls } = await show(
      {
        'POST /commands': {
          body: {
            operations: [],
            problems: [],
            issues: [{ issue: 501, ok: true, problem: null, messages: [] }],
          },
        },
      },
      { preset: 'tag bug' },
    );
    await expect.element(screen.getByRole('button', { name: 'Apply' })).toBeEnabled();
    await userEvent.click(screen.getByRole('button', { name: 'Apply without notifications' }));
    await vi.waitFor(() => {
      expect(applyCalls(calls).length).toBe(1);
    });
    expect(applyCalls(calls)[0]?.body).toMatchObject({ silent: true });
  });

  it('hides the silent button without the right', async () => {
    fakeApi({
      'PATCH /users/me/preferences': { body: {} },
      'POST /commands/assist': assist({}),
    });
    setSessionForTests(SESSION);
    setAccessForTests({
      global: [],
      projects: [{ project_id: 1, permissions: ['project.basic'] }],
      capabilities: [],
    });
    const screen = await render(CommandDialog, {
      props: { open: true, issues: [TARGET], bulk: null, onclose: () => undefined },
    });
    await vi.waitFor(() => {
      const labels = [...screen.container.querySelectorAll('footer button')].map((button) =>
        button.textContent.trim(),
      );
      expect(labels).not.toContain('Apply without notifications');
    });
  });

  it('reopens with the last applied line', async () => {
    recentCommands.remember('tag bug');
    const { screen } = await show();
    await expect.element(screen.getByRole('combobox', { name: 'Command' })).toHaveValue('tag bug');
  });
});

describe('CMD-020: the delete confirm step', () => {
  it('asks before a line that deletes', async () => {
    const { screen, calls } = await show(
      {
        'POST /commands/assist': assist({
          operations: [{ kind: 'delete', start: 0, end: 6, names: [], mode: null, removals: [] }],
        }),
        'POST /commands': {
          body: {
            operations: [],
            problems: [],
            issues: [{ issue: 501, ok: true, problem: null, messages: [] }],
          },
        },
      },
      { preset: 'delete' },
    );
    await expect.element(screen.getByRole('button', { name: 'Delete', exact: true })).toBeEnabled();
    await userEvent.click(screen.getByRole('button', { name: 'Delete', exact: true }));
    // Step 2.2 §8.2: the confirm names the count it deletes (here the one issue of the dialog).
    await expect.element(screen.getByRole('heading', { name: 'Delete 1 issue?' })).toBeVisible();
    await userEvent.click(screen.getByRole('button', { name: 'Delete', exact: true }));
    await vi.waitFor(() => {
      expect(applyCalls(calls).length).toBe(1);
    });
  });
});

describe('CMD-026: the bulk flow', () => {
  function bulkShown(): Record<string, FakeRoute> {
    let polls = 0;
    return {
      'POST /commands/assist': assist({}),
      'POST /commands/bulk': { body: { job: 9 } },
      'GET /commands/bulk/9': () => {
        polls += 1;
        return polls === 1
          ? {
              body: {
                id: 9,
                state: 'running',
                done: 2,
                total: 5,
                failed: 0,
                failures: [],
                cancel_requested: false,
                last_error: null,
              },
            }
          : {
              body: {
                id: 9,
                state: 'finished',
                done: 5,
                total: 5,
                failed: 1,
                failures: [{ issue: 7, code: 'cmd_no_permission', detail: '' }],
                cancel_requested: false,
                last_error: null,
              },
            };
      },
    };
  }

  async function bulkShow(): Promise<Shown> {
    return show(bulkShown(), {
      preset: 'state Fixed',
      bulk: { query: 'state: Open', project: 1, hideResolved: false, count: 5 },
    });
  }

  it('queues the job over the query and follows its progress', async () => {
    const { screen, calls } = await bulkShow();
    await expect.element(screen.getByRole('button', { name: 'Apply' })).toBeEnabled();
    await userEvent.click(screen.getByRole('button', { name: 'Apply' }));
    const queued = calls.find((call) => call.path === '/commands/bulk');
    expect(queued?.body).toMatchObject({ text: 'state Fixed', query: 'state: Open', project: 1 });
    await expect.element(screen.getByText('Applied to 2 of 5')).toBeVisible();
    await expect.element(screen.getByText('Done: 5, failed: 1')).toBeVisible();
    await expect.element(screen.getByText(/#7/)).toBeVisible();
  });

  it('offers silent-by-default once after a bulk apply without silent', async () => {
    const { screen, calls } = await bulkShow();
    await expect.element(screen.getByRole('button', { name: 'Apply' })).toBeEnabled();
    await userEvent.click(screen.getByRole('button', { name: 'Apply' }));
    await expect
      .element(
        screen.getByRole('heading', { name: 'Apply commands to lists without notifications?' }),
      )
      .toBeVisible();
    await userEvent.click(screen.getByRole('button', { name: 'Yes, silently' }));
    await vi.waitFor(() => {
      const saved = calls.find((call) => call.path === '/users/me/preferences');
      expect(saved?.body).toMatchObject({ commands_silent_prompt: 'silent' });
    });
  });

  it('starts silent when the person chose it once', async () => {
    setPreferencesForTests({ commands_silent_prompt: 'silent' });
    const { screen, calls } = await show(bulkShown(), {
      preset: 'state Fixed',
      bulk: { query: 'state: Open', project: 1, hideResolved: false, count: 5 },
    });
    await expect.element(screen.getByRole('button', { name: 'Apply' })).toBeEnabled();
    await userEvent.click(screen.getByRole('button', { name: 'Apply' }));
    expect(calls.find((call) => call.path === '/commands/bulk')?.body).toMatchObject({
      silent: true,
    });
  });
});

describe('The dialog itself (CMD-027)', () => {
  it('asks the assist for the line and the caret while typing', async () => {
    const { screen, calls } = await show();
    const line = screen.getByRole('combobox', { name: 'Command' });
    await userEvent.type(line, 'tag');
    await vi.waitFor(() => {
      expect(assistCalls(calls).at(-1)?.body).toMatchObject({
        text: 'tag',
        caret: 3,
        issues: [501],
      });
    });
  });

  it('is reachable on a phone: the preview folds to its count and touch targets hold', async () => {
    const { screen } = await show({
      'POST /commands/assist': assist({
        operations: [{ kind: 'tag', start: 0, end: 7, names: ['bug'], mode: 'add', removals: [] }],
      }),
    });
    if (isPhone()) {
      const fold = screen.getByRole('button', { name: /change/ });
      await expect.element(fold).toBeVisible();
      await expect.element(fold).toHaveAttribute('aria-expanded', 'false');
      await userEvent.click(fold);
      await expect.element(screen.getByText('Tag')).toBeVisible();
    } else {
      await expect.element(screen.getByText('Tag')).toBeVisible();
    }
    await expectAccessible();
    expect(await smallTargets(screen.container, isPhone() ? 44 : 32)).toEqual([]);
  });
});

describe('UI-023: the dialog context', () => {
  it('registers while the dialog is open and leaves with it', async () => {
    const { screen } = await show();
    expect(active()).toBe('dialog');
    await screen.getByRole('button', { name: 'Cancel' }).click();
    await vi.waitFor(() => {
      expect(active()).toBeNull();
    });
  });
});
