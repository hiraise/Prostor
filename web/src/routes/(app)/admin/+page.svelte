<script lang="ts">
  import { brand } from '#lib/i18n/messages/brand.ts';
  import { adminMessages as m } from '#lib/features/admin/index.ts';
  import ForbiddenState from '#lib/ui/ForbiddenState.svelte';
</script>

<svelte:head><title>{m.administration()} · {brand()}</title></svelte:head>

<ForbiddenState />
