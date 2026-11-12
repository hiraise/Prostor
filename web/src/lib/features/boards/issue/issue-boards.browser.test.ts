// The boards of an issue on its page and in the new-issue form (step 1.8b-7; ISS-020, ISS-007).
import { afterEach, describe, expect, it, vi } from 'vitest';
import { render } from 'vitest-browser-svelte';

import type { BoardSummary, IssueBoard } from '#lib/api/schema.gen.ts';
import { setAccessForTests, setSessionForTests } from '#lib/session/index.ts';

import { expectAccessible } from '../../../../test/a11y.ts';
import { fakeApi, failure, type FakeRequest, type FakeRoute } from '../../../../test/fake-api.ts';
import { MEMBER, SESSION, board, sprint } from '../test-data.ts';
import IssueBoards from './IssueBoards.svelte';

const BASE = board();

function summary(id: number, name: string): BoardSummary {
  return {
    id,
    name,
    favorite: false,
    owner: BASE.owner,
    projects: BASE.projects.map((one) => one.project),
  };
}

function row(id: number, name: string, extra: Partial<IssueBoard> = {}): IssueBoard {
  return {
    board: { id, name },
    sprint: null,
    sprints_on: false,
    auto: false,
    skipped: false,
    ...extra,
  };
}

const SCRUM = row(9, 'Team', { sprints_on: true, sprint: { id: 11, name: 'Sprint 1' } });

function server(routes: Record<string, FakeRoute>): FakeRequest[] {
  setSessionForTests(SESSION);
  setAccessForTests(MEMBER);
  return fakeApi(routes);
}

afterEach(() => {
  vi.unstubAllGlobals();
  setSessionForTests(null);
  setAccessForTests(null);
});

describe('ISS-020: the boards of an issue', () => {
  it('links each board and sprint and takes the issue off a board', async () => {
    const calls = server({ 'DELETE /issues/501/boards/9': { body: { items: [] } } });
    const screen = await render(IssueBoards, {
      issue: 501,
      project: 1,
      editable: true,
      boards: [SCRUM],
    });

    const section = screen.getByRole('region', { name: 'Boards' });
    await expect
      .element(section.getByRole('link', { name: 'Team' }))
      .toHaveAttribute('href', '/boards/9');
    await expect
      .element(section.getByRole('link', { name: 'Sprint 1' }))
      .toHaveAttribute('href', '/boards/9/sprints/11');
    await expectAccessible(screen.container);

    await screen.getByRole('button', { name: 'Remove from “Team”' }).click();
    await expect.element(section.getByText('Not on boards')).toBeVisible();
    expect(calls.some((call) => call.method === 'DELETE')).toBe(true);
  });

  it('adds the issue to a chosen board and sprint', async () => {
    const calls = server({
      'GET /boards': { body: { items: [summary(9, 'Team'), summary(4, 'Ops')], next: null } },
      'GET /boards/4/sprints': {
        body: {
          items: [
            sprint(20, 'Old', { archived: true }),
            sprint(21, 'Week 1', { current: true }),
            sprint(22, 'Week 2'),
          ],
        },
      },
      'POST /issues/501/boards': {
        body: {
          items: [SCRUM, row(4, 'Ops', { sprints_on: true, sprint: { id: 22, name: 'Week 2' } })],
        },
      },
    });
    const screen = await render(IssueBoards, {
      issue: 501,
      project: 1,
      editable: true,
      boards: [SCRUM],
    });

    await screen.getByRole('button', { name: 'Add to board' }).click();
    const dialog = screen.getByRole('dialog', { name: 'Add to board' });
    // The board holding it is not offered.
    await expect.element(dialog.getByRole('combobox', { name: 'Board' })).toHaveValue('4');
    const sprints = dialog.getByRole('combobox', { name: 'Sprint' });
    await expect.element(sprints).toBeVisible();
    expect(
      [...(sprints.element() as HTMLSelectElement).options].map((one) => one.textContent),
    ).toEqual(['The current sprint', 'Week 2', 'Week 1']);
    await sprints.selectOptions('Week 2');
    await dialog.getByRole('button', { name: 'Add' }).click();

    await expect.element(screen.getByRole('link', { name: 'Week 2' })).toBeVisible();
    const post = calls.find((call) => call.method === 'POST');
    expect(post?.body).toEqual({ board: 4, sprint: 22 });
    expect(calls.find((call) => call.path === '/boards')?.query.get('project')).toBe('1');
  });

  it('shows why an add was refused', async () => {
    server({
      'GET /boards': { body: { items: [summary(4, 'Ops')], next: null } },
      'GET /boards/4/sprints': { body: { items: [sprint(7, 'internal', { internal: true })] } },
      'POST /issues/501/boards': failure(403, 'forbidden'),
    });
    const screen = await render(IssueBoards, {
      issue: 501,
      project: 1,
      editable: true,
      boards: [],
    });

    await screen.getByRole('button', { name: 'Add to board' }).click();
    const dialog = screen.getByRole('dialog', { name: 'Add to board' });
    await expect.element(dialog.getByRole('combobox', { name: 'Board' })).toHaveValue('4');
    // A board without sprints asks for none.
    await expect.element(dialog.getByRole('combobox', { name: 'Sprint' })).not.toBeInTheDocument();
    await dialog.getByRole('button', { name: 'Add' }).click();
    await expect.element(dialog.getByText('You do not have permission to do this.')).toBeVisible();
  });

  it('offers nothing to change to a reader', async () => {
    server({});
    const screen = await render(IssueBoards, {
      issue: 501,
      project: 1,
      editable: false,
      boards: [SCRUM],
    });
    await expect.element(screen.getByRole('link', { name: 'Team' })).toBeVisible();
    await expect.element(screen.getByRole('button')).not.toBeInTheDocument();
  });
});

describe('ISS-007: the boards of a draft', () => {
  it('loads them, marks the automatic ones and keeps the draft off one', async () => {
    const auto = row(5, 'Kanban', { auto: true });
    const calls = server({
      'GET /issues/501/boards': { body: { items: [auto] } },
      'DELETE /issues/501/boards/5': { body: { items: [{ ...auto, skipped: true }] } },
      'POST /issues/501/boards': { body: { items: [auto] } },
    });
    const screen = await render(IssueBoards, { issue: 501, project: 1, editable: true });

    await expect.element(screen.getByText('automatically')).toBeVisible();
    await screen.getByRole('button', { name: 'Remove from “Kanban”' }).click();
    await expect.element(screen.getByText('will not join')).toBeVisible();
    await expectAccessible(screen.container);

    await screen.getByRole('button', { name: 'Put back on “Kanban”' }).click();
    await expect.element(screen.getByText('automatically')).toBeVisible();
    expect(calls.find((call) => call.method === 'POST')?.body).toEqual({ board: 5 });
  });

  it('loads them again for another project', async () => {
    const calls = server({ 'GET /issues/501/boards': { body: { items: [] } } });
    const screen = await render(IssueBoards, { issue: 501, project: 1, editable: true });
    await expect.element(screen.getByText('Not on boards')).toBeVisible();
    await screen.rerender({ issue: 501, project: 2, editable: true });
    await vi.waitFor(() => {
      expect(calls.filter((call) => call.path === '/issues/501/boards')).toHaveLength(2);
    });
  });
});
