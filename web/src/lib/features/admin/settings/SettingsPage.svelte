<script lang="ts">
  // The «Настройки» screen of the administration area (ADM-002, ADM-003, ADM-004, ADM-015,
  // ADM-016, ADM-017, AUTH-004, UI-004, INT-056; plan step 1.11 §4, the API origins in 1.12
  // §7.2): three sections, each with its own save. «Сервер» carries the address of the instance
  // (with its warnings and the checklist a change brings), the origins other sites may call the
  // API from, the upload limit under the deployment's ceiling, the session lifetimes and the
  // home page; «Локализация» the system language, the default time zone with a search, and the
  // date format previewed on one sample; «О системе» says the version. The mail settings have
  // their own screen.
  import { invalidate } from '$app/navigation';
  import { onMount } from 'svelte';

  import { BOOTSTRAP_DEPENDENCY, knownBootstrap } from '#lib/api/system.ts';
  import type { AdminSettings, DateFormat, HomePage, Language } from '#lib/api/schema.gen.ts';
  import { can } from '#lib/session/index.ts';
  import Button from '#lib/ui/Button.svelte';
  import ErrorState from '#lib/ui/ErrorState.svelte';
  import Notice from '#lib/ui/Notice.svelte';
  import PageHeader from '#lib/ui/PageHeader.svelte';
  import RadioGroup from '#lib/ui/RadioGroup.svelte';
  import Select, { type SelectOption } from '#lib/ui/Select.svelte';
  import TextArea from '#lib/ui/TextArea.svelte';
  import TextField from '#lib/ui/TextField.svelte';
  import { showError, toasts } from '#lib/ui/toast.svelte.ts';
  import { getTimeContext } from '#lib/time/context.svelte.ts';
  import { formatInstant } from '#lib/time/format.ts';
  import { formatOffset, timeZoneNames, wallTimeToInstant, zoneOffset } from '#lib/time/zone.ts';

  import * as m from './messages.ts';
  import { getAdminSettings, saveAdminSettings, settingValue } from './settings.ts';

  interface Props {
    /** The settings as loaded; `null` loads them here, `undefined` when the viewer may not see them. */
    settings: AdminSettings | null | undefined;
  }

  let { settings }: Props = $props();

  type Tab = 'server' | 'localization' | 'about';
  const TABS = [
    { key: 'server', label: m.serverTab },
    { key: 'localization', label: m.localizationTab },
    { key: 'about', label: m.aboutTab },
  ] as const satisfies readonly { key: Tab; label: () => string }[];

  const MIB = 1024 * 1024;
  const GIB = 1024 * 1024 * 1024;

  let tab = $state<Tab>('server');
  let failure = $state<unknown>(null);
  let busy = $state(false);
  const tabButtons: HTMLButtonElement[] = [];

  // — «Сервер» —
  let baseUrl = $state('');
  /** Whether the stored address has just changed: the checklist of what follows rides with it. */
  let baseUrlChanged = $state(false);
  let apiOrigins = $state('');
  let uploadValue = $state('');
  let uploadUnit = $state('mb');
  let sessionIdle = $state('');
  let rememberDays = $state('');
  let defaultHome = $state('issues');

  // — «Локализация» —
  let language = $state('en');
  let zone = $state('UTC');
  let zoneQuery = $state('');
  let dateFormat = $state('locale');

  const mayWrite = $derived(can('system.write'));

  $effect(() => {
    if (!settings) return;
    baseUrl = settingValue<string | null>(settings, 'base_url', null) ?? '';
    apiOrigins = settingValue<string[]>(settings, 'api.allowed_origins', []).join('\n');
    const bytes = settingValue<number>(settings, 'upload_max_bytes', 10 * MIB);
    // A local pick of the unit: reading the state here would tie this effect to itself, and a
    // change of the unit by hand would reset every field of the tab.
    const unit = bytes >= GIB && bytes % GIB === 0 ? 'gb' : 'mb';
    uploadUnit = unit;
    uploadValue = String(Math.round(bytes / (unit === 'gb' ? GIB : MIB)));
    sessionIdle = String(settingValue<number>(settings, 'session_idle_minutes', 60));
    rememberDays = String(settingValue<number>(settings, 'remember_me_days', 30));
    defaultHome = settingValue<HomePage>(settings, 'default_home', 'issues');
    language = settingValue<Language>(settings, 'system_language', 'en');
    zone = settingValue<string>(settings, 'default_time_zone', 'UTC');
    dateFormat = settingValue<DateFormat>(settings, 'default_date_format', 'locale');
  });

  /** A zone as the pickers take it: the IANA name and the label with its offset. */
  interface ZoneOption {
    value: string;
    label: string;
  }

  // Zones with their offset today, as the setup wizard shows them; built once, it takes a moment.
  const zones: ZoneOption[] = (() => {
    const now = Date.now();
    return timeZoneNames().map((name) => ({
      value: name,
      label: `${name.replaceAll('_', ' ')} (${formatOffset(zoneOffset(now, name))})`,
    }));
  })();

  /** The zones matching the search; the chosen one stays even when the search would hide it. */
  function zonesMatching(query: string): ZoneOption[] {
    const text = query.trim().toLowerCase();
    const found =
      text === '' ? zones : zones.filter((option) => option.label.toLowerCase().includes(text));
    return found.some((option) => option.value === zone)
      ? found
      : [...found, { value: zone, label: zone.replaceAll('_', ' ') }];
  }

  const zoneOptions = $derived(zonesMatching(zoneQuery));

  const languages: SelectOption[] = [
    { value: 'en', label: 'English', lang: 'en' },
    { value: 'ru', label: 'Русский', lang: 'ru' },
  ];

  const homeOptions = $derived([
    { value: 'issues', label: m.homeIssues() },
    { value: 'projects', label: m.homeProjects() },
    { value: 'boards', label: m.homeBoards() },
  ]);

  const units = $derived([
    { value: 'mb', label: m.megabyte() },
    { value: 'gb', label: m.gigabyte() },
  ]);

  // The four date formats of §1, each previewed on the last minute of 2000. The «по языку» hint
  // shows the language's own spelling, so the instance's stored pattern is left out of its context.
  const DMY_DOT_SAMPLE = '31.12.2000, 23:59';
  const ISO_SAMPLE = '2000-12-31 23:59';
  const formatOptions = $derived.by(() => {
    const context = getTimeContext();
    const languageDriven = { locale: context.locale, timeZone: context.timeZone };
    const sample = wallTimeToInstant(
      { year: 2000, month: 12, day: 31 },
      { hour: 23, minute: 59 },
      languageDriven.timeZone,
    );
    return [
      {
        value: 'locale',
        label: m.formatLocale(),
        hint: formatInstant(sample, languageDriven, { year: 'always' }),
      },
      { value: 'dmy_dot', label: m.formatDmyDot(), hint: DMY_DOT_SAMPLE },
      { value: 'dmy_name', label: m.formatDmyName(), hint: m.formatDmyNameSample() },
      { value: 'iso', label: m.formatIso(), hint: ISO_SAMPLE },
    ];
  });

  // — Validation, each in its field's own words; the server checks again. —

  const baseUrlInvalid = $derived.by(() => {
    const value = baseUrl.trim();
    if (value === '') return false;
    try {
      return !['http:', 'https:'].includes(new URL(value).protocol);
    } catch {
      return true;
    }
  });

  const httpWarning = $derived(baseUrl.trim().toLowerCase().startsWith('http://'));

  // The typed origins (INT-056): non-empty lines, trimmed; the server normalizes and caps them.
  const originLines = $derived(
    apiOrigins
      .split('\n')
      .map((line) => line.trim())
      .filter((line) => line !== ''),
  );

  const originsInvalid = $derived.by(() =>
    originLines.some((line) => {
      try {
        const url = new URL(line);
        return (
          !['http:', 'https:'].includes(url.protocol) ||
          url.username !== '' ||
          url.password !== '' ||
          (url.pathname !== '' && url.pathname !== '/') ||
          url.search !== '' ||
          url.hash !== ''
        );
      } catch {
        return true;
      }
    }),
  );

  const originWarning = $derived.by(() => {
    const stored = settings ? (settingValue<string | null>(settings, 'base_url', null) ?? '') : '';
    return stored !== '' && typeof location !== 'undefined' && location.origin !== stored;
  });

  // A number field's bound value arrives as a number, a cleared one as a string.
  function asText(value: string | number): string {
    return typeof value === 'number' ? String(value) : value;
  }

  const uploadBytes = $derived.by(() => {
    const typed = asText(uploadValue).trim();
    if (typed === '') return null;
    const number = Number(typed);
    if (!Number.isInteger(number) || number <= 0) return null;
    return number * (uploadUnit === 'gb' ? GIB : MIB);
  });

  const ceiling = $derived(settings?.upload_ceiling ?? 0);

  /** A byte count as a person reads it on this screen: whole gigabytes, else megabytes. */
  function bytesText(bytes: number): string {
    const gigs = bytes / GIB;
    if (Number.isInteger(gigs)) return `${String(gigs)} ${m.gigabyte()}`;
    return `${String(Math.round(bytes / MIB))} ${m.megabyte()}`;
  }

  const uploadError = $derived.by(() => {
    if (uploadBytes === null) return m.wholeNumber();
    if (ceiling > 0 && uploadBytes > ceiling)
      return m.uploadTooBig({ ceiling: bytesText(ceiling) });
    return null;
  });

  function bounded(text: string | number, min: number, max: number): string | null {
    const typed = asText(text).trim();
    if (typed === '') return m.wholeNumber();
    const number = Number(typed);
    if (!Number.isInteger(number) || number < min || number > max) {
      return m.outOfBounds({ min: String(min), max: String(max) });
    }
    return null;
  }

  const idleError = $derived(bounded(sessionIdle, 10, 720));
  const daysError = $derived(bounded(rememberDays, 1, 365));
  const serverInvalid = $derived(
    baseUrlInvalid ||
      originsInvalid ||
      uploadError !== null ||
      idleError !== null ||
      daysError !== null,
  );

  onMount(() => void load());

  async function load(): Promise<void> {
    if (settings === undefined) return;
    failure = null;
    try {
      settings = await getAdminSettings();
      baseUrlChanged = false;
    } catch (error) {
      failure = error;
    }
  }

  async function save(change: Record<string, unknown>): Promise<void> {
    if (!settings || busy) return;
    busy = true;
    try {
      const before = settingValue<string | null>(settings, 'base_url', null);
      settings = await saveAdminSettings(change);
      const after = settingValue<string | null>(settings, 'base_url', null) ?? '';
      baseUrlChanged = change.base_url !== undefined && after !== (before ?? '');
      toasts.show({ tone: 'success', message: m.savedToast() });
      // What the whole app reads from the bootstrap follows: the address, the home page, the limit.
      void invalidate(BOOTSTRAP_DEPENDENCY);
    } catch (error) {
      showError(error);
    } finally {
      busy = false;
    }
  }

  function onkeydown(event: KeyboardEvent, index: number): void {
    const step = { ArrowRight: 1, ArrowLeft: -1, Home: -index, End: TABS.length - 1 - index }[
      event.key
    ];
    if (step === undefined) return;
    event.preventDefault();
    const next = (index + step + TABS.length) % TABS.length;
    const found = TABS[next];
    if (!found) return;
    tab = found.key;
    tabButtons[next]?.focus();
  }
