// One's own profile, groups and roles, and the pages of emailed links, on a desktop and on a
// phone (step 1.1; ACC-004, ACC-017, ACC-018, AUTH-008, AUTH-010, AUTH-014).
import { afterEach, describe, expect, it, vi } from 'vitest';
import { page } from 'vitest/browser';
import { render } from 'vitest-browser-svelte';

import type { Account, LinkInfo, Project, Session } from '#lib/api/schema.gen.ts';
import {
  getSession,
  preferences,
  setPreferencesForTests,
  setSessionForTests,
} from '#lib/session/index.ts';
import { homeHref } from '#lib/shell/sections.ts';

import { expectAccessible, isPhone, smallTargets } from '../../../test/a11y.ts';
import { failure, fakeApi } from '../../../test/fake-api.ts';
import ForcedPasswordForm from './ForcedPasswordForm.svelte';
import ForgotPasswordForm from './ForgotPasswordForm.svelte';
import InterfaceSettings from './InterfaceSettings.svelte';
import InvitationForm from './InvitationForm.svelte';
import MyAccess from './MyAccess.svelte';
import ProfileForm from './ProfileForm.svelte';
import ResetPasswordForm from './ResetPasswordForm.svelte';
import VerifyEmail from './VerifyEmail.svelte';

const ACCOUNT: Account = {
  id: 1,
  username: 'anna',
  full_name: 'Anna Admin',
  avatar_color: 3,
  avatar_url: null,
  email: 'anna@example.com',
  email_verified: false,
  guest: false,
  invited: false,
  disabled: null,
  groups: [{ id: 9, name: 'Developers', builtin: null, team_of: null }],
  projects: [],
  full_profile: true,
  version: 4,
  created_at: '2026-10-01T09:00:00Z',
  tier: 'full',
  password: { has_password: true, must_change: false },
};

const SESSION: Session = {
  user: {
    id: 1,
    username: 'anna',
    full_name: 'Anna Admin',
    email: 'anna@example.com',
    email_verified: false,
    avatar_color: 3,
    avatar_url: null,
    version: 4,
  },
  remember: false,
  created_at: '2026-10-22T09:00:00Z',
  password_change_required: true,
};

function label(text: string): RegExp {
  return new RegExp(`^${text.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}\\s*\\*?$`);
}

function link(purpose: LinkInfo['purpose'], expired = false): LinkInfo {
  return { purpose, email: 'new@example.com', username: 'new', full_name: 'new', expired };
}

afterEach(() => {
  vi.unstubAllGlobals();
  setSessionForTests(null);
  history.replaceState(null, '', location.pathname);
});

describe('ProfileForm', () => {
  it('ACC-017, AUTH-014: renames, and a new email needs the current password', async () => {
    setSessionForTests(SESSION);
    const calls = fakeApi({
      'PATCH /users/1': ({ body }) => ({
        body: { ...ACCOUNT, ...(body as object), email: 'anna@new.example', version: 5 },
      }),
      'POST /users/1/email-verification': { status: 204 },
    });
    const screen = await render(ProfileForm, { account: ACCOUNT, emailEnabled: true });
    await expect.element(screen.getByLabelText(label('Full name'))).toHaveValue('Anna Admin');
    await expect.element(screen.getByText('Not confirmed', { exact: false })).toBeVisible();

    await screen.getByLabelText(label('Email')).fill('anna@new.example');
    await screen.getByRole('button', { name: 'Save profile' }).click();
    await expect.element(screen.getByText('Fill in this field.')).toBeVisible();
    expect(calls.filter((call) => call.method === 'PATCH')).toEqual([]);

    await screen.getByLabelText(label('Current password')).fill('correct horse');
    await screen.getByRole('button', { name: 'Save profile' }).click();
    await vi.waitFor(() => {
      expect(calls.find((call) => call.method === 'PATCH')?.body).toEqual({
        full_name: null,
        username: null,
        email: 'anna@new.example',
        current_password: 'correct horse',
        version: 4,
      });
    });
    expect(getSession()?.user.email).toBe('anna@new.example');

    await screen.getByRole('button', { name: 'Send confirmation link' }).click();
    await vi.waitFor(() => {
      expect(calls.some((call) => call.path === '/users/1/email-verification')).toBe(true);
    });
    expect(smallTargets(screen.container, isPhone() ? 44 : 28)).toEqual([]);
    await expectAccessible(screen.container);
  });

  it('ACC-017: a taken username is reported on its field; no email means no links', async () => {
    fakeApi({
      'PATCH /users/1': failure(400, 'invalid_request', [
        { path: 'username', code: 'taken', message: 'x' },
      ]),
    });
    const screen = await render(ProfileForm, { account: ACCOUNT, emailEnabled: false });
    await screen.getByLabelText(label('Username')).fill('bob');
    await screen.getByRole('button', { name: 'Save profile' }).click();
    await expect.element(screen.getByText('Someone already has this username.')).toBeVisible();
    await expect.element(screen.getByText('Email is not set up', { exact: false })).toBeVisible();
    await expect
      .element(screen.getByRole('button', { name: 'Send confirmation link' }))
      .not.toBeInTheDocument();
  });
});

