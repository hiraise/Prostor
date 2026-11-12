<script lang="ts">
  // Creating a project (PRJ-017); the new project's overview opens right after.
  import { goto } from '$app/navigation';

  import type { Project } from '#lib/api/schema.gen.ts';
  import { CreateProjectPage, projectHref } from '#lib/features/projects/index.ts';
  import { homeHref } from '#lib/shell/sections.ts';

  function open(project: Project): Promise<void> {
    // projectHref resolves the path.
    return goto(projectHref.overview(project));
  }
</script>

<CreateProjectPage oncreated={open} home={homeHref()} />
