<script lang="ts">
  // Who holds a role and where (ACC-057): people and groups with the scope and the date. Each row
  // links to the person's profile or the group's page; the whole row is the target (D-019).
  import Users from '@lucide/svelte/icons/users';
  import { resolve } from '$app/paths';

  import { PersonName, groupName } from '#lib/features/access/index.ts';
  import type { ListHandle } from '#lib/store/index.ts';
  import Time from '#lib/time/Time.svelte';
  import Button from '#lib/ui/Button.svelte';
  import ErrorState from '#lib/ui/ErrorState.svelte';

  import * as m from './messages.ts';
  import type { RoleAssignment } from './roles.ts';

  let { holders }: { holders: ListHandle<RoleAssignment> } = $props();

  const id = $props.id();

  /** Where a holder's row leads; deleted and hidden people have no page to open. */
  function href(assignment: RoleAssignment): string | undefined {
    const { holder } = assignment;
    if (holder.kind === 'group') {
      return resolve('/(app)/admin/groups/[id]', { id: String(holder.group.id) });
    }
    if (holder.user.kind !== 'person') return undefined;
    return resolve('/(app)/users/[id]', { id: String(holder.user.id) });
  }
</script>

<section class="holders" aria-labelledby="{id}-title">
  <h2 id="{id}-title">{m.holdersTitle()}</h2>

  {#if holders.error && holders.ids.length === 0}
    <ErrorState error={holders.error} level={3} onretry={() => holders.refresh()} />
  {:else if holders.ids.length === 0}
    <p class="none">{m.noHolders()}</p>
  {:else}
    <ul class="list">
      {#each holders.items as assignment (assignment.id)}
        {@const link = href(assignment)}
        <li>
          <svelte:element this={link ? 'a' : 'div'} class="holder" href={link}>
            {#if assignment.holder.kind === 'user'}
              <PersonName person={assignment.holder.user} withUsername />
            {:else}
              <span class="group">
                <span class="group-icon"><Users aria-hidden="true" /></span>
                <span class="group-name">{groupName(assignment.holder.group)}</span>
              </span>
            {/if}
            <span class="meta">
              {#if assignment.holder.kind === 'group'}{m.group()} ·{/if}
              {assignment.project_id === null
                ? m.globalScope()
                : m.projectScope({ id: assignment.project_id })} ·
              {m.assigned()}
              <Time value={assignment.created_at} kind="date" />
            </span>
          </svelte:element>
        </li>
      {/each}
    </ul>
    {#if holders.hasMore}
      <div>
        <Button loading={holders.loadingMore} onclick={() => holders.loadMore()}>
          {m.showMore()}
        </Button>
      </div>
    {/if}
  {/if}
</section>

<style>
  .holders {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }

  h2 {
    font-size: var(--text-lg);
  }

  .none {
    color: var(--color-text-muted);
  }

  .list {
    margin: 0;
    padding: 0;
    overflow: hidden;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-lg);
    background: var(--color-surface);
    list-style: none;
  }

  li + li {
    border-top: 1px solid var(--color-border);
  }

  .holder {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-1) var(--space-4);
    min-height: var(--control-lg);
    padding: var(--space-2) var(--space-4);
    color: inherit;
    text-decoration: none;
  }

  a.holder:hover {
    background: var(--color-hover);
  }

  a.holder:hover :global(.name),
  a.holder:hover .group-name {
    color: var(--color-link);
    text-decoration: underline;
  }

  .group {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    min-width: 0;
  }

  /* The size of a small avatar, so people and groups line up. */
  .group-icon {
    display: inline-flex;
    flex: none;
    align-items: center;
    justify-content: center;
    width: 1.5rem;
    height: 1.5rem;
    border-radius: var(--radius-full);
    background: var(--color-surface-sunken);
    color: var(--color-text-muted);
  }

  .group-icon :global(svg) {
    width: var(--icon-sm);
    height: var(--icon-sm);
  }

  .group-name {
    font-weight: var(--weight-medium);
    overflow-wrap: anywhere;
  }

  .meta {
    color: var(--color-text-muted);
    font-size: var(--text-sm);
  }
</style>
