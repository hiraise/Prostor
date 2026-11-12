<script lang="ts">
  // What sets an account apart, as small badges: disabled (with its status word), invited and not
  // joined, the guest, and the viewer's own account. Each badge is a word, never color alone.
  import type { AccountSummary } from '#lib/api/schema.gen.ts';
  import { accessMessages, disabledLabels } from '#lib/features/access/index.ts';

  let { account, own = false }: { account: AccountSummary; own?: boolean } = $props();

  const any = $derived(own || account.disabled !== null || account.invited || account.guest);
</script>

{#if any}
  <span class="badges">
    {#if own}<span class="badge neutral">{accessMessages.youBadge()}</span>{/if}
    {#if account.disabled}
      <span class="badge danger">{disabledLabels[account.disabled.label]()}</span>
    {/if}
    {#if account.invited}<span class="badge info">{accessMessages.invitedBadge()}</span>{/if}
    {#if account.guest}<span class="badge neutral">{accessMessages.guestBadge()}</span>{/if}
  </span>
{/if}

<style>
  .badges {
    display: inline-flex;
    flex-wrap: wrap;
    gap: var(--space-1);
  }

  .badge {
    display: inline-flex;
    align-items: center;
    padding: var(--space-0_5) var(--space-1_5);
    border-radius: var(--radius-sm);
    font-size: var(--text-xs);
    font-weight: var(--weight-medium);
    line-height: var(--leading-tight);
    white-space: nowrap;
  }

  .neutral {
    background: var(--color-surface-sunken);
    color: var(--color-text-muted);
  }

  .danger {
    background: var(--color-danger-bg);
    color: var(--color-danger);
  }

  .info {
    background: var(--color-info-bg);
    color: var(--color-info);
  }

  @media (forced-colors: active) {
    .badge {
      border: 1px solid CanvasText;
    }
  }
</style>
