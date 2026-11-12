<script lang="ts">
  // The sprint menu (BRD-083): the shown sprint's name opens the board's sprints with a filter, the
  // current one in bold, the archived ones in their own section (loaded on first open, BRD-103), a
  // pencil on each for editors and «Новый спринт». Each sprint is a link to its address, so it
  // opens in a new tab with a modifier.
  import ChevronDown from '@lucide/svelte/icons/chevron-down';
  import Pencil from '@lucide/svelte/icons/pencil';
  import Plus from '@lucide/svelte/icons/plus';

  import type { Sprint } from '#lib/api/schema.gen.ts';
  import * as common from '#lib/i18n/messages/common.ts';
  import Button from '#lib/ui/Button.svelte';
  import IconButton from '#lib/ui/IconButton.svelte';
  import Popover from '#lib/ui/Popover.svelte';
  import Spinner from '#lib/ui/Spinner.svelte';
  import TextField from '#lib/ui/TextField.svelte';

  import { listSprints } from '../boards.ts';
  import { boardAddress } from '../board/header-filter.svelte.ts';
  import * as m from '../messages.ts';
  import { sprintDates } from './schedule.ts';

  interface Props {
    board: { id: number; rights: { edit: boolean } };
    /** The sprints that are not archived, in creation order. */
    sprints: readonly Sprint[];
    /** The sprint on screen; `null` when there is no current one. */
    shown: Sprint | null;
    /** The header filter, kept in the links. */
    q?: string;
    onchoose: (sprint: Sprint) => unknown;
    onedit: (sprint: Sprint) => unknown;
    oncreate: () => unknown;
  }

  let { board, sprints, shown, q = '', onchoose, onedit, oncreate }: Props = $props();

  let open = $state<boolean>(false);
  let text = $state('');
  let archived = $state<Sprint[] | null>(null);
  let problem = $state(false);

  // Archived sprints load on the first open.
  $effect(() => {
    if (!open || archived !== null) return;
    const controller = new AbortController();
    listSprints(board.id, controller.signal).then(
      (all) => {
        archived = all.filter((one) => one.archived).reverse();
        problem = false;
      },
      () => {
        if (!controller.signal.aborted) problem = true;
      },
    );
    return () => {
      controller.abort();
    };
  });

  const typed = $derived(text.trim().toLocaleLowerCase());
  const matches = (one: Sprint): boolean =>
    typed === '' || one.name.toLocaleLowerCase().includes(typed);
  // Newest first: the next sprints are the ones looked for.
  const active = $derived([...sprints].reverse().filter(matches));
  const old = $derived((archived ?? []).filter(matches));

  function choose(event: MouseEvent, one: Sprint, close: () => void): void {
    if (event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) {
      return;
    }
    event.preventDefault();
    close();
    onchoose(one);
  }
</script>

{#snippet row(one: Sprint, close: () => void)}
  <li class="row">
    <!-- boardAddress() resolves the href. -->
    <!-- eslint-disable svelte/no-navigation-without-resolve -->
    <a
      class="name"
      class:current={one.current}
      href={boardAddress(board.id, one.id, q)}
      aria-current={one.id === shown?.id ? 'page' : undefined}
      onclick={(event) => {
        choose(event, one, close);
      }}
    >
      <span
        >{one.name}{#if one.current}<span class="tag">{m.currentSprint()}</span>{/if}</span
      >
      <span class="dates">{sprintDates(one)}</span>
    </a>
    <!-- eslint-enable svelte/no-navigation-without-resolve -->
    {#if board.rights.edit}
      <IconButton
        icon={Pencil}
        size="sm"
        label={m.editSprintOf({ name: one.name })}
        onclick={() => {
          close();
          onedit(one);
        }}
      />
    {/if}
  </li>
{/snippet}

<Popover label={m.sprintMenu()} bind:open placement={{ side: 'bottom', align: 'start' }}>
  {#snippet trigger(props)}
    <Button size="sm" variant="secondary" {...props}>
      <span class="trigger">
        <span class="sr">{m.sprintMenu()}:</span>
        <span class="title">{shown?.name ?? m.noSprint()}</span>
        <ChevronDown aria-hidden="true" />
      </span>
    </Button>
  {/snippet}
  {#snippet children({ close }: { close: () => void })}
    <div class="panel">
      <TextField
        label={m.findSprint()}
        type="search"
        bind:value={text}
        autocomplete="off"
        spellcheck="false"
        maxlength={100}
      />
      {#if active.length === 0 && (archived === null || old.length === 0)}
        <p class="note">{m.noSprintsFound()}</p>
      {/if}
      {#if active.length > 0}
        <ul class="list" aria-label={m.sprintMenu()}>
          {#each active as one (one.id)}{@render row(one, close)}{/each}
        </ul>
      {/if}
      {#if problem}
        <p class="note danger">{m.sprintsFailed()}</p>
      {:else if archived === null}
        <p class="note"><Spinner label={common.loading()} /></p>
      {:else if old.length > 0}
        <h3 class="section">{m.archivedSprints()}</h3>
        <ul class="list" aria-label={m.archivedSprints()}>
          {#each old as one (one.id)}{@render row(one, close)}{/each}
        </ul>
      {/if}
      {#if board.rights.edit}
        <div class="new">
          <Button
            size="sm"
            variant="ghost"
            icon={Plus}
            onclick={() => {
              close();
              oncreate();
            }}>{m.newSprint()}</Button
          >
        </div>
      {/if}
    </div>
  {/snippet}
</Popover>

<style>
  .trigger {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
    max-width: 16rem;
  }

  .trigger :global(svg) {
    flex: none;
    width: var(--icon-sm);
    height: var(--icon-sm);
  }

  .title {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .sr {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip-path: inset(50%);
    white-space: nowrap;
  }

  .panel {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    width: min(22rem, 100%);
    max-height: 28rem;
    padding: var(--space-2);
    overflow-y: auto;
  }

  .list {
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .row {
    display: flex;
    align-items: center;
    gap: var(--space-1);
  }

  .name {
    display: flex;
    flex: 1;
    flex-direction: column;
    justify-content: center;
    min-width: 0;
    min-height: var(--control-md);
    padding: var(--space-1) var(--space-2);
    border-radius: var(--radius-md);
    color: var(--color-text);
    text-decoration: none;
    overflow-wrap: anywhere;
  }

  .name:hover {
    background: var(--color-hover);
  }

  .name[aria-current='page'] {
    background: var(--color-surface-sunken);
  }

  .name.current {
    font-weight: var(--weight-semibold);
  }

  .tag {
    margin-inline-start: var(--space-2);
    color: var(--color-text-muted);
    font-size: var(--text-sm);
    font-weight: var(--weight-regular);
  }

  .dates {
    color: var(--color-text-muted);
    font-size: var(--text-sm);
    font-weight: var(--weight-regular);
  }

  .section {
    padding: var(--space-2) var(--space-2) 0;
    border-top: 1px solid var(--color-border);
    color: var(--color-text-muted);
    font-size: var(--text-sm);
    font-weight: var(--weight-medium);
  }

  .note {
    padding: var(--space-2);
    color: var(--color-text-muted);
    font-size: var(--text-sm);
  }

  .note.danger {
    color: var(--color-danger);
  }

  .new {
    padding-top: var(--space-1);
    border-top: 1px solid var(--color-border);
  }
</style>
