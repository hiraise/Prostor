<script lang="ts">
  // The links of an issue (LNK-010, LNK-013, LNK-014): tabs by link name with counts, «Все» first;
  // rows with the linked issue, a star toggle and, for those who may link, a remove button;
  // unresolved issues first in each group. The panel is limited in height until «Развернуть», folds
  // (a personal setting, UI-018), loads a group's further rows with «Ещё», and opens the linked
  // issues as a search in a new tab. The subtasks' group says how many are resolved.
  import { resolve } from '$app/paths';
  import ChevronDown from '@lucide/svelte/icons/chevron-down';
  import ChevronRight from '@lucide/svelte/icons/chevron-right';
  import ExternalLink from '@lucide/svelte/icons/external-link';
  import Link from '@lucide/svelte/icons/link';
  import Star from '@lucide/svelte/icons/star';
  import X from '@lucide/svelte/icons/x';
  import { untrack } from 'svelte';

  import type { Issue, LinkGroup } from '#lib/api/schema.gen.ts';
  import { preferences } from '#lib/session/index.ts';
  import { announce } from '#lib/ui/announce.svelte.ts';
  import Button from '#lib/ui/Button.svelte';
  import IconButton from '#lib/ui/IconButton.svelte';
  import { showError, toasts } from '#lib/ui/toast.svelte.ts';

  import { setStar } from '../actions.ts';
  import IssueRef from '../IssueRef.svelte';
  import { issueRow } from '../issues.ts';
  import * as m from '../messages.ts';
  import { listQuery, loadLinked, type IssueLinksState } from './links.svelte.ts';

  interface Props {
    issue: Issue;
    links: IssueLinksState;
    /** «Связать» in the panel's head (the new-issue form; the page has it in its header). */
    onadd?: () => void;
  }

  let { issue, links, onadd }: Props = $props();

  const id = $props.id();
  /** The chosen tab: a group's `type:side`, or `null` for «Все». */
  let tab = $state<string | null>(null);
  let expanded = $state(false);
  let tabList: HTMLDivElement | undefined = $state();

  const collapsed = $derived(preferences.value('links_collapsed'));
  const groups = $derived(links.groups.filter((group) => group.total > 0));
  const shown = $derived(
    tab === null ? groups : groups.filter((group) => key(group) === tab).slice(0, 1),
  );
  const chosen = $derived(tab === null ? undefined : shown[0]);
  const query = $derived(issue.readable_id ? listQuery(issue.readable_id, chosen) : null);

  // A tab whose group is gone goes back to «Все».
  $effect(() => {
    if (tab !== null && !groups.some((group) => key(group) === tab)) tab = null;
  });

  // Rows of the linked issues, for their stars.
  $effect(() => {
    const ids = groups.flatMap((group) => group.items.map((row) => row.issue.id));
    // Only the groups decide; rows changing in the store do not ask again.
    untrack(() => loadLinked(ids)).catch(() => undefined);
  });

  function key(group: LinkGroup): string {
    return `${String(group.link_type.id)}:${group.side}`;
  }

  function toggle(): void {
    preferences.set({ links_collapsed: collapsed ? null : true });
  }

  function choose(next: string | null): void {
    tab = next;
  }

  /** Arrows move between the tabs (UI-032). */
  function tabKeys(event: KeyboardEvent): void {
    if (!tabList) return;
    const all = [...tabList.querySelectorAll<HTMLButtonElement>('[role="tab"]')];
    const at = all.indexOf(document.activeElement as HTMLButtonElement);
    let next: number;
    if (event.key === 'ArrowRight' || event.key === 'ArrowDown') next = at + 1;
    else if (event.key === 'ArrowLeft' || event.key === 'ArrowUp') next = at - 1;
    else if (event.key === 'Home') next = 0;
    else if (event.key === 'End') next = all.length - 1;
    else return;
    event.preventDefault();
    const target = all[(next + all.length) % all.length];
    target?.focus();
    target?.click();
  }

  function remove(link: number): void {
    links.remove(link).then(
      () => {
        toasts.show({ tone: 'success', message: m.linkRemoved() });
        announce(m.linkRemoved());
      },
      (error: unknown) => showError(error),
    );
  }

  function progress(group: LinkGroup): string | null {
    if (group.link_type.builtin !== 'subtask' || group.side !== 'source') return null;
    const count = issue.subtasks;
    if (count.total === 0) return null;
    return m.subtaskProgress({ resolved: count.total - count.unresolved, total: count.total });
  }
</script>

