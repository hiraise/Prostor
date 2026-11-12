<script lang="ts">
  // A person's picture, or their initials in a circle. Decorative: the name next to it, or the
  // label of the control around it, says who it is. Each account has its own background color for
  // life (ACC-017); without one the circle is ink. A picture that fails to load gives way to the
  // initials.
  interface Props {
    name: string;
    size?: 'sm' | 'md' | 'lg' | 'xl';
    /** The account's avatar color, 0–7. */
    color?: number | null;
    /** The uploaded or imported picture (`avatar_url`). */
    src?: string | null;
  }

  let { name, size = 'md', color = null, src = null }: Props = $props();

  // The picture that failed; a new address is tried again.
  let failed = $state<string | null>(null);
  const picture = $derived(src !== null && src !== failed ? src : null);

  // A picture that failed before the error listener was there (from the cache) is caught here.
  function broken(img: HTMLImageElement): void {
    if (img.complete && img.naturalWidth === 0) failed = img.getAttribute('src');
  }

  const segmenter = new Intl.Segmenter();
  // The first character of the first two words, as people see characters (letters with marks,
  // emoji).
  const initials = $derived(
    name
      .trim()
      .split(/\s+/u)
      .filter(Boolean)
      .slice(0, 2)
      .map((word) => segmenter.segment(word)[Symbol.iterator]().next().value?.segment ?? '')
      .join('')
      .toLocaleUpperCase() || '?',
  );
</script>

<span
  class={['avatar', size, { colored: color !== null, picture: picture !== null }]}
  style:--avatar-bg={color === null ? undefined : `var(--avatar-${String(((color % 8) + 8) % 8)})`}
  aria-hidden="true"
  >{#if picture !== null}<img
      src={picture}
      alt=""
      decoding="async"
      onerror={() => (failed = picture)}
      {@attach broken}
    />{:else}{initials}{/if}</span
>

<style>
  .avatar {
    display: inline-grid;
    flex: none;
    place-items: center;
    border-radius: var(--radius-full);
    background: var(--color-ink);
    color: var(--color-on-ink);
    font-weight: var(--weight-semibold);
    line-height: 1;
    user-select: none;
  }

  .picture {
    overflow: hidden;
    background: var(--color-surface-sunken);
  }

  img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }

  .colored {
    background: var(--avatar-bg);
    color: var(--color-on-avatar);
  }

  .lg {
    width: 3rem;
    height: 3rem;
    font-size: var(--text-lg);
  }

  .sm {
    width: 1.5rem;
    height: 1.5rem;
    font-size: var(--text-xs);
  }

  .md {
    width: 2rem;
    height: 2rem;
    font-size: var(--text-sm);
  }

  /* Pictures being chosen: the account and project settings. */
  .xl {
    width: 5rem;
    height: 5rem;
    font-size: var(--text-2xl);
  }
</style>
