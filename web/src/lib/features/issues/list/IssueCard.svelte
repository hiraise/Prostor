<script lang="ts">
  // A list row on a phone (QRY-075, design §11): the issue's ID and summary, a parent's subtask
  // progress (LNK-014), then the chosen columns as small chips. The row's actions — the star,
  // changing fields (QRY-076) and cloning (AUT-066) — open under a visible «Ещё» button.
  import ChevronDown from '@lucide/svelte/icons/chevron-down';
  import FileStack from '@lucide/svelte/icons/file-stack';
  import Files from '@lucide/svelte/icons/files';

  import type { Issue } from '#lib/api/schema.gen.ts';
  import { can } from '#lib/session/index.ts';
  import Button from '#lib/ui/Button.svelte';

  import { askCloning, cloneAsDrafts, cloneIssue, cloning, cloningKnown } from '../clone.svelte.ts';
  import * as cloneTexts from '../messages.ts';
  import IssueRef from '../IssueRef.svelte';
  import Cell from './Cell.svelte';
  import type { ShownColumn } from './columns.ts';
  import FieldCell from './FieldCell.svelte';
  import * as m from './messages.ts';
  import RowStar from './RowStar.svelte';

  let { issue, columns }: { issue: Issue; columns: readonly ShownColumn[] } = $props();

  const id = $props.id();
  let open = $state(false);
  const chips = $derived(
    columns.filter((c) => c.column.kind === 'field' || !['id', 'summary'].includes(c.text)),
  );
  const editable = $derived(
    columns.filter((c) => {
      if (c.column.kind !== 'field') return false;
      const fieldId = c.column.id;
      return issue.fields.some((f) => f.field.id === fieldId && f.editable);
    }),
  );
  const done = $derived(issue.subtasks.total - issue.subtasks.unresolved);
  // The toggle is asked for when the actions open; the buttons appear when it lands (AUT-066).
  const mayClone = $derived(
    !issue.draft && cloningKnown(issue.project.id) && can('issue.create', issue.project.id),
  );
  const busy = $derived(cloning(issue.id));

  $effect(() => {
    if (open && !issue.draft) askCloning(issue.project.id);
  });
</script>

<article class="card" aria-labelledby="{id}-title">
  <div class="head">
    <p class="title" id="{id}-title"><IssueRef {issue} /></p>
    <button
      type="button"
      class="more"
      aria-expanded={open}
      aria-controls="{id}-actions"
      aria-label={m.moreOf({ id: issue.readable_id ?? '' })}
      onclick={() => {
        open = !open;
      }}
    >
      {m.more()}
      <ChevronDown size={16} aria-hidden="true" />
    </button>
  </div>
  {#if issue.subtasks.total > 0 || chips.length > 0}
    <ul class="chips">
      {#if issue.subtasks.total > 0}
        <li class="chip">
          <span aria-hidden="true">{done}/{issue.subtasks.total}</span>
          <span class="sr-only">{m.progress({ done, total: issue.subtasks.total })}</span>
        </li>
      {/if}
      {#each chips as shown (shown.key)}
        <li class="chip">
          <span class="sr-only">{`${shown.name}: `}</span>
          <Cell {issue} column={shown.column} readonly />
        </li>
      {/each}
    </ul>
  {/if}
  <div class="actions" id="{id}-actions" hidden={!open}>
    {#if open}
      <div class="star"><RowStar {issue} size="md" /></div>
      {#if mayClone}
        <div class="clone">
          <Button
            variant="secondary"
            size="sm"
            icon={Files}
            loading={busy}
            onclick={() => void cloneIssue(issue)}
          >
            {cloneTexts.clone()}
          </Button>
          <Button
            variant="ghost"
            size="sm"
            icon={FileStack}
            disabled={busy}
            onclick={() => void cloneAsDrafts([issue])}
          >
            {cloneTexts.cloneAsDraft()}
          </Button>
        </div>
      {/if}
      {#if editable.length > 0}
        <dl class="fields">
          {#each editable as shown (shown.key)}
            {#if shown.column.kind === 'field'}
              <div class="field">
                <dt>{shown.name}</dt>
                <dd><FieldCell {issue} fieldId={shown.column.id} /></dd>
              </div>
            {/if}
          {/each}
        </dl>
      {/if}
    {/if}
  </div>
</article>

<style>
  .card {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    padding: var(--space-3) var(--space-4);
  }

  .head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: var(--space-2);
  }

  .title {
    min-width: 0;
    padding-top: var(--space-2);
  }

  .more {
    display: inline-flex;
    flex: none;
    align-items: center;
    gap: var(--space-1);
    min-width: 2.75rem;
    min-height: 2.75rem;
    padding: 0 var(--space-2);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    background: transparent;
    color: var(--color-text);
    font: inherit;
    font-size: var(--text-sm);
    cursor: pointer;
  }

  .more[aria-expanded='true'] :global(svg) {
    transform: rotate(180deg);
  }

  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-1);
    color: var(--color-text-muted);
    font-size: var(--text-xs);
  }

  .chip {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
    max-width: 100%;
    padding: 0 var(--space-2);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-full);
  }

  .actions {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    padding-top: var(--space-2);
    border-top: 1px solid var(--color-border);
  }

  .clone {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
  }

  .actions[hidden] {
    display: none;
  }

  .fields {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    margin: 0;
  }

  .field {
    display: grid;
    grid-template-columns: minmax(6rem, 35%) 1fr;
    align-items: center;
    gap: var(--space-2);
  }

  dt {
    color: var(--color-text-muted);
    font-size: var(--text-sm);
  }

  dd {
    min-width: 0;
    margin: 0;
  }
</style>
