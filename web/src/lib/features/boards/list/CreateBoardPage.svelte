<script lang="ts">
  import { brand } from '#lib/i18n/messages/brand.ts';
  // The new-board form as a page (BRD-003): phones open it instead of the dialog.
  import type { Board } from '#lib/api/schema.gen.ts';
  import type { ProjectChoice } from '#lib/features/projects/index.ts';
  import PageHeader from '#lib/ui/PageHeader.svelte';
  import { toasts } from '#lib/ui/toast.svelte.ts';

  import * as m from '../messages.ts';
  import CreateBoardForm from './CreateBoardForm.svelte';
  import { openCreated } from './open.svelte.ts';

  interface Props {
    project?: ProjectChoice;
    oncancel: () => unknown;
  }

  let { project, oncancel }: Props = $props();

  async function created(board: Board): Promise<void> {
    toasts.show({ tone: 'success', message: m.boardCreated({ name: board.name }) });
    await openCreated(board.id);
  }
</script>

<svelte:head><title>{m.newBoard()} · {brand()}</title></svelte:head>

<PageHeader title={m.newBoard()} />
<div class="page">
  <CreateBoardForm {project} oncreated={created} {oncancel} />
</div>

<style>
  .page {
    max-width: 40rem;
    padding: var(--space-2) var(--space-6) var(--space-12);
  }

  @media (width < 48rem) {
    .page {
      padding: var(--space-2) var(--space-4) var(--space-8);
    }
  }
</style>
