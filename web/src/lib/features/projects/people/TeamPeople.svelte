<script lang="ts">
  // Accounts added to the team directly (PRJ-046, PRJ-047): found by name, a page at a time, each
  // removable by people who change the team (PRJ-050). Members through a group are not listed one
  // by one: they come and go with the group. References carry no version, so the list is kept
  // here, like a picker's results, and loaded again after every change (`revision`).
  import UserMinus from '@lucide/svelte/icons/user-minus';
  import { resolve } from '$app/paths';

  import type { UserRef } from '#lib/api/schema.gen.ts';
  import { PersonName, accessMessages, personName } from '#lib/features/access/index.ts';
  import * as common from '#lib/i18n/messages/common.ts';
  import { canSeeDirectory } from '#lib/session/index.ts';
  import Button from '#lib/ui/Button.svelte';
  import ErrorState from '#lib/ui/ErrorState.svelte';
  import IconButton from '#lib/ui/IconButton.svelte';
  import Spinner from '#lib/ui/Spinner.svelte';
  import TextField from '#lib/ui/TextField.svelte';

  import { findTeamUsers } from '../projects.ts';
  import * as m from './messages.ts';

  interface Props {
    projectId: number;
    /** Changes after every change of the team; the list loads again. */
    revision: number;
    mayChange: boolean;
    /** Hears which accounts are shown, so pickers do not offer them again. */
    onmembers?: (ids: number[]) => void;
    onremove: (person: UserRef) => void;
  }

  let { projectId, revision, mayChange, onmembers, onremove }: Props = $props();

  const headingId = $props.id();
  let text = $state('');
  let members = $state<UserRef[]>([]);
  let cursor = $state<string | null>(null);
  let loading = $state(false);
  let loadingMore = $state(false);
  let searched = $state(false);
  let failure = $state<unknown>(null);
  let run = 0;

  async function load(more: boolean, id = projectId): Promise<void> {
    const current = ++run;
    if (more) loadingMore = true;
    else loading = true;
    try {
      const page = await findTeamUsers(id, text, more ? cursor : null);
      if (current !== run) return;
      members = more ? [...members, ...page.items] : page.items;
      onmembers?.(members.map((member) => member.id));
      cursor = page.next_cursor;
      searched = true;
      failure = null;
    } catch (error) {
      if (current === run) failure = error;
    } finally {
      if (current === run) loading = loadingMore = false;
    }
  }

  // Search a moment after typing stops; the first page and every reload (a new `revision`) at
  // once.
  $effect(() => {
    const typed = text;
    const id = projectId;
    // Reading `revision` first makes every new one load again.
    const delay = revision >= 0 && typed ? 250 : 0;
    const timer = setTimeout(() => void load(false, id), delay);
    return () => {
      clearTimeout(timer);
    };
  });

  function profileHref(person: UserRef): string | undefined {
    if (!canSeeDirectory() || person.kind !== 'person') return undefined;
    return resolve('/(app)/users/[id]', { id: String(person.id) });
  }
</script>

<section class="part" aria-labelledby={headingId}>
  <h3 id={headingId}>{m.teamPeopleTitle()}</h3>
  {#if cursor !== null || text !== ''}
    <TextField
      class="search"
      label={m.findTeamPeople()}
      type="search"
      hint={m.findTeamPeopleHint()}
      bind:value={text}
      autocomplete="off"
      autocapitalize="none"
      spellcheck="false"
    />
  {/if}
  {#if failure}
    <ErrorState error={failure} level={3} onretry={() => load(false)} />
  {:else if !searched}
    <p class="note"><Spinner label={common.loading()} /></p>
  {:else if members.length === 0}
    <p class="note">{text ? accessMessages.noPeopleFound() : m.noTeamPeople()}</p>
  {:else}
    <ul class="list" aria-busy={loading}>
      {#each members as person (person.id)}
        <li class="row">
          <PersonName {person} withUsername href={profileHref(person)} />
          {#if mayChange}
            <IconButton
              icon={UserMinus}
              label={m.removePersonLabel({ name: personName(person) })}
              onclick={() => {
                onremove(person);
              }}
            />
          {/if}
        </li>
      {/each}
    </ul>
    {#if cursor}
      <div>
        <Button loading={loadingMore} onclick={() => load(true)}>{m.showMore()}</Button>
      </div>
    {/if}
  {/if}
</section>

<style>
  .part {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }

  h3 {
    font-size: var(--text-md);
    font-weight: var(--weight-semibold);
  }

  .part :global(.search) {
    max-width: 24rem;
  }

  .note {
    color: var(--color-text-muted);
    font-size: var(--text-sm);
  }

  .list {
    margin: 0;
    padding: 0;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-lg);
    background: var(--color-surface);
    list-style: none;
  }

  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3);
    min-height: calc(var(--control-md) + var(--space-2));
    padding: var(--space-1) var(--space-2) var(--space-1) var(--space-4);
  }

  .row + .row {
    border-top: 1px solid var(--color-border);
  }
</style>
