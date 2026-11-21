// A project's logo in its general settings (PRJ-007), on a desktop and on a phone.
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { userEvent } from 'vitest/browser';
import { render } from 'vitest-browser-svelte';

import type { AccessMap, Project, Session, Upload } from '#lib/api/schema.gen.ts';
import { setBootstrapForTests } from '#lib/api/system.ts';
import { BASE_LOCALE, setLocale } from '#lib/i18n/locale.svelte.ts';
import { setAccessForTests, setSessionForTests } from '#lib/session/index.ts';
import { store } from '#lib/store/index.ts';
import { toasts } from '#lib/ui/toast.svelte.ts';

import { expectAccessible, isPhone, smallTargets } from '../../../../test/a11y.ts';
import { failure, fakeApi } from '../../../../test/fake-api.ts';
import { xhrThroughFetch } from '../../../../test/fake-xhr.ts';
import { PICTURE } from '../../../../test/pictures.ts';
import { projectRow } from '../projects.ts';
import GeneralSettings from './GeneralSettings.svelte';
import LogoForm from './LogoForm.svelte';

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
  created_at: '2026-10-22T09:00:00Z',
  password_change_required: false,
};

const WEB: Project = {
  id: 1,
  name: 'Web',
  key: 'WEB',
  archived_at: null,
  created_at: '2026-03-14T09:00:00Z',
  default_audience: null,
  deletion: null,
  description: '',
  description_html: '',
  favorite: false,
  former_keys: [],
  kind: 'standard',
  logo_color: 2,
  logo_url: null,
  next_number: 1,
  owner: null,
  team: { id: 101, name: 'Web', builtin: null, team_of: 1 },
  team_size: 4,
  template: 'blank',
  version: 3,
};

const UPLOAD: Upload = {
  id: 41,
  name: 'logo.png',
  size: 79,
  content_type: 'image/png',
  kind: 'image',
  url: '/api/v1/uploads/41/logo.png',
  created_at: '2026-10-22T09:00:00Z',
};

const READER: AccessMap = {
  global: [],
  projects: [{ project_id: 1, permissions: ['project.basic', 'project.full'] }],
  capabilities: [],
};

/** The stored project row, as the settings page passes it. */
function project(extra: Partial<Project> = {}): Project {
  return store.entities.table('project').upsert({ ...WEB, ...extra }, 'full');
}

function fileInput(container: Element): HTMLInputElement {
  const input = container.querySelector('input[type="file"]');
  if (!(input instanceof HTMLInputElement)) throw new Error('No file input');
  return input;
}

const png = () => new File([new Uint8Array(79)], 'logo.png', { type: 'image/png' });

beforeEach(() => {
  setSessionForTests(SESSION);
  xhrThroughFetch();
});

afterEach(() => {
  vi.unstubAllGlobals();
  store.entities.table('project').clear();
  setSessionForTests(null);
  setAccessForTests(null);
  setBootstrapForTests(null);
  setLocale(BASE_LOCALE);
  toasts.clear();
});

describe('LogoForm', () => {
  it('PRJ-007: uploads a logo and shows it on the project', async () => {
    const calls = fakeApi({
      'POST /uploads': { status: 201, body: UPLOAD },
      'PUT /projects/1/logo': { body: { url: PICTURE } },
    });
    const screen = await render(LogoForm, { project: project(), editable: true });
    await expect.element(screen.getByRole('heading', { name: 'Logo' })).toBeVisible();
    expect(screen.container.textContent).toContain('WE');
    expect(screen.getByRole('button', { name: 'Restore the default' }).query()).toBeNull();
    await expect.element(screen.getByRole('button', { name: 'Upload logo' })).toBeVisible();

    await userEvent.upload(fileInput(screen.container), png());

    await vi.waitFor(() => {
      expect(screen.container.querySelector('img')?.getAttribute('src')).toBe(PICTURE);
    });
    expect(calls.find((call) => call.path === '/uploads')?.body).toBeInstanceOf(Blob);
    expect(calls.find((call) => call.method === 'PUT')?.body).toEqual({ upload: 41 });
    expect(projectRow(1)?.logo_url).toBe(PICTURE);
    expect(toasts.items.at(-1)?.message).toBe('Logo updated.');
    await expect.element(screen.getByRole('button', { name: 'Restore the default' })).toBeVisible();
    expect(await smallTargets(screen.container, isPhone() ? 44 : 32)).toEqual([]);
    await expectAccessible(screen.container);
  });

  it('PRJ-007: goes back to the generated logo, in Russian', async () => {
    setLocale('ru');
    const calls = fakeApi({ 'DELETE /projects/1/logo': { status: 204 } });
    const screen = await render(LogoForm, {
      project: project({ logo_url: PICTURE }),
      editable: true,
    });

    await screen.getByRole('button', { name: 'Вернуть стандартный' }).click();
    await vi.waitFor(() => {
      expect(screen.container.querySelector('img')).toBeNull();
    });
    expect(calls.map((call) => `${call.method} ${call.path}`)).toEqual(['DELETE /projects/1/logo']);
    expect(projectRow(1)?.logo_url).toBeNull();
    expect(screen.container.textContent).toContain('WE');
    expect(toasts.items.at(-1)?.message).toBe('Возвращён стандартный логотип.');
    await expect.element(screen.getByRole('button', { name: 'Загрузить логотип' })).toBeVisible();
  });

  it('PRJ-007: problems are said under the buttons', async () => {
    fakeApi({
      'POST /uploads': { status: 201, body: UPLOAD },
      'PUT /projects/1/logo': failure(403, 'forbidden'),
    });
    const screen = await render(LogoForm, { project: project(), editable: true });
    await userEvent.upload(fileInput(screen.container), png());
    await expect.element(screen.getByText('You do not have permission to do this.')).toBeVisible();
    expect(projectRow(1)?.logo_url).toBeNull();
  });

  it('PRJ-007: people who may not change the project only see the logo', async () => {
    setAccessForTests(READER);
    fakeApi({ 'GET /users/me/access': { body: READER } });
    const screen = await render(GeneralSettings, {
      project: project({ logo_url: PICTURE }),
      ondeleted: vi.fn(),
    });
    await expect.element(screen.getByRole('heading', { name: 'Logo' })).toBeVisible();
    await vi.waitFor(() => {
      expect(screen.container.querySelector('img')?.getAttribute('src')).toBe(PICTURE);
    });
    expect(screen.getByRole('button', { name: 'Upload logo' }).query()).toBeNull();
    expect(screen.getByRole('button', { name: 'Restore the default' }).query()).toBeNull();
    expect(screen.container.querySelector('input[type="file"]')).toBeNull();
  });
});
