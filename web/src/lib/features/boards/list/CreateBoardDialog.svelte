<script lang="ts">
  // The new-board dialog (BRD-003), opened by `openNewBoard` from the «Создать» menu, the boards
  // list and a project's boards.
  import type { Board } from '#lib/api/schema.gen.ts';
  import Dialog from '#lib/ui/Dialog.svelte';
  import { toasts } from '#lib/ui/toast.svelte.ts';

  import * as m from '../messages.ts';
  import CreateBoardForm from './CreateBoardForm.svelte';
  import { newBoardDialog, openCreated } from './open.svelte.ts';

  let busy = $state(false);

  async function created(board: Board): Promise<void> {
    newBoardDialog.open = false;
    toasts.show({ tone: 'success', message: m.boardCreated({ name: board.name }) });
    await openCreated(board.id);
  }
</script>

<Dialog bind:open={newBoardDialog.open} title={m.newBoard()} size="lg" dismissible={!busy}>
  {#if newBoardDialog.open}
    <CreateBoardForm
      project={newBoardDialog.project}
      bind:busy
      oncreated={created}
      oncancel={() => {
        newBoardDialog.open = false;
      }}
    />
  {/if}
</Dialog>
