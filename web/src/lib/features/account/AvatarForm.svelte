<script lang="ts">
  // One's picture (ACC-017): the picture or the initials, the name and username next to it, and
  // the ways to change it — upload a photo, take the one Gravatar has for the email (when the
  // server may ask Gravatar), remove it. Problems are said under the buttons.
  import ImageUp from '@lucide/svelte/icons/image-up';

  import { ApiError } from '#lib/api/client.ts';
  import type { Account } from '#lib/api/schema.gen.ts';
  import { PICTURE_TYPES } from '#lib/api/upload.ts';
  import Avatar from '#lib/ui/Avatar.svelte';
  import Button from '#lib/ui/Button.svelte';
  import FileButton from '#lib/ui/FileButton.svelte';
  import Notice from '#lib/ui/Notice.svelte';
  import { announce } from '#lib/ui/announce.svelte.ts';
  import { describeError, toasts } from '#lib/ui/toast.svelte.ts';

  import { importGravatar, removeAvatar, uploadAvatar } from './avatar.ts';
  import * as m from './messages.ts';

  interface Props {
    account: Account;
    /** The server may take pictures from Gravatar. */
    gravatar: boolean;
  }

  let { account, gravatar }: Props = $props();

  let busy = $state<'upload' | 'gravatar' | 'remove' | null>(null);
  let problem = $state<string | null>(null);

  async function run(kind: 'upload' | 'gravatar' | 'remove', action: () => Promise<unknown>) {
    busy = kind;
    problem = null;
    try {
      await action();
      toasts.show({
        tone: 'success',
        message: kind === 'remove' ? m.photoRemoved() : m.photoSaved(),
      });
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
    if (file) void run('upload', () => uploadAvatar(account, file));
  }
</script>

<div class="avatar-form">
  <div class="row">
    <Avatar
      name={account.full_name}
      color={account.avatar_color}
      src={account.avatar_url}
      size="xl"
    />
    <div class="side">
      <p class="who">
        <span class="name">{account.full_name}</span>
        <span class="username">{account.username}</span>
      </p>
      <div class="buttons" role="group" aria-label={m.photo()}>
        <FileButton
          accept={PICTURE_TYPES}
          icon={ImageUp}
          size="sm"
          loading={busy === 'upload'}
          disabled={busy !== null && busy !== 'upload'}
          onfiles={chosen}
        >
          {m.uploadPhoto()}
        </FileButton>
        {#if gravatar}
          <Button
            size="sm"
            variant="ghost"
            loading={busy === 'gravatar'}
            disabled={busy !== null && busy !== 'gravatar'}
            onclick={() => run('gravatar', () => importGravatar(account))}
          >
            {m.gravatarPhoto()}
          </Button>
        {/if}
        {#if account.avatar_url}
          <Button
            size="sm"
            variant="ghost"
            loading={busy === 'remove'}
            disabled={busy !== null && busy !== 'remove'}
            onclick={() => run('remove', () => removeAvatar(account))}
          >
            {m.removePhoto()}
          </Button>
        {/if}
      </div>
      <p class="hint">{m.photoHint()}</p>
    </div>
  </div>
  {#if problem}
    <Notice tone="danger" ondismiss={() => (problem = null)}>{problem}</Notice>
  {/if}
</div>

<style>
  .avatar-form {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    max-width: 28rem;
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

  .who {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: 0 var(--space-2);
    overflow-wrap: anywhere;
  }

  .name {
    font-size: var(--text-lg);
    font-weight: var(--weight-semibold);
  }

  .username {
    color: var(--color-text-muted);
    font-size: var(--text-sm);
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
