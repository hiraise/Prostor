<script lang="ts">
  // A new issue (ISS-005, ISS-010): `?draft=ID` continues a draft, `?project=KEY` starts in that
  // project, `?parent=ID` as its subtask, `?link=<type>:<side>:<issue>` linked so (ISS-009). The page keeps its draft while the address follows it.
  import { page } from '$app/state';
  import { untrack } from 'svelte';

  import { NewIssuePage, parseLinkParam } from '#lib/features/issues/index.ts';

  const draft = untrack(() => Number(page.url.searchParams.get('draft')) || undefined);
  const project = untrack(() => page.url.searchParams.get('project') ?? undefined);
  const parent = untrack(() => Number(page.url.searchParams.get('parent')) || undefined);
  const link = untrack(() => parseLinkParam(page.url.searchParams.get('link')) ?? undefined);
</script>

<NewIssuePage {draft} {project} {parent} {link} />
