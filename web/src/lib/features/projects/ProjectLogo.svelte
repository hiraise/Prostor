<script lang="ts">
  // A project's logo (PRJ-007): the uploaded picture, or the first letters of its key on a rounded
  // square in the project's own color. Decorative: the project's name is always next to it. A
  // picture that fails to load gives way to the letters.
  interface Props {
    /** The project key, such as `WEB`. */
    projectKey: string;
    /** The project's logo color, 0–7. */
    color: number;
    /** The uploaded picture (`logo_url`). */
    src?: string | null;
    size?: 'sm' | 'md' | 'lg' | 'xl';
  }

  let { projectKey, color, src = null, size = 'md' }: Props = $props();

  // The picture that failed; a new address is tried again.
  let failed = $state<string | null>(null);
  const picture = $derived(src !== null && src !== failed ? src : null);

  // A picture that failed before the error listener was there (from the cache) is caught here.
  function broken(img: HTMLImageElement): void {
    if (img.complete && img.naturalWidth === 0) failed = img.getAttribute('src');
  }

  const letters = $derived(projectKey.replace(/_/gu, '').slice(0, size === 'sm' ? 1 : 2) || '?');
</script>

<span
  class={['logo', size, { picture: picture !== null }]}
  style:--logo-bg={`var(--avatar-${String(((color % 8) + 8) % 8)})`}
  aria-hidden="true"
  >{#if picture !== null}<img
      src={picture}
      alt=""
      decoding="async"
      onerror={() => (failed = picture)}
      {@attach broken}
    />{:else}{letters}{/if}</span
>

<style>
  .logo {
    display: inline-grid;
    flex: none;
    place-items: center;
    border-radius: var(--radius-md);
    background: var(--logo-bg);
    color: var(--color-on-avatar);
    font-family: var(--font-mono);
    font-weight: var(--weight-semibold);
    line-height: 1;
    user-select: none;
  }

  /* Logos keep their own shape; transparent parts show the page. */
  .picture {
    overflow: hidden;
    background: transparent;
  }

  img {
    width: 100%;
    height: 100%;
    object-fit: contain;
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

  .lg {
    width: 3rem;
    height: 3rem;
    border-radius: var(--radius-lg);
    font-size: var(--text-lg);
  }

  /* Pictures being chosen: the account and project settings. */
  .xl {
    width: 5rem;
    height: 5rem;
    border-radius: var(--radius-lg);
    font-size: var(--text-2xl);
  }
</style>
