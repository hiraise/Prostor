<script lang="ts">
  // Asking before taking people's access away (PRJ-050, PRJ-051, PRJ-052): what is lost and a
  // destructive button. A refusal stays inside the dialog, naming missing permissions.
  import * as common from '#lib/i18n/messages/common.ts';
  import Button from '#lib/ui/Button.svelte';
  import Dialog from '#lib/ui/Dialog.svelte';

  import { problemOf, type Problem } from './people.ts';
  import ProblemNotice from './ProblemNotice.svelte';

  interface Props {
    open?: boolean;
    title: string;
    text: string;
    /** The destructive button's text. */
    action: string;
    /** Makes the change; throws when it is refused. */
    run: () => Promise<unknown>;
    /** After the change, while the dialog closes. */
    ondone?: () => unknown;
  }

  let { open = $bindable(false), title, text, action, run, ondone }: Props = $props();

  let busy = $state(false);
  let problem = $state<Problem | null>(null);

  $effect(() => {
    if (open) problem = null;
  });

  async function confirm(): Promise<void> {
    busy = true;
    problem = null;
    try {
      await run();
      open = false;
      await ondone?.();
    } catch (error) {
      problem = problemOf(error);
    } finally {
      busy = false;
    }
  }
</script>

<Dialog bind:open {title} size="sm" dismissible={!busy}>
  <div class="body">
    {#if problem}<ProblemNotice {problem} />{/if}
    <p>{text}</p>
  </div>
  {#snippet footer()}
    <Button
      disabled={busy}
      onclick={() => {
        open = false;
      }}
    >
      {common.cancel()}
    </Button>
    <Button variant="danger" loading={busy} onclick={confirm}>{action}</Button>
  {/snippet}
</Dialog>

<style>
  .body {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }
</style>
