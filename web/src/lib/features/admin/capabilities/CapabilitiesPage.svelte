<script lang="ts">
  import { brand } from '#lib/i18n/messages/brand.ts';
  // The «Возможности» screen of the administration area (ACC-064, ADM-023; plan step 1.11 §6.1):
  // features switched on for chosen groups — a name, a description and a group multiselect with a
  // search per row, one save per row, and a confirmation when a save would switch a capability
  // off for everybody. The «Общие для всех» flags render only while the registry has any (it is
  // empty in P1); they change through the settings API and apply without a restart.
  import { onMount } from 'svelte';

  import type { AdminFlag, CapabilityKey, Capabilities, Group } from '#lib/api/schema.gen.ts';
  import { groupName } from '#lib/features/access/index.ts';
  import * as common from '#lib/i18n/messages/common.ts';
  import { can } from '#lib/session/index.ts';
  import Button from '#lib/ui/Button.svelte';
  import Checkbox from '#lib/ui/Checkbox.svelte';
  import Dialog from '#lib/ui/Dialog.svelte';
  import ErrorState from '#lib/ui/ErrorState.svelte';
  import Notice from '#lib/ui/Notice.svelte';
  import OptionList from '#lib/ui/OptionList.svelte';
  import PageHeader from '#lib/ui/PageHeader.svelte';
  import Spinner from '#lib/ui/Spinner.svelte';
  import { showError, toasts } from '#lib/ui/toast.svelte.ts';

  import * as m from './messages.ts';
  import {
    allGroups,
    instanceFlags,
    listCapabilities,
    saveInstanceFlag,
    setCapability,
  } from './capabilities.ts';

  interface Props {
    /** The capabilities as loaded; `null` loads them here, `undefined` when the viewer may not see them. */
    capabilities: Capabilities | null | undefined;
  }

  let { capabilities }: Props = $props();

  const mayWrite = $derived(can('system.write'));

  // What each machine key is called and does, our texts for the screen. A key the screen does
  // not know (a newer server) falls back to the machine word itself.
  const TEXTS: Partial<Record<CapabilityKey, { name: () => string; description: () => string }>> = {
    share_views: { name: m.shareViewsName, description: m.shareViewsDescription },
  };

  let groups = $state<Group[] | null>(null);
  let flags = $state<AdminFlag[] | null>(null);
  let failure = $state<unknown>(null);

  // The groups chosen per key: what a save sends. Starts from the stored list, drops on save.
  let drafts = $state<Record<string, number[]>>({});
  /** One search field per row, the queries by key. */
  let queries = $state<Record<string, string>>({});
  let busy = $state<string | null>(null);
  /** The row waiting for the «switch off for everybody?» confirmation. */
  let confirmingOff = $state<string | null>(null);

  const stored = $derived.by(() => {
    const lists: Record<string, number[]> = {};
    capabilities?.items.forEach((item) => {
      lists[item.key] = item.groups.map((g) => g.id);
    });
    return lists;
  });

  function chosen(key: string): number[] {
    return drafts[key] ?? stored[key] ?? [];
  }

  function changed(key: string): boolean {
    const now = chosen(key);
    const was = stored[key] ?? [];
    return now.length !== was.length || now.some((id) => !was.includes(id));
  }

  function groupsMatching(key: string): Group[] {
    const text = (queries[key] ?? '').trim().toLowerCase();
    if (text === '' || groups === null) return groups ?? [];
    // A chosen group stays listed while the search would hide it: the person must see what a save
    // sends and be able to take a group out without clearing the search.
    const chosenNow = new Set(chosen(key));
    return groups.filter(
      (group) => chosenNow.has(group.id) || groupName(group).toLowerCase().includes(text),
    );
  }

  function toggle(key: string, id: number, on: boolean): void {
    const now = chosen(key);
    drafts = { ...drafts, [key]: on ? [...now, id] : now.filter((other) => other !== id) };
  }

  async function load(): Promise<void> {
    if (capabilities === undefined) return;
    failure = null;
    try {
      const [list, every, settings] = await Promise.all([
        listCapabilities(),
        allGroups(),
        instanceFlags(),
      ]);
      capabilities = list;
      groups = every;
      flags = settings.flags;
      // The search field of a row shows only once its query exists (the OptionList hides it
      // while `query` is undefined), so every row starts with an empty one.
      queries = Object.fromEntries(list.items.map((item) => [item.key, '']));
    } catch (error) {
      failure = error;
    }
  }

  async function save(key: string): Promise<void> {
    if (busy !== null || !capabilities) return;
    const item = capabilities.items.find((found) => found.key === key);
    if (!item) return;
    const next = chosen(key);
    // An empty list switches a switchable capability off for everybody: ask first (ADM-023).
    if (next.length === 0 && item.groups.length > 0 && item.can_be_switched_off) {
      confirmingOff = key;
      return;
    }
    await write(key, next);
  }

  async function write(key: string, next: number[]): Promise<void> {
    if (!capabilities) return;
    busy = key;
    try {
      const saved = await setCapability(key as CapabilityKey, next);
      capabilities = {
        ...capabilities,
        items: capabilities.items.map((item) =>
          item.key === key ? { ...item, groups: saved.groups } : item,
        ),
      };
      drafts = Object.fromEntries(Object.entries(drafts).filter(([one]) => one !== key));
      toasts.show({ tone: 'success', message: m.savedToast() });
    } catch (error) {
      showError(error);
    } finally {
      busy = null;
      confirmingOff = null;
    }
  }

  async function switchFlag(flag: AdminFlag, on: boolean): Promise<void> {
    if (!on && !flag.may_switch_off) {
      toasts.show({ tone: 'error', message: m.flagMayNotSwitchOff() });
      return;
    }
    try {
      flags = (await saveInstanceFlag(flag.key, on)).flags;
      toasts.show({ tone: 'success', message: m.flagSavedToast() });
    } catch (error) {
      showError(error);
    }
  }

  async function resetFlag(flag: AdminFlag): Promise<void> {
    try {
      flags = (await saveInstanceFlag(flag.key, null)).flags;
      toasts.show({ tone: 'success', message: m.flagSavedToast() });
    } catch (error) {
      showError(error);
    }
  }

  onMount(() => void load());
