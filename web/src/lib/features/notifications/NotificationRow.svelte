<script lang="ts">
  // One row of the inbox (NTF-049, NTF-053): who did what and when, then the lines the server
  // rendered for this reader. The issue id and comment excerpts link into the issue page (the
  // comment one right at its comment); a `system` row reports a job instead.
  import Mail from '@lucide/svelte/icons/mail';
  import MailOpen from '@lucide/svelte/icons/mail-open';

  import type { InboxNotification } from '#lib/api/schema.gen.ts';
  import { PersonName } from '#lib/features/access/index.ts';
  import { issueHref } from '#lib/features/issues/index.ts';
  import Time from '#lib/time/Time.svelte';
  import IconButton from '#lib/ui/IconButton.svelte';

  import * as m from './messages.ts';

  interface Props {
    row: InboxNotification;
    /** Closes the popup when a link into an issue is followed. */
    onnavigate?: () => void;
    /** Marks the row read or unread again (NTF-050). */
    ontoggleread: (row: InboxNotification) => void;
  }

  let { row, onnavigate, ontoggleread }: Props = $props();

  const change = $derived('issue' in row.content ? row.content : null);
  const report = $derived('job' in row.content ? row.content : null);
  const kind = $derived(m.kindWords[row.kind]?.() ?? row.kind);
  const commentHrefOf = (comment: number): string =>
    `${issueHref(change?.issue.readable_id ?? '')}?comment=${String(comment)}`;
</script>

<div class={['row', { unread: !row.read }]}>
  {#if !row.read}<span class="dot" aria-hidden="true"></span>{/if}
  <div class="body">
    <p class="meta">
      <PersonName person={row.author} size="sm" />
      <span class="kind">{kind}</span>
      <Time value={row.created_at} kind="relative" />
      {#if !row.read}<span class="sr-only"> ({m.unreadRow()})</span>{/if}
    </p>
    {#if change}
      <!-- issueHref resolves the address. -->
      <!-- eslint-disable svelte/no-navigation-without-resolve -->
      <a
        class="issue"
        href={issueHref(change.issue.readable_id)}
        title={change.issue.summary}
        onclick={onnavigate}
      >
        {change.issue.readable_id}
      </a>
      <!-- eslint-enable svelte/no-navigation-without-resolve -->
      <ul class="lines">
        {#each change.lines as line, index (index)}
          {#if line.t === 'text'}
            <li>{line.text}</li>
          {:else if line.t === 'field'}
            <li><span class="field">{line.name}: {line.old} → {line.new}</span></li>
          {:else}
            <li>
              <!-- issueHref resolves the address. -->
              <!-- eslint-disable-next-line svelte/no-navigation-without-resolve -->
              <a class="comment" href={commentHrefOf(line.id)} onclick={onnavigate}>
                {line.excerpt || m.kindComment()}
              </a>
            </li>
          {/if}
        {/each}
      </ul>
    {:else if report}
      <p class="lines report">
        {report.job} — {m.jobStates[report.state]?.() ?? report.state}{#if report.message}:
          {report.message}{/if}
      </p>
    {/if}
  </div>
  <span class="toggle">
    <IconButton
      icon={row.read ? Mail : MailOpen}
      label={row.read ? m.markUnread() : m.markRead()}
      size="sm"
      onclick={() => {
        ontoggleread(row);
      }}
    />
  </span>
</div>

<style>
  .row {
    display: flex;
    gap: var(--space-2);
    padding: var(--space-2) var(--space-3);
  }

  .dot {
    flex: none;
    width: var(--space-2);
    height: var(--space-2);
    margin-top: var(--space-1_5);
    border-radius: var(--radius-full);
    background: var(--color-brand);
  }

  .body {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    min-width: 0;
    flex: 1;
  }

  .meta {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-2);
    margin: 0;
    color: var(--color-text-muted);
    font-size: var(--text-sm);
  }

  .kind {
    color: var(--color-text);
  }

  .issue {
    font-family: var(--font-mono);
    font-size: var(--text-sm);
    font-weight: var(--weight-medium);
    color: var(--color-link);
    text-decoration: none;
  }

  .issue:hover {
    text-decoration: underline;
  }

  .lines {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    margin: 0;
    padding: 0;
    list-style: none;
    font-size: var(--text-sm);
    overflow-wrap: anywhere;
  }

  .lines.report {
    color: var(--color-text);
  }

  .field {
    color: var(--color-text-muted);
  }

  .comment {
    color: var(--color-text);
    text-decoration: none;
  }

  .comment:hover {
    text-decoration: underline;
  }

  .toggle {
    display: inline-flex;
    align-self: flex-start;
    opacity: 0;
  }

  .row:focus-within .toggle,
  .row:hover .toggle {
    opacity: 1;
  }

  @media (pointer: coarse) {
    /* Nothing exists only on hover (D-019): the read toggle stays visible for touch. */
    .toggle {
      opacity: 1;
    }
  }
</style>
