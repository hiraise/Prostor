<script lang="ts">
  // The frame of pages for people who are not signed in: sign-in, the first-run setup, and the
  // states of a server that cannot be reached. The logo, the interface settings and one centered
  // column, which fills the width of a phone.
  import type { Snippet } from 'svelte';

  import Logo from './Logo.svelte';
  import PreferencesMenu from './PreferencesMenu.svelte';

  interface Props {
    /** The page's only h1; leave out when the content has its own heading. */
    title?: string;
    width?: 'narrow' | 'wide';
    children: Snippet;
  }

  let { title, width = 'narrow', children }: Props = $props();
</script>

<div class="frame">
  <header class="top">
    <Logo />
    <PreferencesMenu placement={{ side: 'bottom', align: 'end' }} />
  </header>
  <main id="main" tabindex="-1" class={['column', width]}>
    {#if title}<h1>{title}</h1>{/if}
    {@render children()}
  </main>
</div>

<style>
  .frame {
    display: flex;
    flex-direction: column;
    min-height: 100dvh;
    padding: calc(var(--space-3) + var(--safe-top)) calc(var(--space-4) + var(--safe-right))
      calc(var(--space-8) + var(--safe-bottom)) calc(var(--space-4) + var(--safe-left));
  }

  .top {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2);
    width: 100%;
    max-width: 40rem;
    margin: 0 auto;
  }

  .column {
    display: flex;
    flex-direction: column;
    gap: var(--space-5);
    width: 100%;
    margin: var(--space-10) auto 0;
    padding: var(--space-6);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-lg);
    background: var(--color-surface);
    box-shadow: var(--shadow-sm);
  }

  .narrow {
    max-width: 26rem;
  }

  .wide {
    max-width: 36rem;
  }

  .column:focus {
    outline: none;
  }

  h1 {
    font-size: var(--text-xl);
  }

  @media (width < 48rem) {
    .column {
      margin-top: var(--space-6);
      padding: var(--space-5) var(--space-4);
    }
  }
</style>
