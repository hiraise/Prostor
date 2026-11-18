// Personal interface settings on the server (UI-018): defaults, merged sends, resets, other tabs,
// and the values this browser kept before.
import { afterEach, describe, expect, it, vi } from 'vitest';

import { failure, fakeApi } from '../../test/fake-api.ts';
import { preferences, setPreferencesForTests } from './preferences.svelte.ts';

afterEach(() => {
  vi.useRealTimers();
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
  setPreferencesForTests(null);
  localStorage.removeItem('taskmaster.nav.collapsed');
  localStorage.removeItem('taskmaster.editor.toolbar');
});

describe('UI-018: personal interface settings', () => {
  it('UI-018: unset keys and keys before the load read as defaults', async () => {
    fakeApi({ 'GET /users/me/preferences': { body: { issue_panel_side: 'start' } } });
    expect(preferences.value('issue_panel_side')).toBe('end');
    expect(preferences.value('issue_panel_width')).toBe(180);
    expect(preferences.chosen('issue_panel_side')).toBeUndefined();

    await preferences.ensure();
    expect(preferences.value('issue_panel_side')).toBe('start');
    expect(preferences.value('issue_panel_width')).toBe(180);
    expect(preferences.value('recent_issues')).toEqual([]);
  });

  it('UI-018: changes apply at once and go out merged, 400 ms after the last one', async () => {
    vi.useFakeTimers();
    const calls = fakeApi({ 'PATCH /users/me/preferences': ({ body }) => ({ body }) });
    setPreferencesForTests({ issue_panel_collapsed: true });

    preferences.set({ issue_panel_width: 300 });
    expect(preferences.value('issue_panel_width')).toBe(300);
    await vi.advanceTimersByTimeAsync(300);
    preferences.set({ issue_panel_width: 320, issue_panel_side: 'start' });
    preferences.set({ issue_panel_collapsed: null });
    expect(preferences.value('issue_panel_collapsed')).toBe(false);
    await vi.advanceTimersByTimeAsync(399);
    expect(calls).toHaveLength(0);
    await vi.advanceTimersByTimeAsync(1);

    expect(calls).toHaveLength(1);
    expect(calls[0]?.body).toEqual({
      issue_panel_width: 320,
      issue_panel_side: 'start',
      issue_panel_collapsed: null,
    });
  });

  it('UI-018: a failed send keeps the value and goes again with the next change', async () => {
    vi.useFakeTimers();
    vi.spyOn(console, 'warn').mockImplementation(() => undefined);
    let fail = true;
    const calls = fakeApi({
      'PATCH /users/me/preferences': ({ body }) =>
        fail ? failure(503, 'unavailable') : { body: body as object },
    });
    setPreferencesForTests({});

    preferences.set({ list_hide_resolved: true });
    await vi.advanceTimersByTimeAsync(400);
    expect(preferences.value('list_hide_resolved')).toBe(true);
    fail = false;
    preferences.set({ home: 'projects' });
    await vi.advanceTimersByTimeAsync(400);

    expect(calls.map((call) => call.body)).toEqual([
      { list_hide_resolved: true },
      { list_hide_resolved: true, home: 'projects' },
    ]);
  });

  it('UI-018: a change in another tab shows here without a request', async () => {
    const calls = fakeApi({});
    setPreferencesForTests({});
    // Opens this tab's channel, as a load or a change does.
    preferences.set({ links_collapsed: true });
    const other = new BroadcastChannel('taskmaster-preferences');
    other.postMessage({ issue_panel_side: 'start', links_collapsed: null });
    other.close();

    await expect.poll(() => preferences.value('issue_panel_side')).toBe('start');
    expect(preferences.value('links_collapsed')).toBe(false);
    expect(calls).toHaveLength(0);
  });

  it('UI-018: what this browser kept moves to the server once, unless the server has its own', async () => {
    vi.useFakeTimers();
    localStorage.setItem('taskmaster.nav.collapsed', 'true');
    localStorage.setItem('taskmaster.editor.toolbar', 'hidden');
    const calls = fakeApi({
      'GET /users/me/preferences': { body: { editor_toolbar: true } },
      'PATCH /users/me/preferences': ({ body }) => ({ body }),
    });

    await preferences.ensure();
    await vi.advanceTimersByTimeAsync(400);

    expect(preferences.value('rail_collapsed')).toBe(true);
    expect(preferences.value('editor_toolbar')).toBe(true);
    expect(calls.at(-1)?.body).toEqual({ rail_collapsed: true });
    expect(localStorage.getItem('taskmaster.nav.collapsed')).toBeNull();
    expect(localStorage.getItem('taskmaster.editor.toolbar')).toBeNull();
  });
});
