<script lang="ts">
  // A place that failed to load (UI-036): what happened in the user's words, the request ID to
  // quote to an administrator, and Try again. Announced when it appears (UI-032).
  import RefreshCw from '@lucide/svelte/icons/refresh-cw';
  import TriangleAlert from '@lucide/svelte/icons/triangle-alert';
  import { onMount } from 'svelte';

  import * as common from '#lib/i18n/messages/common.ts';
  import * as states from '#lib/i18n/messages/states.ts';

  import { announce } from './announce.svelte.ts';
  import Button from './Button.svelte';
  import EmptyState from './EmptyState.svelte';
  import { describeError } from './toast.svelte.ts';

  interface Props {
    error: unknown;
    title?: string;
    /** Replaces the text derived from the error. */
    message?: string;
    level?: 1 | 2 | 3;
    onretry?: () => unknown;
  }

  let { error, title, message, level = 2, onretry }: Props = $props();

  const described = $derived(describeError(error));
  const heading = $derived(title ?? states.loadFailedTitle());
  const text = $derived(message ?? described.message);
  let retrying = $state(false);

  onMount(() => {
    announce(`${heading}. ${text}`, 'assertive');
  });

  async function retry(): Promise<void> {
    if (!onretry) return;
    retrying = true;
    try {
      await onretry();
    } finally {
      retrying = false;
    }
  }
</script>

<EmptyState title={heading} description={text} icon={TriangleAlert} {level} tone="danger">
  {#if described.requestId}
    <p class="request-id">{common.requestId({ id: described.requestId })}</p>
  {/if}
  {#snippet actions()}
    {#if onretry}
      <Button icon={RefreshCw} loading={retrying} onclick={retry}>{common.retry()}</Button>
    {/if}
  {/snippet}
</EmptyState>

<style>
  .request-id {
    color: var(--color-text-muted);
    font-family: var(--font-mono);
    font-size: var(--text-xs);
    user-select: all;
  }
</style>
