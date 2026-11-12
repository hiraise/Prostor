<script lang="ts">
  // A project's logo (PRJ-007): the picture or the generated letters, and for people who may
  // change the project, uploading a picture and going back to the generated logo. Problems are
  // said under the buttons.
  import ImageUp from '@lucide/svelte/icons/image-up';

  import { ApiError } from '#lib/api/client.ts';
  import type { Project } from '#lib/api/schema.gen.ts';
  import { PICTURE_TYPES } from '#lib/api/upload.ts';
  import Button from '#lib/ui/Button.svelte';
  import FileButton from '#lib/ui/FileButton.svelte';
  import Notice from '#lib/ui/Notice.svelte';
  import { announce } from '#lib/ui/announce.svelte.ts';
  import { describeError, toasts } from '#lib/ui/toast.svelte.ts';

  import ProjectLogo from '../ProjectLogo.svelte';
  import { resetProjectLogo, uploadProjectLogo } from '../projects.ts';
  import * as m from './messages.ts';

  interface Props {
    project: Project;
    /** The person may change the project, and it is not archived. */
    editable: boolean;
  }

  let { project, editable }: Props = $props();

  const id = $props.id();
  let busy = $state<'upload' | 'reset' | null>(null);
  let problem = $state<string | null>(null);

  async function run(kind: 'upload' | 'reset', action: () => Promise<unknown>) {
    busy = kind;
    problem = null;
    try {
      await action();
      toasts.show({ tone: 'success', message: kind === 'reset' ? m.logoReset() : m.logoSaved() });
    } catch (error) {
      problem =
        error instanceof ApiError && error.code === 'invalid_request'
          ? m.notAPicture()
          : describeError(error).message;
      announce(problem, 'assertive');
    } finally {
      busy = null;
    }
  }

  function chosen(files: File[]): void {
    const [file] = files;
    if (file) void run('upload', () => uploadProjectLogo(project, file));
  }
</script>

<section class="logo-form" aria-labelledby="{id}-title">
  <h2 id="{id}-title" class="title">{m.logo()}</h2>
  <div class="row">
    <ProjectLogo
      projectKey={project.key}
      color={project.logo_color}
      src={project.logo_url}
      size="xl"
    />
    {#if editable}
      <div class="side">
        <div class="buttons">
          <FileButton
            accept={PICTURE_TYPES}
            icon={ImageUp}
            loading={busy === 'upload'}
            disabled={busy === 'reset'}
            onfiles={chosen}
          >
            {m.uploadLogo()}
          </FileButton>
          {#if project.logo_url}
            <Button
              variant="ghost"
              loading={busy === 'reset'}
              disabled={busy === 'upload'}
              onclick={() => run('reset', () => resetProjectLogo(project))}
            >
              {m.resetLogo()}
            </Button>
          {/if}
        </div>
        <p class="hint">{m.logoHint()}</p>
      </div>
    {/if}
  </div>
  {#if problem}
    <Notice tone="danger" ondismiss={() => (problem = null)}>{problem}</Notice>
  {/if}
</section>

<style>
  .logo-form {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }

  .title {
    font-size: var(--text-md);
    font-weight: var(--weight-medium);
  }

  .row {
    display: flex;
    align-items: flex-start;
    gap: var(--space-4);
  }

  .side {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    min-width: 0;
  }

  .buttons {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
  }

  .hint {
    color: var(--color-text-muted);
    font-size: var(--text-sm);
  }
</style>
