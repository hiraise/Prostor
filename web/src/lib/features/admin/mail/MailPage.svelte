<script lang="ts">
  // The «Почта» screen of the administration area (NTF-002, NTF-004, NTF-006, NTF-007; plan step
  // 1.10 §5.1): the mail server with its secrets, the sender and the reply address, the instance
  // switch and a test send. Secret values never come back — the screen shows that one is stored,
  // an empty field keeps it, a typed one replaces it. After a save and a successful test it
  // offers to switch email on (NTF-002: off until configured and tested).
  import { onMount } from 'svelte';

  import { ApiError } from '#lib/api/client.ts';
  import type { MailSettings } from '#lib/api/schema.gen.ts';
  import { can } from '#lib/session/index.ts';
  import Button from '#lib/ui/Button.svelte';
  import Checkbox from '#lib/ui/Checkbox.svelte';
  import ErrorState from '#lib/ui/ErrorState.svelte';
  import Notice from '#lib/ui/Notice.svelte';
  import PageHeader from '#lib/ui/PageHeader.svelte';
  import TextField from '#lib/ui/TextField.svelte';
  import { showError, toasts } from '#lib/ui/toast.svelte.ts';

  import * as m from './messages.ts';
  import { getMailSettings, saveMailSettings, sendMailTest } from './mail.ts';

  interface Props {
    /** The settings as loaded; `null` loads them here, `undefined` when the viewer may not see them. */
    settings: MailSettings | null | undefined;
  }

  let { settings }: Props = $props();

  let failure = $state<unknown>(null);
  let busy = $state(false);
  let testing = $state(false);
  let tested = $state(false);

  // Secret fields are write-only: the input replaces when typed, the flag removes when set.
  let smtpUrl = $state('');
  let removeUrl = $state(false);
  let username = $state('');
  let removeUsername = $state(false);
  let password = $state('');
  let removePassword = $state(false);
  let mailFrom = $state('');
  let mailReplyTo = $state('');
  let emailEnabled = $state(false);
  let testTo = $state('');

  const mayWrite = $derived(can('system.write'));

  $effect(() => {
    if (settings) {
      mailFrom = settings.mail_from;
      mailReplyTo = settings.mail_reply_to;
      emailEnabled = settings.email_enabled;
      testTo = '';
    }
  });

  onMount(() => void load());

  async function load(): Promise<void> {
    if (settings === undefined) return;
    failure = null;
    try {
      settings = await getMailSettings();
    } catch (error) {
      failure = error;
    }
  }

  /** What a secret field sends: `undefined` keeps, `null` removes, the typed value stores. */
  function secret(input: string, remove: boolean): string | null | undefined {
    if (remove) return null;
    return input.trim() === '' ? undefined : input.trim();
  }

  async function save(enabled?: boolean): Promise<void> {
    if (!settings || busy) return;
    busy = true;
    try {
      settings = await saveMailSettings({
        smtp_url: secret(smtpUrl, removeUrl),
        smtp_username: secret(username, removeUsername),
        smtp_password: secret(password, removePassword),
        mail_from: mailFrom.trim(),
        mail_reply_to: mailReplyTo.trim(),
        // An untouched switch rides along as absent: a newly saved transport then starts the
        // switch off until tested and switched on (NTF-002), a touched one saves its new state.
        email_enabled:
          enabled ?? (emailEnabled === settings.email_enabled ? undefined : emailEnabled),
      });
      smtpUrl = username = password = '';
      removeUrl = removeUsername = removePassword = false;
      toasts.show({ tone: 'success', message: m.savedToast() });
    } catch (error) {
      showError(error);
    } finally {
      busy = false;
    }
  }

  async function test(): Promise<void> {
    if (!settings || testing) return;
    testing = true;
    try {
      await sendMailTest(testTo.trim());
      tested = true;
      toasts.show({ tone: 'success', message: m.testSent() });
    } catch (error) {
      tested = false;
      // The transport's complaint comes in full (NTF-007); show it as it is.
      if (error instanceof ApiError) toasts.show({ tone: 'error', message: error.message });
      else showError(error);
    } finally {
      testing = false;
    }
  }
</script>

<svelte:head><title>{m.mailTitle()} · TaskMaster</title></svelte:head>

