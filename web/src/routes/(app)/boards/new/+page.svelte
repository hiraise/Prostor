<script lang="ts">
  // A new board as a page (BRD-003): phones open it from «Создать»; `?project=ID` starts with that
  // project.
  import { untrack } from 'svelte';
  import { goto } from '$app/navigation';
  import { page } from '$app/state';

  import { CreateBoardPage, boardHref } from '#lib/features/boards/index.ts';
  import { projectRow } from '#lib/features/projects/index.ts';

  const id = untrack(() => Number(page.url.searchParams.get('project')) || undefined);
  const project = id === undefined ? undefined : projectRow(id);

  function cancel(): void {
    if (history.length > 1) history.back();
    else void goto(boardHref.entry());
  }
</script>

<CreateBoardPage {project} oncancel={cancel} />