</script>

<svelte:head><title>{m.capabilitiesTitle()} · {brand()}</title></svelte:head>

<div class="capabilities-page">
  <PageHeader title={m.capabilitiesTitle()} />
  <p class="intro">{m.capabilitiesIntro()}</p>
  {#if failure !== null}
    <ErrorState error={failure} level={3} onretry={() => void load()} />
  {:else if capabilities === null || capabilities === undefined}
    <p class="loading"><Spinner label={common.loading()} /></p>
  {:else}
    {#if !mayWrite}<Notice tone="info">{m.readOnlyNote()}</Notice>{/if}

    {#each capabilities.items as item (item.key)}
      {@const texts = TEXTS[item.key]}
      <section class="capability" aria-labelledby="capability-{item.key}">
        <h2 id="capability-{item.key}">{texts?.name() ?? item.key}</h2>
        <p class="description">{texts?.description() ?? ''}</p>
        {#if chosen(item.key).length === 0}
          <p class="off">{m.offForEverybody()}</p>
        {/if}
        {#if groups !== null}
          {@const options = groupsMatching(item.key)}
          <OptionList
            label={m.groupsLegend()}
            mode="multiple"
            {options}
            key={(group: Group) => group.id}
            checked={(group: Group) => chosen(item.key).includes(group.id)}
            disabled={() => !mayWrite || busy === item.key}
            onchange={(group: Group, on: boolean) => toggle(item.key, group.id, on)}
            bind:query={queries[item.key]}
            searchLabel={m.findGroupsLabel()}
          >
            {#snippet option(group: Group)}
              {groupName(group)}
            {/snippet}
          </OptionList>
          {#if options.length === 0}<p class="note">{m.noGroupsFound()}</p>{/if}
        {:else}
          <Spinner label={common.loading()} />
        {/if}
        {#if mayWrite}
          <div class="actions">
            <Button
              variant="primary"
              disabled={!changed(item.key)}
              loading={busy === item.key}
              onclick={() => void save(item.key)}
            >
              {m.saveCapability()}
            </Button>
          </div>
        {/if}
      </section>
    {/each}

    {#if flags !== null && flags.length > 0}
      <section class="flags" aria-labelledby="instance-flags">
        <h2 id="instance-flags">{m.flagsTitle()}</h2>
        <p class="description">{m.flagsIntro()}</p>
        <ul class="flag-list">
          {#each flags as flag (flag.key)}
            <li class="flag">
              <Checkbox
                label={flag.description}
                disabled={!mayWrite || !flag.may_switch_off}
                hint={flag.is_stored ? m.flagStored() : undefined}
                bind:checked={
                  () => flag.value,
                  (on: boolean) => {
                    void switchFlag(flag, on);
                  }
                }
              />
              {#if mayWrite && flag.is_stored}
                <Button variant="ghost" onclick={() => void resetFlag(flag)}>
                  {m.flagDefault()}
                </Button>
              {/if}
            </li>
          {/each}
        </ul>
      </section>
    {/if}
  {/if}

  {#if confirmingOff !== null}
    <Dialog
      open={true}
      title={m.switchOffTitle()}
      size="sm"
      dismissible={busy === null}
      onclose={() => {
        if (busy === null) confirmingOff = null;
      }}
    >
      <p>{m.switchOffText()}</p>
      {#snippet footer()}
        <Button
          disabled={busy !== null}
          onclick={() => {
            confirmingOff = null;
          }}
        >
          {common.cancel()}
        </Button>
        <Button
          variant="danger"
          loading={busy !== null}
          onclick={() => {
            const key = confirmingOff;
            if (key !== null) void write(key, []);
          }}
        >
          {m.switchOffConfirm()}
        </Button>
      {/snippet}
    </Dialog>
  {/if}
</div>

<style>
  .capabilities-page {
    display: flex;
    flex-direction: column;
    gap: var(--space-6);
  }
  .intro {
    margin: 0;
    padding: 0 var(--space-6);
    color: var(--color-text-muted);
  }
  .loading {
    color: var(--color-text-muted);
  }
  .capability,
  .flags {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    padding: 0 var(--space-6);
  }
  h2 {
    font-size: var(--text-lg);
  }
  .description,
  .off,
  .note {
    margin: 0;
    color: var(--color-text-muted);
  }
  .off {
    font-weight: var(--weight-medium);
  }
  .actions {
    display: flex;
    gap: var(--space-2);
  }
  .flag-list {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .flag {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-2) var(--space-4);
  }
  @media (width < 48rem) {
    .capability,
    .flags {
      padding: 0 var(--space-4);
    }
    .intro {
      padding: 0 var(--space-4);
    }
  }
</style>
