<script lang="ts">
  // Someone saved the summary or description while the person was editing (ISS-051): the word
  // diff from the saved text to the person's, and three ways on — save the person's version over
  // the new one, keep editing with the saved text shown above the editor, or drop the edits.
  import Button from '#lib/ui/Button.svelte';
  import Dialog from '#lib/ui/Dialog.svelte';

  import type { Texts } from '../actions.ts';
  import * as m from '../messages.ts';
  import DiffView from './DiffView.svelte';

  interface Props {
    open?: boolean;
    /** The texts as saved meanwhile. */
    theirs: Texts;
    /** The person's texts. */
    mine: Texts;
    busy?: boolean;
    onkeep: () => unknown;
    oncontinue: () => unknown;
    ondiscard: () => unknown;
  }

  let {
    open = $bindable(false),
    theirs,
    mine,
    busy = false,
    onkeep,
    oncontinue,
    ondiscard,
  }: Props = $props();
</script>

<Dialog
  bind:open
  title={m.conflictTitle()}
  description={m.conflictText()}
  size="lg"
  dismissible={!busy}
>
  <div class="body">
    {#if theirs.summary !== mine.summary}
      <DiffView label={m.summary()} before={theirs.summary} after={mine.summary} />
    {/if}
    {#if theirs.description !== mine.description}
      <DiffView label={m.description()} before={theirs.description} after={mine.description} />
    {/if}
  </div>
  {#snippet footer()}
    <Button variant="ghost" disabled={busy} onclick={ondiscard}>{m.dropMine()}</Button>
    <Button disabled={busy} onclick={oncontinue}>{m.editFurther()}</Button>
    <Button variant="primary" loading={busy} onclick={onkeep}>{m.keepMine()}</Button>
  {/snippet}
</Dialog>

<style>
  .body {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }
</style>
