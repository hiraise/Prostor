<script lang="ts">
  // One permission of the role editor (ACC-034, ACC-044, ACC-046): a checkbox with the name, the
  // key, the description, a "Global only" mark, and what switching it on or off also switches.
  // The whole row is the target; text wraps, so it reads on a 360 px phone.
  import Globe from '@lucide/svelte/icons/globe';

  import type { PermissionInfo, PermissionKey } from '#lib/api/schema.gen.ts';
  import { permissions } from '#lib/features/access/index.ts';
  import Spinner from '#lib/ui/Spinner.svelte';

  import Badge from './Badge.svelte';
  import * as m from './messages.ts';

  interface Props {
    permission: PermissionInfo;
    checked: boolean;
    /** The change is being saved: further clicks are ignored. */
    busy?: boolean;
    disabled?: boolean;
    ontoggle: (on: boolean) => unknown;
  }

  let { permission, checked, busy = false, disabled = false, ontoggle }: Props = $props();

  const id = $props.id();
  const names = (keys: readonly PermissionKey[]): string =>
    keys.map((key) => permissions[key].name()).join(', ');
</script>

<label class={['row', { busy, disabled }]} aria-busy={busy}>
  <input
    type="checkbox"
    {checked}
    {disabled}
    aria-labelledby="{id}-name"
    aria-describedby="{id}-details"
    onclick={(event) => {
      if (busy) event.preventDefault();
    }}
    onchange={(event) => {
      ontoggle(event.currentTarget.checked);
    }}
  />
  <span class="text">
    <span class="title">
      <span id="{id}-name" class="name">{permissions[permission.key].name()}</span>
      {#if permission.global_only}<Badge icon={Globe} text={m.globalOnly()} />{/if}
      <span class="key">{permission.key}</span>
      {#if busy}<span class="spinner"><Spinner label={m.saving()} /></span>{/if}
    </span>
    <span id="{id}-details" class="details">
      {#if permission.global_only}<span class="sr-only">{m.globalOnly()}.</span>{/if}
      <span>{permissions[permission.key].description()}</span>
      {#if permission.implies.length > 0}
        <span class="effect">{m.alsoOn({ names: names(permission.implies) })}</span>
      {/if}
      {#if permission.dependents.length > 0}
        <span class="effect">{m.alsoOff({ names: names(permission.dependents) })}</span>
      {/if}
    </span>
  </span>
</label>

<style>
  .row {
    display: grid;
    grid-template-columns: auto 1fr;
    align-items: start;
    column-gap: var(--space-3);
    min-height: var(--control-sm);
    padding: var(--space-2) var(--space-4);
    cursor: pointer;
  }

  .row:hover:not(.disabled) {
    background: var(--color-hover);
  }

  .disabled {
    cursor: default;
  }

  .busy {
    cursor: progress;
  }

  input {
    width: 1rem;
    height: 1rem;
    margin: 0.15rem 0 0;
    accent-color: var(--color-ink);
    cursor: inherit;
  }

  @media (pointer: coarse) {
    input {
      width: 1.25rem;
      height: 1.25rem;
      margin-top: 0;
    }
  }

  .text {
    display: flex;
    flex-direction: column;
    gap: var(--space-0_5);
    min-width: 0;
  }

  .title {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-1) var(--space-2);
  }

  .name {
    font-weight: var(--weight-medium);
    overflow-wrap: anywhere;
  }

  .key {
    color: var(--color-text-muted);
    font-family: var(--font-mono);
    font-size: var(--text-xs);
    overflow-wrap: anywhere;
  }

  .spinner {
    display: inline-flex;
    color: var(--color-text-muted);
  }

  .details {
    display: flex;
    flex-direction: column;
    gap: var(--space-0_5);
    color: var(--color-text-muted);
    font-size: var(--text-sm);
    overflow-wrap: anywhere;
  }

  .effect {
    color: var(--color-text);
  }
</style>
