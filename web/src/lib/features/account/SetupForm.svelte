<script lang="ts">
  // The first-run setup wizard (ADM-047): the setup code from the server log, the administrator,
  // and the basics of the installation. One page with three parts, so a phone scrolls through it
  // and every problem shows at once next to its field.
  import { ApiError } from '#lib/api/client.ts';
  import type { Language } from '#lib/api/schema.gen.ts';
  import { getLocale } from '#lib/i18n/locale.svelte.ts';
  import { fieldErrorText, fieldErrorTexts } from '#lib/i18n/messages/errors.ts';
  import { runSetup } from '#lib/session/index.ts';
  import { browserTimeZone, formatOffset, timeZoneNames, zoneOffset } from '#lib/time/zone.ts';
  import { announce } from '#lib/ui/announce.svelte.ts';
  import Button from '#lib/ui/Button.svelte';
  import Notice from '#lib/ui/Notice.svelte';
  import Select, { type SelectOption } from '#lib/ui/Select.svelte';
  import TextField from '#lib/ui/TextField.svelte';
  import { describeError } from '#lib/ui/toast.svelte.ts';

  import * as m from './messages.ts';

  let { ondone }: { ondone: () => unknown } = $props();

  let code = $state('');
  let fullName = $state('');
  let username = $state('admin');
  let email = $state('');
  let password = $state('');
  let repeat = $state('');
  let language = $state<Language>(getLocale());
  let baseUrl = $state(typeof location === 'undefined' ? '' : location.origin);
  let timeZone = $state(browserTimeZone());

  let busy = $state(false);
  let problem = $state<string | null>(null);
  let errors = $state<Record<string, string>>({});
  let form: HTMLFormElement | undefined = $state();

  const languages: SelectOption[] = [
    { value: 'en', label: 'English', lang: 'en' },
    { value: 'ru', label: 'Русский', lang: 'ru' },
  ];
  // Names with their offset today; built once, it takes a few milliseconds.
  const zones: SelectOption[] = (() => {
    const now = Date.now();
    return timeZoneNames().map((zone) => ({
      value: zone,
      label: `${zone.replaceAll('_', ' ')} (${formatOffset(zoneOffset(now, zone))})`,
    }));
  })();

  const own = {
    code: { incorrect: m.setupCodeWrong },
    password: { too_short: m.passwordTooShort },
  };

  function check(): Record<string, string> {
    const found: Record<string, string> = {};
    const required: [string, string][] = [
      ['code', code],
      ['full_name', fullName],
      ['username', username],
      ['email', email],
      ['password', password],
    ];
    for (const [path, value] of required) {
      if (!value.trim()) found[path] = fieldErrorText.required();
    }
    if (!found.password && Array.from(password).length < 8) found.password = m.passwordTooShort();
    if (!found.password && password !== repeat) found.repeat = m.passwordsDiffer();
    return found;
  }

  function focusFirstError(): void {
    form?.querySelector<HTMLElement>('[aria-invalid="true"]')?.focus();
  }

  async function submit(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    problem = null;
    errors = check();
    if (Object.keys(errors).length > 0) {
      queueMicrotask(focusFirstError);
      return;
    }
    busy = true;
    try {
      await runSetup({
        code,
        full_name: fullName,
        username,
        email,
        password,
        system_language: language,
        base_url: baseUrl.trim() || null,
        time_zone: timeZone,
      });
      await ondone();
    } catch (error) {
      if (error instanceof ApiError && error.code === 'invalid_request') {
        errors = fieldErrorTexts(error.body?.fields ?? [], own);
        queueMicrotask(focusFirstError);
      } else if (error instanceof ApiError && error.code === 'rate_limited') {
        problem = m.tooManyAttempts();
      } else {
        problem = describeError(error).message;
      }
      if (problem) announce(problem, 'assertive');
    } finally {
      busy = false;
    }
  }
</script>

<form class="form" novalidate onsubmit={submit} bind:this={form}>
  <p class="intro">{m.setupIntro()}</p>
  {#if problem}<Notice tone="danger">{problem}</Notice>{/if}

  <div class="part">
    <TextField
      label={m.setupCode()}
      bind:value={code}
      hint={m.setupCodeHint()}
      error={errors.code}
      autocomplete="off"
      autocapitalize="characters"
      spellcheck="false"
      class="code"
      required
    />
  </div>

  <section class="part" aria-labelledby="setup-admin">
    <h2 id="setup-admin">{m.administrator()}</h2>
    <TextField
      label={m.fullName()}
      bind:value={fullName}
      error={errors.full_name}
      autocomplete="name"
      required
    />
    <TextField
      label={m.username()}
      bind:value={username}
      hint={m.usernameHint()}
      error={errors.username}
      autocomplete="username"
      autocapitalize="none"
      spellcheck="false"
      required
    />
    <TextField
      label={m.email()}
      type="email"
      bind:value={email}
      error={errors.email}
      autocomplete="email"
      required
    />
    <TextField
      label={m.password()}
      type="password"
      bind:value={password}
      hint={m.passwordHint()}
      error={errors.password}
      autocomplete="new-password"
      required
    />
    <TextField
      label={m.repeatPassword()}
      type="password"
      bind:value={repeat}
      error={errors.repeat}
      autocomplete="new-password"
      required
    />
  </section>

  <section class="part" aria-labelledby="setup-installation">
    <h2 id="setup-installation">{m.installation()}</h2>
    <Select
      label={m.systemLanguage()}
      options={languages}
      bind:value={language}
      hint={m.systemLanguageHint()}
      error={errors.system_language}
    />
    <TextField
      label={m.baseUrl()}
      type="url"
      bind:value={baseUrl}
      hint={m.baseUrlHint()}
      error={errors.base_url}
      autocomplete="off"
      spellcheck="false"
    />
    <Select
      label={m.timeZone()}
      options={zones}
      bind:value={timeZone}
      hint={m.timeZoneHint()}
      error={errors.time_zone}
    />
  </section>

  <Button type="submit" variant="primary" size="lg" loading={busy}>{m.finishSetup()}</Button>
</form>

<style>
  .form {
    display: flex;
    flex-direction: column;
    gap: var(--space-6);
  }

  .intro {
    color: var(--color-text-muted);
  }

  .part {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }

  h2 {
    font-size: var(--text-lg);
  }

  .form :global(.code input) {
    font-family: var(--font-mono);
    letter-spacing: 0.08em;
  }

  .form > :global(button[type='submit']) {
    width: 100%;
  }
</style>