<div class="mail-page">
  <PageHeader title={m.mailTitle()} />
  <p class="intro">{m.mailIntro()}</p>
  {#if failure !== null}
    <ErrorState error={failure} level={3} onretry={() => void load()} />
  {:else if settings === null || settings === undefined}
    <p class="loading">{m.mailTitle()}</p>
  {:else}
    {#if !mayWrite}
      <Notice tone="info">{m.readOnlyNote()}</Notice>
    {/if}
    {#if settings.source === 'env'}
      <Notice tone="info">{m.envNotice()}</Notice>
    {/if}
    {#if !settings.email_enabled}
      <Notice tone="warning">{m.offNotice()}</Notice>
    {/if}

    {#if settings.configured && !settings.email_enabled && tested}
      <Notice tone="success">
        {m.offerOn()}
        {#if mayWrite}
          <Button onclick={() => void save(true)}>{m.switchOn()}</Button>
        {/if}
      </Notice>
    {/if}

    <form
      onsubmit={(event) => {
        event.preventDefault();
        void save();
      }}
    >
      <fieldset class="block" aria-labelledby="mail-transport" disabled={!mayWrite || busy}>
        <legend id="mail-transport">{m.transportLegend()}</legend>
        <TextField
          label={m.smtpUrlLabel()}
          hint={m.smtpUrlHint()}
          placeholder={settings.has_smtp_url ? m.storedMark() : 'smtp://mail.example.com:587'}
          bind:value={smtpUrl}
        />
        {#if settings.has_smtp_url}
          <Checkbox label={m.clearSecret()} bind:checked={removeUrl} />
        {/if}
        <TextField
          label={m.usernameLabel()}
          hint={m.usernameHint()}
          placeholder={settings.has_username ? m.storedMark() : ''}
          bind:value={username}
        />
        {#if settings.has_username}
          <Checkbox label={m.clearSecret()} bind:checked={removeUsername} />
        {/if}
        <TextField
          label={m.passwordLabel()}
          hint={m.passwordHint()}
          type="password"
          placeholder={settings.has_password ? m.storedMark() : ''}
          bind:value={password}
        />
        {#if settings.has_password}
          <Checkbox label={m.clearSecret()} bind:checked={removePassword} />
        {/if}
      </fieldset>

      <fieldset class="block" aria-labelledby="mail-sender" disabled={!mayWrite || busy}>
        <legend id="mail-sender">{m.senderLegend()}</legend>
        <TextField
          label={m.fromLabel()}
          hint={m.fromHint()}
          placeholder="TaskMaster <tracker@example.com>"
          bind:value={mailFrom}
        />
        <TextField label={m.replyToLabel()} hint={m.replyToHint()} bind:value={mailReplyTo} />
      </fieldset>

      <fieldset class="block" aria-labelledby="mail-switch" disabled={!mayWrite || busy}>
        <legend id="mail-switch">{m.switchLegend()}</legend>
        <Checkbox
          label={m.emailEnabledLabel()}
          hint={m.emailEnabledHint()}
          bind:checked={emailEnabled}
        />
      </fieldset>

      {#if mayWrite}
        <div class="actions">
          <Button type="submit" loading={busy}>{m.saveSettings()}</Button>
        </div>
      {/if}
    </form>

    <fieldset
      class="block"
      aria-labelledby="mail-test"
      disabled={!mayWrite || !settings.configured || testing}
    >
      <legend id="mail-test">{m.testLegend()}</legend>
      <p class="stored">{m.testIntro()}</p>
      <div class="test-row">
        <TextField label={m.testToLabel()} placeholder="anyone@example.com" bind:value={testTo} />
        <Button loading={testing} onclick={() => void test()}>{m.testSend()}</Button>
      </div>
    </fieldset>
  {/if}
</div>

<style>
  .mail-page {
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
  .stored {
    margin: 0;
    font-size: 0.875rem;
    color: var(--color-text-muted);
  }
  .actions {
    display: flex;
    gap: var(--space-2);
  }
  .test-row {
    display: flex;
    gap: var(--space-2);
    align-items: flex-end;
    flex-wrap: wrap;
  }
  .loading {
    color: var(--color-text-muted);
  }
  .intro {
    margin: 0;
    padding: 0 var(--space-6);
    color: var(--color-text-muted);
  }
</style>
