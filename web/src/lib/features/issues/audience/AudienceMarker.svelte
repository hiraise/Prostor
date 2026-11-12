<script lang="ts">
  // Who sees an issue (ISS-016, ISS-041): everyone who sees the project, or a lock with
  // «Ограничена» and the audience, reporter included, as its description.
  import Lock from '@lucide/svelte/icons/lock';
  import UsersRound from '@lucide/svelte/icons/users-round';

  import type { Audience } from '#lib/api/schema.gen.ts';

  import * as m from '../messages.ts';
  import { audienceText } from './audience.ts';

  let { audience }: { audience: Audience } = $props();

  const id = $props.id();
  const text = $derived(audienceText(audience));
</script>

{#if text}
  <span class="marker restricted" aria-describedby="{id}-who">
    <Lock size={16} aria-hidden="true" />
    <span>{m.restricted()}</span>
    <span class="who" id="{id}-who">{text}</span>
  </span>
{:else}
  <span class="marker">
    <UsersRound size={16} aria-hidden="true" />
    <span>{m.visibleToAll()}</span>
  </span>
{/if}

<style>
  .marker {
    display: inline-flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-1);
    color: var(--color-text-muted);
  }

  .restricted {
    color: var(--color-text);
  }

  .who {
    color: var(--color-text-muted);
    font-size: var(--text-sm);
  }
</style>