</script>

<svelte:head><title>{m.settingsTitle()} · trackflow</title></svelte:head>

<div class="settings-page">
  <PageHeader title={m.settingsTitle()} />
  <p class="intro">{m.settingsIntro()}</p>
  {#if failure !== null}
    <ErrorState error={failure} level={3} onretry={() => void load()} />
  {:else if settings === null || settings === undefined}
    <p class="loading">{m.settingsTitle()}</p>
  {:else}
    {#if !mayWrite}
      <Notice tone="info">{m.readOnlyNote()}</Notice>
    {/if}

    <div class="tabs" role="tablist" aria-label={m.tabsLabel()}>
      {#each TABS as one, index (one.key)}
        <button
          bind:this={tabButtons[index]}
          type="button"
          role="tab"
          id="settings-{one.key}-tab"
          aria-controls="settings-{one.key}"
          aria-selected={tab === one.key}
          tabindex={tab === one.key ? 0 : -1}
          onclick={() => {
            tab = one.key;
          }}
          onkeydown={(event) => onkeydown(event, index)}>{one.label()}</button
        >
      {/each}
    </div>

    {#if tab === 'server'}
      <div class="tab" role="tabpanel" id="settings-server" aria-labelledby="settings-server-tab">
        <form
          onsubmit={(event) => {
            event.preventDefault();
            if (!serverInvalid && uploadBytes !== null) {
              void save({
                base_url: baseUrl.trim() === '' ? null : baseUrl.trim(),
                'api.allowed_origins': originLines,
                upload_max_bytes: uploadBytes,
                session_idle_minutes: Number(sessionIdle),
                remember_me_days: Number(rememberDays),
                default_home: defaultHome,
              });
            }
          }}
        >
          <fieldset class="block" disabled={!mayWrite || busy}>
            <TextField
              label={m.baseUrlLabel()}
              hint={m.baseUrlHint()}
              error={baseUrlInvalid ? m.baseUrlInvalid() : null}
              type="url"
              inputmode="url"
              autocomplete="off"
              spellcheck="false"
              oninput={() => (baseUrlChanged = false)}
              bind:value={baseUrl}
            />
            {#if httpWarning}<Notice tone="warning">{m.baseUrlHttpWarning()}</Notice>{/if}
            {#if originWarning}
              <Notice tone="warning">{m.baseUrlOriginWarning({ origin: location.origin })}</Notice>
            {/if}
            {#if baseUrlChanged}
              <Notice tone="info">
                <p class="checklist-title">{m.baseUrlChecklistTitle()}</p>
                <ul class="checklist">
                  <li>{m.baseUrlChecklistLetters()}</li>
                  <li>{m.baseUrlChecklistOld()}</li>
                </ul>
              </Notice>
            {/if}
          </fieldset>

          <fieldset class="block" disabled={!mayWrite || busy}>
            <TextArea
              label={m.apiOriginsLabel()}
              hint={m.apiOriginsHint()}
              error={originsInvalid ? m.apiOriginsInvalid() : null}
              rows={4}
              spellcheck="false"
              autocomplete="off"
              bind:value={apiOrigins}
            />
          </fieldset>

          <fieldset class="block" disabled={!mayWrite || busy}>
            <div class="upload-row">
              <TextField
                label={m.uploadLabel()}
                hint={ceiling > 0 ? m.uploadHint({ ceiling: bytesText(ceiling) }) : undefined}
                error={uploadError}
                type="number"
                inputmode="numeric"
                min="1"
                step="1"
                bind:value={uploadValue}
              />
              <Select label={m.uploadUnitLabel()} options={units} bind:value={uploadUnit} />
            </div>
          </fieldset>

          <fieldset class="block" disabled={!mayWrite || busy}>
            <TextField
              label={m.sessionIdleLabel()}
              hint={m.sessionIdleHint()}
              error={idleError}
              type="number"
              inputmode="numeric"
              min="10"
              max="720"
              step="1"
              bind:value={sessionIdle}
            />
            <TextField
              label={m.rememberMeLabel()}
              hint={m.rememberMeHint()}
              error={daysError}
              type="number"
              inputmode="numeric"
              min="1"
              max="365"
              step="1"
              bind:value={rememberDays}
            />
            <RadioGroup
              legend={m.homeLabel()}
              hint={m.homeHint()}
              options={homeOptions}
              name="instance-home"
              bind:value={
                () => defaultHome,
                (home: string) => {
                  defaultHome = home;
                }
              }
            />
          </fieldset>

          {#if mayWrite}
            <div class="actions">
              <Button type="submit" disabled={serverInvalid} loading={busy}>
                {m.saveSettings()}
              </Button>
            </div>
          {/if}
        </form>
      </div>
    {:else if tab === 'localization'}
      <div
        class="tab"
        role="tabpanel"
        id="settings-localization"
        aria-labelledby="settings-localization-tab"
      >
        <form
          onsubmit={(event) => {
            event.preventDefault();
            void save({
              system_language: language,
              default_time_zone: zone,
              default_date_format: dateFormat,
            });
          }}
        >
          <fieldset class="block" disabled={!mayWrite || busy}>
            <Select
              label={m.languageLabel()}
              hint={m.languageHint()}
              options={languages}
              bind:value={language}
            />
          </fieldset>

          <fieldset class="block" disabled={!mayWrite || busy}>
            <TextField
              label={m.zoneSearch()}
              type="search"
              autocomplete="off"
              spellcheck="false"
              bind:value={zoneQuery}
            />
            <Select
              label={m.zoneLabel()}
              hint={m.zoneHint()}
              options={zoneOptions}
              bind:value={zone}
            />
          </fieldset>

          <fieldset class="block" disabled={!mayWrite || busy}>
            <RadioGroup
              legend={m.formatLabel()}
              hint={m.formatHint()}
              options={formatOptions}
              name="instance-date-format"
              bind:value={
                () => dateFormat,
                (format: string) => {
                  dateFormat = format;
                }
              }
            />
          </fieldset>

          {#if mayWrite}
            <div class="actions">
              <Button type="submit" loading={busy}>{m.saveSettings()}</Button>
            </div>
          {/if}
        </form>
      </div>
    {:else}
      <div class="tab" role="tabpanel" id="settings-about" aria-labelledby="settings-about-tab">
        <dl class="about">
          <dt>{m.aboutVersion()}</dt>
          <dd>{knownBootstrap()?.version ?? '—'}</dd>
        </dl>
      </div>
    {/if}
  {/if}
</div>

<style>
  .settings-page {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }
  .intro {
    margin: 0;
    padding: 0 var(--space-6);
    color: var(--color-text-muted);
  }
  .loading {
    color: var(--color-text-muted);
  }
  .tabs {
    display: flex;
    gap: var(--space-1);
    border-bottom: 1px solid var(--color-border);
    overflow-x: auto;
  }
  [role='tab'] {
    min-height: var(--control-md);
    padding: 0 var(--space-3);
    border: 0;
    border-bottom: 2px solid transparent;
    background: none;
    color: var(--color-text-muted);
    font: inherit;
    font-weight: var(--weight-medium);
    white-space: nowrap;
  }
  [role='tab']:hover {
    color: var(--color-text);
  }
  [role='tab'][aria-selected='true'] {
    border-bottom-color: var(--color-brand);
    color: var(--color-text);
    font-weight: var(--weight-semibold);
  }
  .tab {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }
  .block {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    margin: 0;
    padding: 0;
    border: 0;
  }
  .upload-row {
    display: flex;
    gap: var(--space-2);
    align-items: flex-start;
    flex-wrap: wrap;
  }
  .actions {
    display: flex;
    gap: var(--space-2);
  }
  .checklist-title {
    margin: 0 0 var(--space-1);
    font-weight: var(--weight-medium);
  }
  .checklist {
    margin: 0;
    padding-inline-start: var(--space-5);
  }
  .about {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: var(--space-2) var(--space-4);
    margin: 0;
  }
  .about dt {
    color: var(--color-text-muted);
  }
  .about dd {
    margin: 0;
    font-family: var(--font-mono);
  }
</style>
