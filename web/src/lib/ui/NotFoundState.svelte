<script lang="ts">
  // One page for missing and for inaccessible things, so it never reveals that something exists
  // (UI-036, architecture §8).
  import SearchX from '@lucide/svelte/icons/search-x';

  import * as states from '#lib/i18n/messages/states.ts';

  import Button from './Button.svelte';
  import EmptyState from './EmptyState.svelte';

  let { home = '/' }: { home?: string } = $props();
</script>

<EmptyState
  title={states.notFoundTitle()}
  description={states.notFoundText()}
  icon={SearchX}
  level={1}
>
  {#snippet actions()}
    <Button href={home} variant="primary">{states.toHomePage()}</Button>
  {/snippet}
</EmptyState>
