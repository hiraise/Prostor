<script lang="ts">
  // A new parent lane (BRD-054): its title becomes the summary of a new issue in the first board
  // project, which joins the sprint as the last lane.
  import Button from '#lib/ui/Button.svelte';
  import Dialog from '#lib/ui/Dialog.svelte';
  import TextField from '#lib/ui/TextField.svelte';

  import type { BoardState } from '../board/board-state.svelte.ts';
  import * as m from '../messages.ts';

  interface Props {
    open?: boolean;
    model: BoardState;
  }

  let { open = $bindable(false), model }: Props = $props();

  const id = $props.id();
  let title = $state('');
  let busy = $state(false);

  $effect(() => {
    if (open) title = '';
  });

  async function submit(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    if (title.trim() === '' || busy) return;
    busy = true;
    const done = await model.createLane(title.trim());
    busy = false;
    if (done) open = false;
  }
</script>

<Dialog bind:open title={m.newLane()} size="sm" dismissible={!busy}>
  <form id="{id}-form" onsubmit={submit}>
    <TextField
      label={m.laneTitle()}
      hint={m.laneTitleHint()}
      bind:value={title}
      required
      maxlength={255}
      autofocus
    />
  </form>
  {#snippet footer()}
    <Button variant="ghost" disabled={busy} onclick={() => (open = false)}>{m.cancel()}</Button>
    <Button
      type="submit"
      form="{id}-form"
      variant="primary"
      loading={busy}
      disabled={title.trim() === ''}>{m.create()}</Button
    >
  {/snippet}
</Dialog>
