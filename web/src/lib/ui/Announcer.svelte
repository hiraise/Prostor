<script lang="ts">
  // The live regions behind `announce()`; rendered once by the root layout. They must exist before
  // a message arrives, otherwise screen readers miss it.
  import { announcement } from './announce.svelte.ts';
</script>

<div class="sr-only" aria-live="polite" aria-atomic="true" data-testid="announcer-polite">
  {announcement('polite')}
</div>
<div class="sr-only" aria-live="assertive" aria-atomic="true" data-testid="announcer-assertive">
  {announcement('assertive')}
</div>
