<script lang="ts">
  // The activity of the issue (HIST-001, HIST-008…HIST-011): the comment box (`box`) at the top of
  // the zone, then the display settings on their divider — the tab strip «Все», «Комментарии»,
  // «Изменения» (HIST-008) with the order button at the strip's end — and the comments and change
  // records in the person's order below it. «Показать более ранние» / «Показать более новые» wait
  // at the edges of what is loaded. The tab and the order are personal settings (UI-018).
  import ArrowDownUp from '@lucide/svelte/icons/arrow-down-up';
  import type { Snippet } from 'svelte';

  import type { Issue, Upload } from '#lib/api/schema.gen.ts';
  import Button from '#lib/ui/Button.svelte';
  import Notice from '#lib/ui/Notice.svelte';
  import { showError } from '#lib/ui/toast.svelte.ts';

  import * as m from '../messages.ts';
  import type { ActivityStream, ActivityView } from './activity.svelte.ts';
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
  const tabs: { value: ActivityView; label: string }[] = [
    { value: 'all', label: m.activityAll() },
    { value: 'comments', label: m.activityComments() },
    { value: 'changes', label: m.activityChanges() },
  ];
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

  /** Arrows walk the tabs and open the one they land on (WAI-ARIA tabs pattern). */
  function tabKeys(event: KeyboardEvent): void {
    const current = tabs.findIndex((tab) => tab.value === stream.view);
    const last = tabs.length - 1;
    let next: number;
    if (event.key === 'ArrowRight') next = current === last ? 0 : current + 1;
    else if (event.key === 'ArrowLeft') next = current === 0 ? last : current - 1;
    else if (event.key === 'Home') next = 0;
    else if (event.key === 'End') next = last;
    else return;
    event.preventDefault();
    const target = tabs[next];
    if (!target) return;
    document.getElementById(`${id}-tab-${target.value}`)?.focus();
    run(stream.setView(target.value));
  }
</script>

{#snippet edge(direction: 'older' | 'newer')}
  {#if (direction === 'older' ? stream.older : stream.newer) !== null}
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
    {#if stream.incoming.length > 0}
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
  <h2 class="sr-only" id="{id}-title">{m.activity()}</h2>
  {@render box?.()}

  <div class="tabstrip">
    <div class="tabs" role="tablist" aria-label={m.activityShown()}>
      {#each tabs as tab (tab.value)}
        <button
          type="button"
          class="tab"
          role="tab"
          id="{id}-tab-{tab.value}"
          aria-selected={stream.view === tab.value}
          aria-controls="{id}-panel"
          tabindex={stream.view === tab.value ? 0 : -1}
          onclick={() => run(stream.setView(tab.value))}
          onkeydown={tabKeys}
        >
          {tab.label}
        </button>
      {/each}
    </div>
    <div class="order">
      <Button
        size="sm"
        variant="ghost"
        icon={ArrowDownUp}
        onclick={() => run(stream.setOrder(oldestFirst ? 'newest_first' : 'oldest_first'))}
      >
        {oldestFirst ? m.orderOldest() : m.orderNewest()}
      </Button>
    </div>
  </div>

  <div class="panel" id="{id}-panel" role="tabpanel" aria-labelledby="{id}-tab-{stream.view}">
    {#if !oldestFirst}{@render end()}{/if}
    {@render edge(oldestFirst ? 'older' : 'newer')}

    {#if stream.rows.length === 0 && stream.loading !== 'all'}
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
    {#if oldestFirst}{@render end()}{/if}
  </div>
</section>

<style>
  .activity {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }

  /* The divider the tabs sit on is the zone's top edge. */
  .tabstrip {
    display: flex;
    align-items: flex-end;
    gap: var(--space-1);
    border-block-end: 1px solid var(--color-border);
  }

  .tabs {
    display: flex;
    align-items: flex-end;
  }

  .tab {
    padding: var(--space-1) var(--space-2) var(--space-1_5);
    margin-block-end: -1px;
    border: 0;
    border-block-end: 2px solid transparent;
    background: transparent;
    color: var(--color-text-muted);
    font: inherit;
    cursor: pointer;
  }

  .tab:hover {
    color: var(--color-text);
  }

  .tab[aria-selected='true'] {
    border-block-end-color: var(--color-brand);
    color: var(--color-text);
    font-weight: var(--weight-medium);
  }

  .order {
    margin-inline-start: auto;
    padding-block-end: var(--space-1);
  }

  .panel {
    display: flex;
    flex-direction: column;
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

  @media (pointer: coarse) {
    .tab {
      min-height: 2.75rem;
    }
  }
</style>