<section class="links" aria-labelledby="{id}-title">
  <div class="head">
    <h2 id="{id}-title">
      <button
        type="button"
        class="toggle"
        aria-expanded={!collapsed}
        aria-controls="{id}-body"
        onclick={toggle}
      >
        {#if collapsed}
          <ChevronRight aria-hidden="true" size={16} />
        {:else}
          <ChevronDown aria-hidden="true" size={16} />
        {/if}
        {m.links()}
        {#if links.total > 0}<span class="count">{links.total}</span>{/if}
      </button>
    </h2>
    {#if !collapsed && groups.length > 0}
      {#if query}
        <!-- A search of the issue list in a new tab; the query is built from the link's name. -->
        <!-- eslint-disable svelte/no-navigation-without-resolve -->
        <a
          class="list-link"
          href="{resolve('/(app)/issues')}?q={encodeURIComponent(query)}"
          target="_blank"
          rel="noopener"
        >
          <ExternalLink aria-hidden="true" size={14} />{m.openAsList()}
        </a>
        <!-- eslint-enable svelte/no-navigation-without-resolve -->
      {/if}
      <Button size="sm" variant="ghost" onclick={() => (expanded = !expanded)}>
        {expanded ? m.collapseLinks() : m.expandLinks()}
      </Button>
    {/if}
    {#if onadd && links.canLink}
      <Button size="sm" variant="ghost" icon={Link} onclick={onadd}>{m.addLink()}</Button>
    {/if}
  </div>

  <div id="{id}-body" class="body" hidden={collapsed}>
    {#if groups.length === 0}
      <p class="muted">{m.noLinks()}</p>
    {:else}
      <div
        class="tabs"
        role="tablist"
        aria-label={m.linkTabs()}
        tabindex="-1"
        bind:this={tabList}
        onkeydown={tabKeys}
      >
        <button
          type="button"
          role="tab"
          id="{id}-tab-all"
          aria-selected={tab === null}
          aria-controls="{id}-panel"
          tabindex={tab === null ? 0 : -1}
          onclick={() => choose(null)}
          >{m.allLinks()} <span class="count">{links.total}</span></button
        >
        {#each groups as group (key(group))}
          <button
            type="button"
            role="tab"
            id="{id}-tab-{key(group)}"
            aria-selected={tab === key(group)}
            aria-controls="{id}-panel"
            tabindex={tab === key(group) ? 0 : -1}
            onclick={() => choose(key(group))}
            >{group.name} <span class="count">{group.total}</span></button
          >
        {/each}
      </div>
      <div
        id="{id}-panel"
        class={['panel', { expanded }]}
        role="tabpanel"
        aria-labelledby={tab === null ? `${id}-tab-all` : `${id}-tab-${tab}`}
        tabindex="0"
      >
        {#each shown as group (key(group))}
          {@const done = progress(group)}
          <div class="group">
            <h3>
              {group.name}
              {#if done}<span class="progress">{done}</span>{/if}
            </h3>
            <ul aria-label={group.name}>
              {#each group.items as row (row.link)}
                {@const linked = issueRow(row.issue.id)}
                <li>
                  <span class="ref"><IssueRef issue={row.issue} /></span>
                  {#if linked}
                    <IconButton
                      icon={Star}
                      size="sm"
                      class={['star', { on: linked.starred }]}
                      label={m.watchIssue({ id: row.issue.readable_id })}
                      pressed={linked.starred}
                      onclick={() => {
                        setStar(linked, !linked.starred).catch(showError);
                      }}
                    />
                  {/if}
                  {#if links.canLink}
                    <IconButton
                      icon={X}
                      size="sm"
                      label={m.removeLink({ id: row.issue.readable_id })}
                      onclick={() => remove(row.link)}
                    />
                  {/if}
                </li>
              {/each}
            </ul>
            {#if group.next_cursor !== null}
              <Button
                size="sm"
                variant="ghost"
                loading={links.isLoading(group)}
                onclick={() => links.more(group).catch(showError)}>{m.moreLinks()}</Button
              >
            {/if}
          </div>
        {/each}
      </div>
    {/if}
  </div>
</section>

<style>
  .links {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }

  .head {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-2);
  }

  h2 {
    flex: 1 1 auto;
    font-size: var(--text-lg);
  }

  .toggle {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
    min-height: var(--control-md);
    padding: 0;
    border: 0;
    background: none;
    color: inherit;
    font: inherit;
    cursor: pointer;
  }

  .count {
    color: var(--color-text-muted);
    font-size: var(--text-sm);
    font-weight: normal;
  }

  .list-link {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
    min-height: var(--control-sm);
    font-size: var(--text-sm);
  }

  .body {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }

  .body[hidden] {
    display: none;
  }

  .tabs {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-1);
  }

  [role='tab'] {
    min-height: var(--control-md);
    padding-inline: var(--space-2);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
    background: none;
    color: inherit;
    font: inherit;
    font-size: var(--text-sm);
    cursor: pointer;
  }

  [role='tab'][aria-selected='true'] {
    border-color: var(--color-ink);
    background: var(--color-selected);
    font-weight: var(--weight-medium);
  }

  .panel {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    max-height: 20rem;
    overflow: auto;
  }

  .panel.expanded {
    max-height: none;
  }

  .group {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: var(--space-1);
  }

  h3 {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
    color: var(--color-text-muted);
    font-size: var(--text-sm);
    font-weight: 600;
  }

  .progress {
    font-weight: normal;
  }

  ul {
    display: flex;
    flex-direction: column;
    align-self: stretch;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  li {
    display: flex;
    align-items: center;
    gap: var(--space-1);
    min-height: var(--control-md);
  }

  .ref {
    flex: 1 1 auto;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  /* The whole row height is the link's target (phones). */
  .ref :global(a) {
    display: inline-flex;
    align-items: center;
    max-width: 100%;
    min-height: var(--control-md);
  }

  li :global(.star.on) {
    color: var(--color-brand);
  }

  .muted {
    color: var(--color-text-muted);
  }
</style>
