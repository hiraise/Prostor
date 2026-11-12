<script lang="ts">
  // A loading indicator. With `label` it is a status message for screen readers; without it,
  // it is decoration next to text that already says what is happening.
  let { label }: { label?: string } = $props();
</script>

<span class="spinner" role={label ? 'status' : undefined}>
  <svg viewBox="0 0 16 16" aria-hidden="true">
    <circle cx="8" cy="8" r="6" fill="none" stroke-width="2" class="track" />
    <path d="M8 2a6 6 0 0 1 6 6" fill="none" stroke-width="2" stroke-linecap="round" />
  </svg>
  {#if label}<span class="sr-only">{label}</span>{/if}
</span>

<style>
  .spinner {
    display: inline-flex;
    width: 1em;
    height: 1em;
  }

  svg {
    width: 100%;
    height: 100%;
    stroke: currentColor;
    animation: spin 0.8s linear infinite;
  }

  .track {
    opacity: 0.25;
  }

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }

  @media (prefers-reduced-motion: reduce) {
    /* A slow turn still shows progress without motion that can make people unwell. */
    svg {
      animation: spin 2.4s linear infinite !important;
    }
  }
</style>
