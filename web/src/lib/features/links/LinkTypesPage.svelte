<script lang="ts">
  // The link types of the instance (LNK-018…LNK-021), oldest first: each with its internal name,
  // the words its sides use, its direction and how many links it has; built-in types are marked,
  // and the locked ones (duplicates, subtasks) offer no actions. System administrators create,
  // change and delete types; deleted types wait in their own section until restored. Rows are
  // cards on phones (D-019).
  import Ellipsis from '@lucide/svelte/icons/ellipsis';
  import Link2 from '@lucide/svelte/icons/link-2';
  import Lock from '@lucide/svelte/icons/lock';
  import Pencil from '@lucide/svelte/icons/pencil';
  import Plus from '@lucide/svelte/icons/plus';
  import RotateCcw from '@lucide/svelte/icons/rotate-ccw';
  import Trash2 from '@lucide/svelte/icons/trash-2';

  import { ApiError } from '#lib/api/client.ts';
  import type { LinkType } from '#lib/api/schema.gen.ts';
  import { can, canSeeLinkTypes } from '#lib/session/index.ts';
  import type { ListHandle } from '#lib/store/index.ts';
  import Button from '#lib/ui/Button.svelte';
  import EmptyState from '#lib/ui/EmptyState.svelte';
  import ErrorState from '#lib/ui/ErrorState.svelte';
  import ForbiddenState from '#lib/ui/ForbiddenState.svelte';
  import IconButton from '#lib/ui/IconButton.svelte';
  import Menu, { type MenuEntry } from '#lib/ui/Menu.svelte';
  import PageHeader from '#lib/ui/PageHeader.svelte';
  import { showError, toasts } from '#lib/ui/toast.svelte.ts';

  import DeleteLinkTypeDialog from './DeleteLinkTypeDialog.svelte';
  import LinkTypeDialog from './LinkTypeDialog.svelte';
  import { restoreLinkType } from './link-types.ts';
  import * as m from './messages.ts';

  interface Props {
    /** Every link type; `null` when the viewer may not see them. */
    types: ListHandle<LinkType> | null;
    /** Where "forbidden" offers to go. */
    home?: string;
  }

  let { types, home }: Props = $props();

  let editing = $state(false);
  /** The type the dialog changes; `null` creates one. */
  let chosen = $state<LinkType | null>(null);
  let deleting = $state(false);
  let doomed = $state<LinkType | null>(null);
  let restoring = $state<number | null>(null);

  const mayWrite = $derived(can('system.write'));
  const live = $derived(types?.items.filter((type) => type.deleted_at === null) ?? []);
  const trashed = $derived(types?.items.filter((type) => type.deleted_at !== null) ?? []);

  function refresh(): void {
    void types?.refresh().catch(() => undefined);
  }

  function create(): void {
    chosen = null;
    editing = true;
  }

  function menuOf(type: LinkType): MenuEntry[] {
    return [
      {
        type: 'action',
        label: m.edit(),
        icon: Pencil,
        onSelect: () => {
          chosen = type;
          editing = true;
        },
      },
      {
        type: 'action',
        label: m.remove(),
        icon: Trash2,
        onSelect: () => {
          doomed = type;
          deleting = true;
        },
      },
    ];
  }

  async function restore(type: LinkType): Promise<void> {
    restoring = type.id;
    try {
      await restoreLinkType(type.id);
      toasts.show({ tone: 'success', message: m.restored() });
    } catch (error) {
      const taken =
        error instanceof ApiError &&
        error.code === 'invalid_request' &&
        (error.body?.fields ?? []).some((field) => field.code === 'taken');
      if (taken) toasts.show({ tone: 'error', message: m.restoreTaken() });
      else showError(error);
    } finally {
      restoring = null;
      refresh();
    }
  }

  function sides(type: LinkType): string {
    return type.direction === 'undirected' || type.target_name === null
      ? m.bothSides({ name: type.source_name })
      : m.twoSides({ source: type.source_name, target: type.target_name });
  }
</script>

<svelte:head><title>{m.linkTypesTitle()} · trackflow</title></svelte:head>

