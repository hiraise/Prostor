<script lang="ts">
  // The watcher list while the popup is open (NTF-015, NTF-016, plan §4.3): it loads when the
  // popup opens and leaves with it. Adding picks from the people who can read the issue, as the
  // mention list offers them; removing one's own marker needs no right, moving anyone else's
  // needs `issue.watchers.update` — the server says so when the UI is wrong.
  import X from '@lucide/svelte/icons/x';

  import type { Issue, UserRef } from '#lib/api/schema.gen.ts';
  import { personName, personUsername } from '#lib/features/access/index.ts';
  import { mentionCandidates } from '#lib/features/content/content.ts';
  import * as common from '#lib/i18n/messages/common.ts';
  import { currentUser } from '#lib/session/index.ts';
  import EmptyState from '#lib/ui/EmptyState.svelte';
  import IconButton from '#lib/ui/IconButton.svelte';
  import OptionList from '#lib/ui/OptionList.svelte';
  import Spinner from '#lib/ui/Spinner.svelte';
  import { describeError, showError } from '#lib/ui/toast.svelte.ts';

  import * as m from './messages.ts';
  import { loadWatchers, saveWatchers } from './watchers.ts';

  let { issue }: { issue: Issue } = $props();

  let list = $state<UserRef[] | null>(null);
  let failed = $state(false);
  let busy = $state(false);
  let query = $state('');
  let found = $state<UserRef[]>([]);
  let loading = $state(false);
  let problem = $state<string | null>(null);

  const me = $derived(currentUser()?.id ?? null);
  const manage = $derived(issue.can.manage_watchers);
  const ids = $derived(list?.map((user) => user.id) ?? []);

  /** Loads the list; the popup opening this content runs it once, «Повторить» again. */
  function load(): void {
    list = null;
    failed = false;
    loadWatchers(issue.id).then(
      (users) => {
        list = users;
      },
      (error: unknown) => {
        failed = true;
        showError(error);
      },
    );
  }

  $effect(() => {
    load();
  });

  // Typing replaces the suggested people after a pause, as the sidebar's pickers do.
  $effect(() => {
    const text = query;
    const controller = new AbortController();
    const timer = setTimeout(
      () => {
        loading = true;
        problem = null;
        mentionCandidates(issue.id, text, controller.signal).then(
          (users) => {
            if (!controller.signal.aborted) {
              found = users;
              loading = false;
            }
          },
          (failure: unknown) => {
            if (controller.signal.aborted) return;
            problem = describeError(failure).message;
            loading = false;
          },
        );
      },
      text ? 250 : 0,
    );
    return () => {
      clearTimeout(timer);
      controller.abort();
    };
  });

  /** The people to offer: ones who can read the issue and do not watch it yet. */
  const offers = $derived(found.filter((user) => !ids.includes(user.id)));

  /** Sends the change and shows the answer or the refusal, once at a time. */
  function apply(change: Promise<UserRef[]>): void {
    if (busy) return;
    busy = true;
    change.then(
      (users) => {
        list = users;
        busy = false;
      },
      (error: unknown) => {
        showError(error);
        busy = false;
      },
    );
  }

  /** Replaces the list without one person. */
  function drop(user: UserRef): void {
    if (list === null) return;
    apply(
      saveWatchers(
        issue,
        ids.filter((id) => id !== user.id),
      ),
    );
  }

  /** Adds one person to the list. */
  function add(user: UserRef): void {
    if (list === null) return;
    apply(saveWatchers(issue, [...ids, user.id]));
  }
</script>

<div class="popup">
  {#if list === null}
    {#if failed}
      <EmptyState title={m.watchersFailed()}>
        {#snippet actions()}
          <button type="button" onclick={load}>{common.retry()}</button>
        {/snippet}
      </EmptyState>
    {:else}
      <div class="loading"><Spinner label={m.watchersLoading()} /></div>
    {/if}
  {:else}
    <ul class="people">
      {#each list as user (user.id)}
        <li>
          <span class="who">
            <span>{personName(user)}</span>
            {#if user.id === me}<span class="muted">{m.you()}</span>{/if}
            <span class="muted">{personUsername(user)}</span>
          </span>
          {#if user.id === me || manage}
            <IconButton
              icon={X}
              size="sm"
              label={m.removeWatcher({ name: personName(user) })}
              disabled={busy}
              onclick={() => drop(user)}
            />
          {/if}
        </li>
      {:else}
        <li class="empty">{m.noWatchers()}</li>
      {/each}
    </ul>
    {#if manage}
      <OptionList
        label={m.addWatcher()}
        options={offers}
        key={(user: UserRef) => user.id}
        checked={() => false}
        onchange={(user: UserRef) => add(user)}
        bind:query
        searchLabel={m.findWatcher()}
        {loading}
        {problem}
        disabled={() => busy}
      >
        {#snippet option(user: UserRef)}
          {personName(user)}
        {/snippet}
        {#snippet secondary(user: UserRef)}
          {personUsername(user)}
        {/snippet}
      </OptionList>
    {/if}
  {/if}
</div>

<style>
  .popup {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    min-width: 18rem;
  }

  .loading {
    display: flex;
    justify-content: center;
    padding: var(--space-4);
  }

  .people {
    display: flex;
    flex-direction: column;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .people li {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2);
    min-height: 2.25rem;
    padding: var(--space-1) 0;
  }

  .who {
    display: flex;
    align-items: baseline;
    gap: var(--space-2);
    flex-wrap: wrap;
  }

  .muted {
    color: var(--color-text-muted);
    font-size: var(--text-sm);
  }

  .empty {
    padding: var(--space-2) 0;
    color: var(--color-text-muted);
  }
</style>
