<script lang="ts">
  // The settings half of the «Уведомления» section of the account page (NTF-017, NTF-030,
  // NTF-034, NTF-066, NTF-093, NTF-094; plan step 1.10 §4.2, auto-watch ISS-030 in 1.12 §7.1):
  // the email channel with its address and a test letter, the event switches, the kinds of the
  // built-in watch subscription, and what turns the star on by itself. Every choice applies at
  // once, like the interface preferences; a failed save returns the switch. The subscriptions on
  // tags and searches sit next to this, in their own list.
  import { onMount } from 'svelte';

  import type { NotificationSettings } from '#lib/api/schema.gen.ts';
  import Button from '#lib/ui/Button.svelte';
  import Checkbox from '#lib/ui/Checkbox.svelte';
  import ErrorState from '#lib/ui/ErrorState.svelte';
  import Notice from '#lib/ui/Notice.svelte';
  import RadioGroup from '#lib/ui/RadioGroup.svelte';
  import { showError, toasts } from '#lib/ui/toast.svelte.ts';

  import SubscriptionList from './SubscriptionList.svelte';
  import * as m from './messages.ts';
  import {
    AUTOWATCH_KEYS,
    WATCH_KINDS,
    getSettings,
    putSettings,
    sendTestEmail,
  } from './subscriptions.ts';

  interface Props {
    /** The address letters go to (NTF-094); `null` when the profile has none. */
    email: string | null;
    /** Whether the instance sends email at all; the notice replaces the test button when off. */
    emailEnabled: boolean;
  }

  let { email, emailEnabled }: Props = $props();

  let settings = $state<NotificationSettings | null>(null);
  let failure = $state<unknown>(null);
  let busy = $state(false);
  let testing = $state(false);

  async function load(): Promise<void> {
    failure = null;
    try {
      settings = await getSettings();
    } catch (error) {
      failure = error;
    }
  }

  onMount(() => void load());

  async function change(next: NotificationSettings): Promise<void> {
    if (!settings || busy) return;
    const before = settings;
    settings = next;
    busy = true;
    try {
      settings = await putSettings(next);
    } catch (error) {
      settings = before;
      showError(error);
    } finally {
      busy = false;
    }
  }

  async function test(): Promise<void> {
    testing = true;
    try {
      const sent = await sendTestEmail();
      toasts.show({ tone: 'success', message: m.testSent({ email: sent.email }) });
    } catch (error) {
      showError(error);
    } finally {
      testing = false;
    }
  }

  /** Every switch of the settings is a boolean flag. */
  type FlagKey = {
    [K in keyof NotificationSettings]: NotificationSettings[K] extends boolean ? K : never;
  }[keyof NotificationSettings];

  function flip(key: FlagKey, on: boolean): void {
    if (settings) void change({ ...settings, [key]: on });
  }
</script>

<div class="settings">
  {#if failure !== null}
    <ErrorState error={failure} level={3} onretry={() => void load()} />
  {:else if settings}
    <fieldset class="block" aria-labelledby="ntf-channels" disabled={busy}>
      <legend id="ntf-channels">{m.channelsLegend()}</legend>
      <Checkbox
        label={m.emailChannel()}
        bind:checked={() => settings?.email ?? false, (on: boolean) => flip('email', on)}
      />
      {#if settings.email}
        <RadioGroup
          legend={m.emailFormat()}
          options={[
            { value: 'html', label: m.formatHtml() },
            { value: 'plain', label: m.formatPlain() },
          ]}
          bind:value={
            () => (settings?.email_html ? 'html' : 'plain'),
            (format: string) => flip('email_html', format === 'html')
          }
        />
      {/if}
      <div class="address">
        {#if email}
          <p class="goes-to">{m.addressGoesTo({ email })}</p>
        {:else}
          <p class="goes-to">{m.noAddress()}</p>
        {/if}
        {#if !emailEnabled}
          <Notice tone="warning">{m.emailOffNotice()}</Notice>
        {:else if email}
          <Button loading={testing} onclick={() => void test()}>{m.testSend()}</Button>
        {/if}
      </div>
    </fieldset>

    <fieldset class="block" aria-labelledby="ntf-events" disabled={busy}>
      <legend id="ntf-events">{m.eventsLegend()}</legend>
      <Checkbox
        label={m.mentionsSetting()}
        hint={m.mentionsHint()}
        bind:checked={() => settings?.mentions ?? false, (on: boolean) => flip('mentions', on)}
      />
      <Checkbox
        label={m.ownChangesSetting()}
        hint={m.ownChangesHint()}
        bind:checked={
          () => settings?.own_changes ?? false, (on: boolean) => flip('own_changes', on)
        }
      />
    </fieldset>

    <fieldset class="block" aria-labelledby="ntf-watch" disabled={busy}>
      <legend id="ntf-watch">{m.watchLegend()}</legend>
      <p class="block-hint">{m.watchIntro()}</p>
      <div class="watch">
        {#each WATCH_KINDS as kind (kind)}
          <Checkbox
            label={m.watchLabels[kind]?.() ?? kind}
            bind:checked={() => settings?.[kind] ?? false, (on: boolean) => flip(kind, on)}
          />
        {/each}
      </div>
    </fieldset>

    <fieldset class="block" aria-labelledby="ntf-autowatch" disabled={busy}>
      <legend id="ntf-autowatch">{m.autowatchLegend()}</legend>
      <p class="block-hint">{m.autowatchIntro()}</p>
      <div class="watch">
        {#each AUTOWATCH_KEYS as key (key)}
          <Checkbox
            label={m.autowatchLabels[key]?.() ?? key}
            bind:checked={() => settings?.[key] ?? false, (on: boolean) => flip(key, on)}
          />
        {/each}
      </div>
    </fieldset>
  {/if}

  <SubscriptionList />
</div>

<style>
  .settings {
    display: flex;
    flex-direction: column;
    gap: var(--space-6);
  }

  .block {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    margin: 0;
    padding: 0;
    border: none;
  }

  .block:disabled {
    /* A saving PUT holds the inputs, not their look. */
    opacity: revert;
  }

  .block legend {
    padding: 0;
    color: var(--color-text-muted);
    font-size: var(--text-sm);
  }

  .block-hint {
    margin: 0;
    color: var(--color-text-muted);
  }

  .watch {
    display: grid;
    /* Rows of ticks wrap on a phone and spread out on wide screens (D-019). */
    grid-template-columns: repeat(auto-fill, minmax(12rem, 1fr));
    gap: var(--space-2) var(--space-6);
  }

  .address {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-3) var(--space-4);
  }

  .goes-to {
    margin: 0;
  }
</style>
