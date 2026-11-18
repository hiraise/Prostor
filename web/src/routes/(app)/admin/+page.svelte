<script lang="ts">
  import { adminMessages as m } from '#lib/features/admin/index.ts';
  import ForbiddenState from '#lib/ui/ForbiddenState.svelte';
</script>

<svelte:head><title>{m.administration()} · TaskMaster</title></svelte:head>

<ForbiddenState />
