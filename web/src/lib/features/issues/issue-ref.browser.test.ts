// Issues wherever they are named (ISS-024, ISS-042).
import { afterEach, describe, expect, it } from 'vitest';
import { render } from 'vitest-browser-svelte';

import { BASE_LOCALE, setLocale } from '#lib/i18n/locale.svelte.ts';

import { expectAccessible } from '../../../test/a11y.ts';
import IssueRef from './IssueRef.svelte';
import { BORIS, issue, WEB } from './test-data.ts';

afterEach(() => {
  setLocale(BASE_LOCALE);
});

describe('IssueRef', () => {
  it('ISS-024: resolved issues are struck and named', async () => {
    const screen = await render(IssueRef, {
      issue: issue({ resolved_at: '2026-10-23T10:00:00Z' }),
    });

    const link = screen.getByRole('link');
    await expect.element(link).toHaveAccessibleName('WEB-12 (resolved) Login button does nothing');
    expect(link.element().getAttribute('href')).toMatch(/\/issues\/WEB-12$/);
    expect(link.element().querySelector('s')?.textContent).toBe('WEB-12');
    await expectAccessible(screen.container);
  });

  it('ISS-024: open issues are plain; without a link or summary only the ID shows', async () => {
    const screen = await render(IssueRef, {
      issue: {
        id: 7,
        readable_id: 'WEB-7',
        summary: 'Footer',
        project: WEB,
        resolved: false,
        restricted: false,
      },
      link: false,
      summary: false,
    });

    expect(screen.getByRole('link').query()).toBeNull();
    expect(screen.container.textContent.trim()).toBe('WEB-7');
    expect(screen.container.querySelector('s')).toBeNull();
  });

  it('ISS-042: a restricted issue carries a lock that names its audience', async () => {
    const screen = await render(IssueRef, {
      issue: issue({ audience: { users: [BORIS], groups: [] } }),
    });

    await expect
      .element(screen.getByRole('img', { name: 'Visible to Boris Lead and the reporter' }))
      .toBeVisible();
  });

  it('ISS-042: a brief reference only knows that it is restricted', async () => {
    const screen = await render(IssueRef, {
      issue: {
        id: 7,
        readable_id: 'WEB-7',
        summary: 'Footer',
        project: WEB,
        resolved: false,
        restricted: true,
      },
    });

    await expect
      .element(screen.getByRole('img', { name: 'Visible to a limited audience' }))
      .toBeVisible();
  });
});
