// Board helpers: addresses, the order of a list, the duplicate-name flag and the column list query
// (BRD-002, BRD-003, BRD-017, BRD-034), sprint schedules (BRD-083) and filter addresses (BRD-070).
import { describe, expect, it } from 'vitest';

import type { BoardSummary, UserRef } from '#lib/api/schema.gen.ts';

import { boardAddress, underBoards } from './board/header-filter.svelte.ts';
import { boardHref, columnQuery, nameTaken, orderBoards, sprintPhase } from './boards.ts';
import { newBoardHref } from './list/open.svelte.ts';
import { INTERNAL, OPEN, board, column } from './test-data.ts';

const ANNA: UserRef = {
  id: 1,
  full_name: 'Anna Admin',
  username: 'anna',
  avatar_color: 0,
  avatar_url: null,
  disabled: false,
  kind: 'person',
  pseudonym: null,
};

function summary(id: number, name: string, favorite = false): BoardSummary {
  return { id, name, favorite, owner: ANNA, projects: [] };
}

describe('BRD-017: board addresses', () => {
  it('names a board by its ID; the list and the new-board page have their own', () => {
    expect(boardHref.board(7)).toBe('/boards/7');
    expect(boardHref.entry()).toBe('/boards');
    expect(boardHref.list()).toBe('/boards?list');
    expect(boardHref.project('WEB')).toBe('/projects/WEB/boards');
    expect(newBoardHref()).toBe('/boards/new');
    expect(newBoardHref(3)).toBe('/boards/new?project=3');
  });
});

describe('BRD-002: the order of a list', () => {
  it('puts favorites first, then sorts by name ignoring letter case', () => {
    const ordered = orderBoards([
      summary(1, 'beta'),
      summary(2, 'Alpha'),
      summary(3, 'Zeta', true),
      summary(4, 'gamma', true),
    ]);
    expect(ordered.map((board) => board.id)).toEqual([4, 3, 2, 1]);
  });
});

describe('BRD-003: the duplicate-name flag', () => {
  it('matches a name the person sees, ignoring case and outer spaces', () => {
    const boards = [summary(1, 'Team board')];
    expect(nameTaken('  team BOARD ', boards)).toBe(true);
    expect(nameTaken('Team', boards)).toBe(false);
    expect(nameTaken('   ', boards)).toBe(false);
  });
});

describe('BRD-034: the list of a column keeps to the shown sprint', () => {
  const OPEN_COLUMN = column(1, OPEN);
  const sprint = { ...INTERNAL, id: 8, name: 'Sprint 2', internal: false };
  it('names the sprint of a sprint board', () => {
    expect(columnQuery(board({ sprints_on: true }), OPEN_COLUMN, sprint)).toBe(
      'project: WEB sprint: "Team/Sprint 2" State: Open',
    );
  });
  it('leaves it out without sprints', () => {
    expect(columnQuery(board(), OPEN_COLUMN, INTERNAL)).toBe('project: WEB State: Open');
  });
});

describe('BRD-083: where a sprint stands', () => {
  const today = { year: 2026, month: 10, day: 25 };
  const dated = (starts_on: string | null, ends_on: string | null, finished = false) => ({
    starts_on,
    ends_on,
    finished,
  });

  it('counts calendar days to the end, weekends included; the end day is the last', () => {
    expect(sprintPhase(dated('2026-10-12', '2026-11-01'), today)).toEqual({
      kind: 'running',
      days: 7,
    });
    expect(sprintPhase(dated('2026-10-12', '2026-10-25'), today)).toEqual({
      kind: 'running',
      days: 0,
    });
  });

  it('tells a sprint ahead, an open one, one without dates and a finished one', () => {
    expect(sprintPhase(dated('2026-10-27', '2026-11-08'), today)).toEqual({
      kind: 'ahead',
      days: 2,
    });
    expect(sprintPhase(dated('2026-10-01', null), today)).toEqual({ kind: 'open' });
    expect(sprintPhase(dated(null, null), today)).toEqual({ kind: 'none' });
    expect(sprintPhase(dated('2026-10-01', '2026-10-24'), today)).toEqual({ kind: 'finished' });
    expect(sprintPhase(dated('2026-10-12', '2026-11-01', true), today)).toEqual({
      kind: 'finished',
    });
  });
});

describe('BRD-017: sprint and filter addresses', () => {
  it('names a sprint under its board and keeps the filter in the query', () => {
    expect(boardHref.sprint(7, 3)).toBe('/boards/7/sprints/3');
    expect(boardAddress(7, null, '')).toBe('/boards/7');
    expect(boardAddress(7, 3, 'for: me')).toBe('/boards/7/sprints/3?q=for%3A%20me');
  });

  it('BRD-070: keeps the filter only under /boards', () => {
    expect(underBoards('/boards')).toBe(true);
    expect(underBoards('/boards/7/sprints/3')).toBe(true);
    expect(underBoards('/boardsx')).toBe(false);
    expect(underBoards('/issues')).toBe(false);
  });
});
