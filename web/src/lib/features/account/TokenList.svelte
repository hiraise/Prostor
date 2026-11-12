<script lang="ts">
  // The caller's personal API tokens (AUTH-029): name, access, when created and last used; create
  // and delete. Rows wrap on phones; delete is a visible button, not a hover action (D-019).
  import KeyRound from '@lucide/svelte/icons/key-round';
  import Plus from '@lucide/svelte/icons/plus';
  import Trash2 from '@lucide/svelte/icons/trash-2';

  import type { ApiToken } from '#lib/api/schema.gen.ts';
  import * as common from '#lib/i18n/messages/common.ts';
  import type { ListHandle } from '#lib/store/index.ts';
  import Time from '#lib/time/Time.svelte';
  import Button from '#lib/ui/Button.svelte';
  import Dialog from '#lib/ui/Dialog.svelte';
  import EmptyState from '#lib/ui/EmptyState.svelte';
  import ErrorState from '#lib/ui/ErrorState.svelte';
  import IconButton from '#lib/ui/IconButton.svelte';
  import { showError, toasts } from '#lib/ui/toast.svelte.ts';

  import CreateTokenDialog from './CreateTokenDialog.svelte';
  import * as m from './messages.ts';
  import { accessOf, deleteToken, type Access } from './tokens.ts';

  let { tokens }: { tokens: ListHandle<ApiToken> } = $props();

  let creating = $state(false);
  let deleting = $state<ApiToken | null>(null);
  let confirmOpen = $state(false);
  let busy = $state(false);

  const accessText: Record<Exclude<Access, 'none'>, () => string> = {
    read: m.accessRead,
    write: m.accessWrite,
  };

  function areas(token: ApiToken): string {
    const access = accessOf(token.scopes);
    const parts: string[] = [];
    if (access.tracker !== 'none') {
      parts.push(m.areaAccess({ area: m.trackerArea(), access: accessText[access.tracker]() }));
    }
    if (access.admin !== 'none') {
      parts.push(m.areaAccess({ area: m.adminArea(), access: accessText[access.admin]() }));
    }
    return parts.join(' · ');
  }

  function askToDelete(token: ApiToken): void {
    deleting = token;
    confirmOpen = true;
  }

  async function confirmDelete(): Promise<void> {
    const token = deleting;
    if (!token) return;
    busy = true;
    try {
      await deleteToken(token.id);
      confirmOpen = false;
      toasts.show({ tone: 'success', message: m.tokenDeleted() });
      void tokens.refresh().catch(() => undefined);
    } catch (error) {
      confirmOpen = false;
      showError(error);
    } finally {
      busy = false;
    }
  }
</script>

<div class="tokens">
  <div class="intro">
    <p>{m.tokensIntro()}</p>
    <Button
      icon={Plus}
      onclick={() => {
        creating = true;
      }}
    >
      {m.createToken()}
    </Button>
  </div>

  {#if tokens.error && tokens.ids.length === 0}
    <ErrorState error={tokens.error} level={3} onretry={() => tokens.refresh()} />
  {:else if tokens.ids.length === 0}
    <EmptyState icon={KeyRound} title={m.noTokens()} description={m.noTokensText()} level={3} />
  {:else}
    <ul class="list">
      {#each tokens.items as token (token.id)}
        <li class="token">
          <div class="text">
            <p class="name">{token.name}</p>
            <p class="areas">{areas(token)}</p>
            <p class="times">
              {m.tokenCreated()}
              <Time value={token.created_at} kind="date" /> ·
              {#if token.last_used_at}
                {m.tokenUsed()} <Time value={token.last_used_at} kind="relative" />
              {:else}
                {m.tokenNeverUsed()}
              {/if}
            </p>
          </div>
          <IconButton
            icon={Trash2}
            label={m.deleteTokenLabel({ name: token.name })}
            onclick={() => {
              askToDelete(token);
            }}
          />
        </li>
      {/each}
    </ul>
    {#if tokens.hasMore}
      <div>
        <Button loading={tokens.loadingMore} onclick={() => tokens.loadMore()}>
          {m.loadMore()}
        </Button>
      </div>
    {/if}
  {/if}
</div>

<CreateTokenDialog bind:open={creating} oncreated={() => tokens.refresh()} />

<Dialog
  bind:open={confirmOpen}
  title={m.deleteTokenTitle({ name: deleting?.name ?? '' })}
  size="sm"
  dismissible={!busy}
>
  <p>{m.deleteTokenText()}</p>
  {#snippet footer()}
    <Button
      disabled={busy}
      onclick={() => {
        confirmOpen = false;
      }}
    >
      {common.cancel()}
    </Button>
    <Button variant="danger" loading={busy} onclick={confirmDelete}>{m.deleteToken()}</Button>
  {/snippet}
</Dialog>

<style>
  .tokens {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }

  .intro {
    display: flex;
    flex-wrap: wrap;
    align-items: flex-start;
    justify-content: space-between;
    gap: var(--space-3) var(--space-6);
  }

  .intro p {
    flex: 1 1 20rem;
    max-width: 44rem;
    color: var(--color-text-muted);
  }

  .list {
    margin: 0;
    padding: 0;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-lg);
    background: var(--color-surface);
    list-style: none;
  }

  .token {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    padding: var(--space-3) var(--space-3) var(--space-3) var(--space-4);
  }

  .token + .token {
    border-top: 1px solid var(--color-border);
  }

  .text {
    display: flex;
    flex: 1;
    flex-direction: column;
    gap: var(--space-0_5);
    min-width: 0;
  }

  .name {
    font-weight: var(--weight-medium);
    overflow-wrap: anywhere;
  }

  .areas,
  .times {
    color: var(--color-text-muted);
    font-size: var(--text-sm);
  }
</style>
