<script lang="ts">
  // The activity of the issue (HIST-001, HIST-008…HIST-011): comments and change records in the
  // person's order, the comment box at the end of that order (`box`), filters as toggle buttons,
  // «Показать более ранние» / «Показать более новые» at the edges of what is loaded. Filters and
  // order are personal settings (UI-018).
  import ArrowDownUp from '@lucide/svelte/icons/arrow-down-up';
  import type { Snippet } from 'svelte';

  import type { Issue, Upload } from '#lib/api/schema.gen.ts';
  import Button from '#lib/ui/Button.svelte';
  import Notice from '#lib/ui/Notice.svelte';
  import { showError } from '#lib/ui/toast.svelte.ts';

  import * as m from '../messages.ts';
  import type { ActivityStream } from './activity.svelte.ts';
  import ChangeRecord from './ChangeRecord.svelte';
  import CommentCard from '../comments/CommentCard.svelte';

  interface Props {
    issue: Issue;
    stream: ActivityStream;
    /** The comment box. */
    box?: Snippet;
    /** Sends files dropped on a comment; the app's upload by default (tests give their own). */
    upload?: (file: File) => Promise<Upload>;
  }

  let { issue, stream, box, upload }: Props = $props();

  const id = $props.id();
  const oldestFirst = $derived(stream.order === 'oldest_first');
  /** Marks the newest end: live items are appended only while it is on screen (ISS-048). */
  let marker = $state<HTMLElement>();

  $effect(() => {
    if (!marker || typeof IntersectionObserver === 'undefined') return;
    const observer = new IntersectionObserver((entries) => {
      const last = entries.at(-1);
      if (last) stream.atEnd = last.isIntersecting;
    });
    observer.observe(marker);
    return () => {
      observer.disconnect();
    };
  });

  function run(action: Promise<void>): void {
    action.catch((error: unknown) => {
      showError(error);
    });
  }
</script>

{#snippet edge(direction: 'older' | 'newer')}
  {#if (direction === 'older' ? stream.older : stream.newer) !== null && !stream.hidden}
    <div class="edge">
      <Button
        variant="ghost"
        size="sm"
        loading={stream.loading === direction}
        onclick={() => run(stream.more(direction))}
      >
        {direction === 'older' ? m.showOlder() : m.showNewer()}
      </Button>
    </div>
  {/if}
{/snippet}

{#snippet end()}
  <div class="end" bind:this={marker}></div>
  <div class="incoming" aria-live="polite">
    {#if stream.incoming.length > 0 && !stream.hidden}
      <Notice>
        <span class="line">
          {m.newEntries({ count: stream.incoming.length })}
          <Button size="sm" onclick={() => stream.showIncoming()}>{m.showNew()}</Button>
        </span>
      </Notice>
    {/if}
  </div>
{/snippet}

<section class="activity" aria-labelledby="{id}-title" aria-busy={stream.loading === 'all'}>
  <header class="top">
    <h2 id="{id}-title">{m.activity()}</h2>
    <div class="controls">
      <div class="filters" role="group" aria-label={m.activityShown()}>
        <Button
          size="sm"
          aria-pressed={stream.showComments}
          onclick={() => run(stream.setFilter('comments', !stream.showComments))}
        >
          {m.activityComments()}
        </Button>
        <Button
          size="sm"
          aria-pressed={stream.showChanges}
          onclick={() => run(stream.setFilter('changes', !stream.showChanges))}
        >
          {m.activityChanges()}
        </Button>
      </div>
      <Button
        size="sm"
        variant="ghost"
        icon={ArrowDownUp}
        onclick={() => run(stream.setOrder(oldestFirst ? 'newest_first' : 'oldest_first'))}
      >
        {oldestFirst ? m.orderOldest() : m.orderNewest()}
      </Button>
    </div>
  </header>

  {#if !oldestFirst}{@render box?.()}{@render end()}{/if}
  {@render edge(oldestFirst ? 'older' : 'newer')}

  {#if stream.hidden}
    <p class="muted">{m.activityHidden()}</p>
  {:else if stream.rows.length === 0 && stream.loading !== 'all'}
    <p class="muted">{m.activityEmpty()}</p>
  {:else}
    <ol class="rows">
      {#each stream.rows as row (row.id)}
        <li>
          {#if row.type === 'comment'}
            <CommentCard
              comment={row.comment}
              changes={row.changes}
              {issue}
              focused={stream.focus === row.comment.id}
              {...upload ? { upload } : {}}
            />
          {:else}
            <div class="record"><ChangeRecord change={row.change} {issue} /></div>
          {/if}
        </li>
      {/each}
    </ol>
  {/if}

  {@render edge(oldestFirst ? 'newer' : 'older')}
  {#if oldestFirst}{@render end()}{@render box?.()}{/if}
</section>

<style>
  .activity {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }

  .top {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2);
  }

  h2 {
    font-size: var(--text-lg);
  }

  .controls,
  .filters {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-1);
  }

  .controls {
    gap: var(--space-3);
  }

  .rows {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .record {
    padding-inline: var(--space-3);
    font-size: var(--text-sm);
  }

  .edge {
    display: flex;
    justify-content: center;
  }

  .end {
    height: 1px;
    margin-block-end: -1px;
  }

  .incoming:empty {
    display: none;
  }

  .line {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-2);
  }

  .muted {
    color: var(--color-text-muted);
  }
</style>