{#snippet details(type: LinkType)}
  <span class="details">
    <span>{sides(type)}</span>
    <span>
      {[
        m.directionNames[type.direction](),
        type.usage === null ? null : m.usageCount({ count: type.usage }),
      ]
        .filter(Boolean)
        .join(' · ')}
    </span>
  </span>
{/snippet}

{#if !types || !canSeeLinkTypes()}
  <ForbiddenState {home} />
{:else}
  <PageHeader title={m.linkTypesTitle()}>
    {#snippet actions()}
      {#if mayWrite}
        <Button variant="primary" icon={Plus} onclick={create}>{m.createLinkType()}</Button>
      {/if}
    {/snippet}
  </PageHeader>

  <div class="page">
    <p class="intro">{m.linkTypesIntro()}</p>
    {#if !mayWrite}<p class="note">{m.readOnlyNote()}</p>{/if}

    {#if types.error && types.ids.length === 0}
      <ErrorState error={types.error} level={2} onretry={() => types.refresh()} />
    {:else if live.length === 0 && trashed.length === 0}
      <EmptyState icon={Link2} title={m.noLinkTypes()} description={m.noLinkTypesText()} />
    {:else}
      <ul class="list" aria-label={m.linkTypeList()} aria-busy={types.refreshing}>
        {#each live as type (type.id)}
          <li class="type">
            <div class="line">
              <span class="title">
                <span class="name">
                  {type.name}
                </span>
                {#if type.locked}
                  <span class="badge"><Lock aria-hidden="true" />{m.locked()}</span>
                {:else if type.builtin}
                  <span class="badge">{m.builtIn()}</span>
                {/if}
              </span>
              {#if mayWrite && !type.locked}
                <Menu label={m.typeActions({ name: type.name })} items={menuOf(type)}>
                  {#snippet trigger(props)}
                    <IconButton
                      icon={Ellipsis}
                      label={m.typeActions({ name: type.name })}
                      {...props}
                    />
                  {/snippet}
                </Menu>
              {/if}
            </div>
            {@render details(type)}
          </li>
        {/each}
      </ul>

      {#if trashed.length > 0}
        <section class="trash" aria-labelledby="deleted-link-types">
          <h2 id="deleted-link-types">{m.deletedTitle()}</h2>
          <p class="note">{m.deletedIntro()}</p>
          <ul class="list" aria-label={m.deletedTitle()}>
            {#each trashed as type (type.id)}
              <li class="type">
                <div class="line">
                  <span class="title">
                    <span class="name">{type.name}</span>
                  </span>
                  {#if mayWrite}
                    <Button
                      icon={RotateCcw}
                      loading={restoring === type.id}
                      aria-label={m.restoreNamed({ name: type.name })}
                      onclick={() => void restore(type)}
                    >
                      {m.restore()}
                    </Button>
                  {/if}
                </div>
                {@render details(type)}
              </li>
            {/each}
          </ul>
        </section>
      {/if}
    {/if}
  </div>

  {#if mayWrite}
    <LinkTypeDialog bind:open={editing} type={chosen} ondone={refresh} />
    {#if doomed}
      <DeleteLinkTypeDialog bind:open={deleting} type={doomed} ondeleted={refresh} />
    {/if}
  {/if}
{/if}

<style>
  .page {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
    padding: var(--space-2) var(--space-6) var(--space-12);
  }

  .intro {
    max-width: 44rem;
    color: var(--color-text-muted);
  }

  .note {
    color: var(--color-text-muted);
    font-size: var(--text-sm);
  }

  .trash {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    margin-top: var(--space-4);
  }

  h2 {
    font-size: var(--text-lg);
    font-weight: var(--weight-semibold);
  }

  .list {
    margin: 0;
    padding: 0;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-lg);
    background: var(--color-surface);
    list-style: none;
  }

  .type {
    display: flex;
    flex-direction: column;
    gap: var(--space-0_5);
    padding: var(--space-2) var(--space-2) var(--space-3) var(--space-4);
  }

  .type + .type {
    border-top: 1px solid var(--color-border);
  }

  .line {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    min-height: var(--control-md);
  }

  .title {
    display: flex;
    flex: 1;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-1) var(--space-2);
    min-width: 0;
  }

  .name {
    font-weight: var(--weight-medium);
    overflow-wrap: anywhere;
  }

  .badge {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
    max-width: 100%;
    padding: 0.0625rem var(--space-2);
    border-radius: var(--radius-full);
    background: var(--color-surface-sunken);
    color: var(--color-text-muted);
    font-size: var(--text-xs);
    font-weight: var(--weight-medium);
    line-height: var(--leading-normal);
  }

  .badge :global(svg) {
    flex: none;
    width: var(--icon-sm);
    height: var(--icon-sm);
  }

  .details {
    display: flex;
    flex-direction: column;
    gap: var(--space-0_5);
    color: var(--color-text-muted);
    font-size: var(--text-sm);
    font-variant-numeric: tabular-nums;
    overflow-wrap: anywhere;
  }

  @media (forced-colors: active) {
    .badge {
      border: 1px solid CanvasText;
    }
  }

  /* Phones: each type is a card of its own (D-019). */
  @media (width < 48rem) {
    .page {
      padding: var(--space-2) var(--space-4) var(--space-8);
    }

    .list {
      display: flex;
      flex-direction: column;
      gap: var(--space-2);
      border: none;
      background: none;
    }

    .type {
      border: 1px solid var(--color-border);
      border-radius: var(--radius-lg);
      background: var(--color-surface);
      box-shadow: var(--shadow-sm);
    }

    .type + .type {
      border-top: 1px solid var(--color-border);
    }
  }
</style>
