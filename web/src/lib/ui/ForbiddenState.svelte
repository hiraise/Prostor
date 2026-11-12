<script lang="ts">
  // The page for administration areas a signed-in user may not open (UI-036). Objects a user may
  // not see answer "not found" instead (NotFoundState).
  import Lock from '@lucide/svelte/icons/lock';

  import * as states from '#lib/i18n/messages/states.ts';

  import Button from './Button.svelte';
  import EmptyState from './EmptyState.svelte';

  let { home = '/' }: { home?: string } = $props();
</script>

<EmptyState
  title={states.forbiddenTitle()}
  description={states.forbiddenText()}
  icon={Lock}
  level={1}
>
  {#snippet actions()}
    <Button href={home} variant="primary">{states.toHomePage()}</Button>
  {/snippet}
</EmptyState>