describe('MyAccess', () => {
  it('ACC-018, ACC-061: shows groups with the built-in ones and roles with their sources', async () => {
    fakeApi({
      'GET /users/1/roles': {
        body: {
          items: [
            {
              role: { id: 4, name: 'Directory viewer', builtin: 'directory_viewer' },
              project_id: null,
              permissions: ['user.directory', 'user.details', 'user.self'],
              sources: [
                {
                  kind: 'group',
                  group: { id: 2, name: 'All members', builtin: 'all_members', team_of: null },
                },
              ],
            },
            {
              role: { id: 7, name: 'Reviewer', builtin: null },
              project_id: null,
              permissions: ['comment.read'],
              sources: [{ kind: 'direct', assignment_id: 11 }],
            },
          ],
        },
      },
    });
    const screen = await render(MyAccess, { account: ACCOUNT });
    for (const group of ['Everyone', 'All members', 'Developers']) {
      await expect.element(screen.getByRole('listitem').filter({ hasText: group })).toBeVisible();
    }
    await expect
      .element(screen.getByText('Global · through the group “All members”'))
      .toBeVisible();
    await expect.element(screen.getByText('Global · assigned directly')).toBeVisible();
    await screen.getByRole('button', { name: 'Permissions of “Reviewer”' }).click();
    const dialog = page.getByRole('dialog', { name: 'Permissions of “Reviewer”' });
    await expect.element(dialog.getByText('Read comments')).toBeVisible();
    await expectAccessible(screen.container);
  });

  it('ACC-061, ACC-033: a role through a team says so and links to the project; projects are listed', async () => {
    const web: Project = {
      id: 5,
      name: 'Web',
      key: 'WEB',
      kind: 'standard',
      template: 'blank',
      description: '',
      description_html: '',
      archived_at: null,
      created_at: '2026-10-01T09:00:00Z',
      default_audience: null,
      deletion: null,
      favorite: false,
      former_keys: [],
      logo_color: 2,
      logo_url: null,
      next_number: 1,
      owner: null,
      team: null,
      team_size: 3,
      version: 1,
    };
    fakeApi({
      'GET /projects/5': { body: web },
      'GET /users/1/roles': {
        body: {
          items: [
            {
              role: { id: 3, name: 'Project member', builtin: 'project_member' },
              project_id: 5,
              permissions: ['issue.read'],
              sources: [
                { kind: 'group', group: { id: 50, name: 'Web', builtin: null, team_of: 5 } },
                { kind: 'direct', assignment_id: 12 },
              ],
            },
          ],
        },
      },
    });
    const screen = await render(MyAccess, {
      account: {
        ...ACCOUNT,
        groups: [...ACCOUNT.groups, { id: 50, name: 'Web', builtin: null, team_of: 5 }],
        projects: [
          { id: 5, name: 'Web', key: 'WEB', logo_color: 2, logo_url: null, archived: false },
        ],
      },
    });

    const teams = screen.getByRole('region', { name: 'Project teams' });
    await expect
      .element(teams.getByRole('link', { name: 'Web' }))
      .toHaveAttribute('href', '/projects/WEB');
    // The team is not repeated among the groups.
    expect(screen.getByRole('region', { name: 'Groups' }).getByText('Web team').query()).toBeNull();
    const roles = screen.getByRole('region', { name: 'Roles' });
    await expect.element(roles.getByText(/through the team of/)).toBeVisible();
    await expect.element(roles.getByText(/assigned directly/)).toBeVisible();
    await vi.waitFor(() => {
      expect(roles.getByRole('link', { name: 'Web' }).elements()).toHaveLength(2);
    });
    if (isPhone()) expect(smallTargets(screen.container)).toEqual([]);
    await expectAccessible(screen.container);
  });
});

