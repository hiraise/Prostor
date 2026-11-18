<script lang="ts">
  // The issue's visibility (ISS-041…ISS-043) as the lock beside «Изменить»: the button shows the
  // state (a closed lock when the issue is limited to an audience), and the popover under it
  // changes the audience for those who may — saving at once, rolling back with the reason — or
  // names the audience for those who may not. The reporter is always included (ISS-041).
  import Lock from '@lucide/svelte/icons/lock';
  import LockOpen from '@lucide/svelte/icons/lock-open';

  import type { Audience, Issue } from '#lib/api/schema.gen.ts';
  import { saveTexts } from '../actions.ts';
  import AudiencePicker from '../audience/AudiencePicker.svelte';
  import { audienceText, isRestricted } from '../audience/audience.ts';
  import * as m from '../messages.ts';
  import Button from '#lib/ui/Button.svelte';
  import IconButton from '#lib/ui/IconButton.svelte';
  import Popover from '#lib/ui/Popover.svelte';
  import { refusalText } from '#lib/features/fields/index.ts';

  let { issue }: { issue: Issue } = $props();

  const restricted = $derived(isRestricted(issue.audience));
  const editable = $derived(issue.can.update);

  /** The draft inside the popover; fresh from the issue each time it opens. */
  let audience = $state<Audience>({ users: [], groups: [] });
  let limited = $state(false);
  let open = $state(false);
  let busy = $state(false);
  let problem = $state<string | null>(null);

  $effect(() => {
    if (!open) return;
    audience = {
      users: [...issue.audience.users],
      groups: [...issue.audience.groups],
    };
    limited = restricted;
    problem = null;
  });

  async function save(close: () => void): Promise<void> {
    if (busy) return;
    if (limited && audience.users.length + audience.groups.length === 0) {
      problem = m.audienceEmpty();
      return;
    }
    busy = true;
    problem = null;
    try {
      await saveTexts(
        issue.id,
        { audience: limited ? $state.snapshot(audience) : { users: [], groups: [] } },
        issue.text_version,
      );
      close();
    } catch (error) {
      problem = refusalText(error);
    } finally {
      busy = false;
    }
  }
</script>

<Popover label={m.visibility()} bind:open>
  {#snippet trigger(props)}
    <IconButton
      icon={restricted ? Lock : LockOpen}
      label={m.visibility()}
      pressed={restricted}
      {...props}
    />
  {/snippet}
  {#if editable}
    <div class="picker">
      <AudiencePicker
        project={issue.project.id}
        reporter={issue.reporter}
        offer={issue.audience}
        bind:value={audience}
        bind:restricted={limited}
        error={problem}
      />
      <div class="row">
        <Button size="sm" loading={busy} onclick={() => void save(() => (open = false))}
          >{m.saveVisibility()}</Button
        >
      </div>
    </div>
  {:else}
    <p class="who">{restricted ? audienceText(issue.audience) : m.visibleToAll()}</p>
  {/if}
</Popover>

<style>
  .picker {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    min-width: 18rem;
  }

  .row {
    display: flex;
    justify-content: flex-end;
  }

  .who {
    max-width: 22rem;
    margin: 0;
    color: var(--color-text-muted);
    font-size: var(--text-sm);
  }
</style>
