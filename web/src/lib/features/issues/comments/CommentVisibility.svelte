<script lang="ts">
  // Whom a comment is shown to (CNT-003): the issue's audience, or chosen groups and people inside
  // it. Folded behind «Видимость» until opened or restricted, so the box stays small on phones.
  import Lock from '@lucide/svelte/icons/lock';

  import type { Audience, Issue, UserRef } from '#lib/api/schema.gen.ts';
  import Button from '#lib/ui/Button.svelte';

  import AudiencePicker from '../audience/AudiencePicker.svelte';
  import { audienceText, isRestricted } from '../audience/audience.ts';
  import * as m from '../messages.ts';

  interface Props {
    issue: Issue;
    /** Who writes the comment: always sees it. */
    author: UserRef;
    value?: Audience;
    restricted?: boolean;
    error?: string | null;
  }

  let {
    issue,
    author,
    value = $bindable({ users: [], groups: [] }),
    restricted = $bindable(false),
    error = null,
  }: Props = $props();

  const id = $props.id();
  let open = $state(restricted);
  const summary = $derived(restricted ? audienceText(value) : null);
</script>

<div class="visibility">
  <Button
    size="sm"
    variant="ghost"
    icon={Lock}
    aria-expanded={open}
    aria-controls="{id}-picker"
    onclick={() => (open = !open)}
  >
    {m.commentVisibility()}{#if summary && !open}: {summary}{/if}
  </Button>
  <div id="{id}-picker" hidden={!open}>
    {#if open}
      <AudiencePicker
        project={issue.project.id}
        reporter={author}
        bind:value
        bind:restricted
        {error}
        offer={isRestricted(issue.audience) ? issue.audience : undefined}
        allLabel={m.audienceIssue()}
      />
    {/if}
  </div>
</div>

<style>
  .visibility {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: var(--space-2);
  }

  .visibility > div {
    align-self: stretch;
  }
</style>