describe('ForgotPasswordForm', () => {
  it('AUTH-008: asks for a link and says the same whatever the address', async () => {
    const calls = fakeApi({ 'POST /password-reset': { status: 202 } });
    const screen = await render(ForgotPasswordForm, { emailEnabled: true });
    await screen.getByLabelText(label('Email')).fill('anna@example.com');
    await screen.getByRole('button', { name: 'Send link' }).click();
    await expect
      .element(screen.getByText('If an account has this confirmed address', { exact: false }))
      .toBeVisible();
    expect(calls[0]?.body).toEqual({ email: 'anna@example.com' });
    await expectAccessible(screen.container);
  });

  it('AUTH-008: without email the page says whom to ask', async () => {
    const screen = await render(ForgotPasswordForm, { emailEnabled: false });
    await expect
      .element(screen.getByText('Ask your administrator to set a new password', { exact: false }))
      .toBeVisible();
  });
});

describe('ResetPasswordForm', () => {
  it('AUTH-008: sets a new password from the link and removes the secret from the address', async () => {
    history.replaceState(null, '', `${location.pathname}#SECRET`);
    const calls = fakeApi({
      'POST /account-links/inspect': { body: { ...link('password_reset'), username: 'anna' } },
      'POST /password-reset/confirm': { status: 201, body: SESSION },
    });
    const ondone = vi.fn();
    const screen = await render(ResetPasswordForm, { ondone });
    await expect.element(screen.getByText('For the account anna', { exact: false })).toBeVisible();
    expect(location.hash).toBe('');
    await screen.getByLabelText(label('New password')).fill('brand new pass');
    await screen.getByLabelText(label('Repeat the new password')).fill('brand new pass');
    await screen.getByRole('button', { name: 'Set password' }).click();
    await vi.waitFor(() => {
      expect(ondone).toHaveBeenCalledWith(SESSION);
    });
    expect(calls.at(-1)?.body).toEqual({ token: 'SECRET', new_password: 'brand new pass' });
    expect(smallTargets(screen.container, isPhone() ? 44 : 32)).toEqual([]);
    await expectAccessible(screen.container);
  });

  it('AUTH-008: an expired link offers a new one', async () => {
    history.replaceState(null, '', `${location.pathname}#OLD`);
    fakeApi({ 'POST /account-links/inspect': { body: link('password_reset', true) } });
    const screen = await render(ResetPasswordForm, { ondone: vi.fn() });
    await expect.element(screen.getByText('The link has expired', { exact: false })).toBeVisible();
    await expect.element(screen.getByRole('link', { name: 'Ask for a new link' })).toBeVisible();
  });

  it('AUTH-008: a used link says so', async () => {
    history.replaceState(null, '', `${location.pathname}#USED`);
    fakeApi({ 'POST /account-links/inspect': failure(404, 'not_found') });
    const screen = await render(ResetPasswordForm, { ondone: vi.fn() });
    await expect
      .element(screen.getByText('The link does not work', { exact: false }))
      .toBeVisible();
  });
});

