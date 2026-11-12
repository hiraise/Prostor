<script lang="ts">
  // A tag on an issue (ISS-034): its name on the tint of its color, a link to the issues with this
  // tag, and a remove button when the issue may be changed. The name carries the meaning; the
  // color only groups (UI-032).
  import { resolve } from '$app/paths';
  import X from '@lucide/svelte/icons/x';

  import type { TagRef } from '#lib/api/schema.gen.ts';

  import * as m from '../messages.ts';

  let { tag, onremove }: { tag: TagRef; onremove?: () => unknown } = $props();

  const scheme = $derived(
    tag.color !== null && tag.color >= 0 && tag.color < 12 ? tag.color : null,
  );
  const href = $derived(`${resolve('/(app)/issues')}?tag=${String(tag.id)}`);
</script>

<span
  class={['tag', { plain: scheme === null }]}
  style:--tag-bg={scheme === null ? undefined : `var(--value-${String(scheme)}-bg)`}
  style:--tag-fg={scheme === null ? undefined : `var(--value-${String(scheme)}-fg)`}
>
  <!-- The address is resolved above. -->
  <!-- eslint-disable-next-line svelte/no-navigation-without-resolve -->
  <a {href} aria-label={m.tagIssues({ name: tag.name })}>{tag.name}</a>
  {#if onremove}
    <button
      type="button"
      class="remove"
      aria-label={m.removeTag({ name: tag.name })}
      onclick={onremove}
    >
      <X size={14} aria-hidden="true" />
    </button>
  {/if}
</span>

<style>
  .tag {
    display: inline-flex;
    align-items: center;
    max-width: 100%;
    border: 1px solid transparent;
    border-radius: var(--radius-full);
    background: var(--tag-bg);
    color: var(--tag-fg);
    font-size: var(--text-sm);
    font-weight: var(--weight-medium);
    line-height: var(--leading-normal);
  }

  .plain {
    border-color: var(--color-border-control);
    background: transparent;
    color: var(--color-text);
  }

  a {
    padding: 0.0625rem var(--space-2);
    color: inherit;
    text-decoration: none;
    overflow-wrap: anywhere;
  }

  a:hover {
    text-decoration: underline;
  }

  .remove {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-width: 1.5rem;
    min-height: 1.5rem;
    margin-inline-end: var(--space-0_5);
    padding: 0;
    border: 0;
    border-radius: var(--radius-full);
    background: transparent;
    color: inherit;
    cursor: pointer;
  }

  .remove:hover {
    background: color-mix(in oklab, currentColor 15%, transparent);
  }

  /* Phones: targets of at least 44 px (design §11). */
  @media (width < 48rem) {
    .remove,
    a {
      display: inline-flex;
      align-items: center;
      justify-content: center;
      min-width: 2.75rem;
      min-height: 2.75rem;
    }

    .remove {
      margin-inline-end: 0;
    }
  }

  @media (forced-colors: active) {
    .tag {
      border-color: CanvasText;
    }
  }
</style>
