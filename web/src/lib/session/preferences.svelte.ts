// Personal interface settings that follow the person across devices (UI-018, plan 1.7 §3.2): the
// side and width of panels, collapsed sections, list columns, recent issues. A change applies at
// once, reaches the person's other tabs at once, and goes to the server 400 ms after the last one,
// merged into one request. Loaded with the access map and dropped with the session.
import { api as defaultApi, type ApiClient } from '#lib/api/client.ts';
import type { Preferences, PreferencesPatch, paths } from '#lib/api/schema.gen.ts';
import { knownBootstrap } from '#lib/api/system.ts';
import { readStored, writeStored } from '#lib/storage.ts';

type Values = { [K in keyof Preferences]-?: NonNullable<Preferences[K]> };
export type PreferenceKey = keyof Values;

/** What an unset key reads as; `last_project` has none. */
const DEFAULTS: Omit<Values, 'last_project'> = {
  rail_collapsed: false,
  editor_toolbar: true,
  home: 'issues',
  issue_panel_side: 'end',
  issue_panel_width: 220,
  issue_panel_collapsed: false,
  activity_order: 'oldest_first',
  activity_comments: true,
  activity_changes: true,
  long_comments: 'collapsed',
  attachments_collapsed: false,
  links_collapsed: false,
  list_hide_resolved: false,
  list_columns: [],
  list_sidebar: 'open',
  list_sidebar_width: 240,
  list_sidebar_folded: [],
  recent_issues: [],
};

/**
 * Settings this browser kept before they moved to the server: read once, when the server has none
 * of its own, then forgotten here.
 */
const LEGACY: {
  key: 'rail_collapsed' | 'editor_toolbar';
  storage: string;
  read: (s: string) => boolean;
}[] = [
  { key: 'rail_collapsed', storage: 'trackflow.nav.collapsed', read: (s) => s === 'true' },
  { key: 'editor_toolbar', storage: 'trackflow.editor.toolbar', read: (s) => s !== 'hidden' },
];

/** Quiet time after the last change before it is sent. */
const SEND_DELAY_MS = 400;
const CHANNEL = 'trackflow-preferences';

let stored = $state<Preferences>({});
let loaded: Promise<void> | null = null;
let client: ApiClient<paths> = defaultApi;
/** Changes not yet accepted by the server; `null` resets a key. */
let pending: PreferencesPatch = {};
let timer: ReturnType<typeof setTimeout> | null = null;
let channel: BroadcastChannel | null = null;

function openChannel(): void {
  if (channel || typeof BroadcastChannel === 'undefined') return;
  channel = new BroadcastChannel(CHANNEL);
  channel.onmessage = (event: MessageEvent<PreferencesPatch>) => {
    apply(event.data);
  };
}

function apply(patch: PreferencesPatch): void {
  const changed = new Set(Object.keys(patch));
  stored = Object.fromEntries([
    ...Object.entries(stored).filter(([key]) => !changed.has(key)),
    ...Object.entries(patch).filter(([, value]) => value !== null),
  ]);
}

function schedule(): void {
  if (timer !== null) clearTimeout(timer);
  timer = setTimeout(() => {
    timer = null;
    void send();
  }, SEND_DELAY_MS);
}

async function send(): Promise<void> {
  const body = pending;
  if (Object.keys(body).length === 0) return;
  pending = {};
  try {
    await client.patch('/users/me/preferences', { body });
  } catch (error) {
    // The values stay here; the next change sends them again, under any newer ones.
    pending = { ...body, ...pending };
    console.warn('Interface settings were not saved', error);
  }
}

export const preferences = {
  /**
   * The value of a key: the person's, or the default when unset or not loaded. Reactive. The home
   * page falls back to the instance's own default first (UI-004): what the administrator chose in
   * «Настройки» wins over the built-in «Задачи».
   */
  value<K extends Exclude<PreferenceKey, 'last_project'>>(key: K): Values[K] {
    const own = key === 'home' ? (stored.home ?? knownBootstrap()?.default_home) : stored[key];
    return (own ?? DEFAULTS[key]) as Values[K];
  },

  /** The person's own value of a key; `undefined` when they never set it. Reactive. */
  chosen<K extends PreferenceKey>(key: K): Values[K] | undefined {
    return (stored[key] ?? undefined) as Values[K] | undefined;
  },

  /** Sets keys (`null` resets one to its default): here at once, on the server soon after. */
  set(patch: PreferencesPatch): void {
    apply(patch);
    pending = { ...pending, ...patch };
    openChannel();
    channel?.postMessage(patch);
    schedule();
  },

  /** Loads the person's settings once per session. A failed load leaves the defaults. */
  ensure(fetcher?: typeof fetch): Promise<void> {
    loaded ??= client
      .get('/users/me/preferences', { fetch: fetcher })
      .then((answer) => {
        // Changes made before the answer came stay on top of it.
        const local = pending;
        stored = answer;
        apply(local);
        openChannel();
        adoptLegacy();
      })
      .catch((error: unknown) => {
        loaded = null;
        console.warn('Interface settings were not loaded', error);
      });
    return loaded;
  },
};

function adoptLegacy(): void {
  const patch: PreferencesPatch = {};
  for (const { key, storage, read } of LEGACY) {
    const old = readStored(storage);
    if (old === null) continue;
    if (stored[key] === undefined || stored[key] === null) patch[key] = read(old);
    writeStored(storage, null);
  }
  if (Object.keys(patch).length > 0) preferences.set(patch);
}

/** Forgets the settings: the session ended or another person signed in. Unsent changes go too. */
export function clearPreferences(): void {
  if (timer !== null) clearTimeout(timer);
  timer = null;
  pending = {};
  stored = {};
  loaded = null;
}

/** For tests: settings as if loaded (`null`: not loaded), and a client to use instead of the app's. */
export function setPreferencesForTests(values: Preferences | null, api?: ApiClient<paths>): void {
  clearPreferences();
  if (values) {
    stored = values;
    loaded = Promise.resolve();
  }
  if (api) client = api;
}