describe('InvitationForm', () => {
  it('ACC-004: joins with a name, a username and a password of one’s own', async () => {
    history.replaceState(null, '', `${location.pathname}#INVITE`);
    const calls = fakeApi({
      'POST /account-links/inspect': { body: link('invitation') },
      'POST /invitations/accept': { status: 201, body: SESSION },
    });
    const ondone = vi.fn();
    const screen = await render(InvitationForm, { ondone });
    await expect.element(screen.getByLabelText(label('Username'))).toHaveValue('new');
    await screen.getByLabelText(label('Full name')).fill('New Person');
    await screen.getByLabelText(label('New password')).fill('my own password');
    await screen.getByLabelText(label('Repeat the new password')).fill('my own password');
    await screen.getByRole('button', { name: 'Join' }).click();
    await vi.waitFor(() => {
      expect(ondone).toHaveBeenCalledOnce();
    });
    expect(calls.at(-1)?.body).toEqual({
      token: 'INVITE',
      password: 'my own password',
      full_name: 'New Person',
      username: 'new',
    });
    await expectAccessible(screen.container);
  });
});

describe('VerifyEmail', () => {
  it('AUTH-014: confirms the address from the link', async () => {
    history.replaceState(null, '', `${location.pathname}#VERIFY`);
    const calls = fakeApi({
      'POST /email-verification/confirm': { body: { email: 'anna@example.com' } },
    });
    const screen = await render(VerifyEmail);
    await expect.element(screen.getByText('anna@example.com is confirmed.')).toBeVisible();
    expect(calls[0]?.body).toEqual({ token: 'VERIFY' });
    await expectAccessible(screen.container);
  });
});

describe('ForcedPasswordForm', () => {
  it('AUTH-010: a new password is required and must differ from the current one', async () => {
    setSessionForTests(SESSION);
    const calls = fakeApi({ 'POST /users/me/password': { status: 204 } });
    const ondone = vi.fn();
    const screen = await render(ForcedPasswordForm, { ondone });
    await screen.getByLabelText(label('Current password')).fill('one-time pass');
    await screen.getByLabelText(label('New password')).fill('one-time pass');
    await screen.getByLabelText(label('Repeat the new password')).fill('one-time pass');
    await screen.getByRole('button', { name: 'Set password' }).click();
    await expect
      .element(screen.getByText('Choose a password different from the current one.'))
      .toBeVisible();
    expect(calls).toEqual([]);

    await screen.getByLabelText(label('New password')).fill('my own password');
    await screen.getByLabelText(label('Repeat the new password')).fill('my own password');
    await screen.getByRole('button', { name: 'Set password' }).click();
    await vi.waitFor(() => {
      expect(ondone).toHaveBeenCalledOnce();
    });
    expect(getSession()?.password_change_required).toBe(false);
    expect(smallTargets(screen.container, isPhone() ? 44 : 32)).toEqual([]);
    await expectAccessible(screen.container);
  });
});

describe('UI-010: the «Интерфейс» section', () => {
  afterEach(() => {
    setPreferencesForTests(null);
  });

  it('UI-004: shows the person’s choices and keeps a new one at once', async () => {
    fakeApi({ 'PATCH /users/me/preferences': { body: {} } });
    setPreferencesForTests({ list_sidebar: 'closed', activity_order: 'newest_first' });
    const screen = await render(InterfaceSettings);
    const home = screen.getByRole('group', { name: 'Home page' });
    await expect.element(home.getByRole('radio', { name: 'Issues' })).toBeChecked();
    expect(homeHref()).toBe('/issues');
    const sidebar = screen.getByRole('group', { name: 'Sidebar of the issue list' });
    await expect.element(sidebar.getByRole('radio', { name: 'Hidden' })).toBeChecked();
    await expect.element(screen.getByRole('radio', { name: 'Newest first' })).toBeChecked();
    await expect.element(screen.getByRole('radio', { name: 'On the right' })).toBeChecked();
    await expect.element(screen.getByRole('radio', { name: 'Fold' })).toBeChecked();

    await home.getByRole('radio', { name: 'Projects' }).click();
    expect(preferences.value('home')).toBe('projects');
    expect(homeHref()).toBe('/projects');
    await home.getByRole('radio', { name: 'Boards' }).click();
    expect(preferences.value('home')).toBe('boards');
    expect(homeHref()).toBe('/boards');
    await screen.getByRole('radio', { name: 'On the left' }).click();
    expect(preferences.value('issue_panel_side')).toBe('start');
    await sidebar.getByRole('radio', { name: /pointing/ }).click();
    expect(preferences.value('list_sidebar')).toBe('auto');
    await expectAccessible(screen.container);
  });
});
