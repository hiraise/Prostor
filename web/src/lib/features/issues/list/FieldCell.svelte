<script lang="ts">
  // One field of a list row (QRY-076): the value, and for people who may change it a button that
  // opens the field's editor in a popover (a sheet on phones), as on the issue page (ISS-019). A
  // saved change shows at once and is announced; a refused one rolls back with the reason in a
  // toast. A field the row's project lacks shows nothing.
  import type { Issue, IssueField } from '#lib/api/schema.gen.ts';
  import { refusalText } from '#lib/features/fields/index.ts';
  import { announce } from '#lib/ui/announce.svelte.ts';
  import Popover from '#lib/ui/Popover.svelte';
  import { toasts } from '#lib/ui/toast.svelte.ts';

  import { isMissing } from '../fields/field-values.ts';
  import FieldEditor from '../fields/FieldEditor.svelte';
  import FieldValue from '../fields/FieldValue.svelte';
  import { saveField, type FieldRequest } from '../fields/save.ts';
  import * as issueMessages from '../messages.ts';
  import * as m from './messages.ts';

  interface Props {
    issue: Issue;
    fieldId: number;
    /** Only the value, no editor (the phone card's chips). */
    readonly?: boolean;
  }

  let { issue, fieldId, readonly = false }: Props = $props();

  const entry = $derived(issue.fields.find((f) => f.field.id === fieldId));
  const label = $derived(
    entry ? m.fieldOf({ name: entry.field.name, id: issue.readable_id ?? '' }) : '',
  );

  function save(next: IssueField, request: FieldRequest): void {
    saveField(issue.id, next, request).then(
      () => {
        announce(m.fieldSaved({ name: next.field.name }));
      },
      (error: unknown) => {
        toasts.show({ tone: 'error', message: refusalText(error) });
      },
    );
  }
</script>

{#snippet value(shown: IssueField)}
  {#if isMissing(shown)}
    <span class="missing">{issueMessages.requiredField()}</span>
  {:else}
    <FieldValue entry={shown} />
  {/if}
{/snippet}

{#if entry}
  {#if !readonly && entry.editable && entry.field.kind !== 'text'}
    <Popover label={issueMessages.fieldEditor({ name: entry.field.name })}>
      {#snippet trigger(props)}
        <button type="button" class="value" {...props}
          ><span class="sr-only">{`${label}: `}</span>{@render value(entry)}</button
        >
      {/snippet}
      {#snippet children({ close })}
        <FieldEditor issueId={issue.id} {entry} onsave={save} {close} />
      {/snippet}
    </Popover>
  {:else}
    {@render value(entry)}
  {/if}
{/if}

<style>
  .value {
    display: inline-flex;
    align-items: center;
    max-width: 100%;
    min-height: 2rem;
    margin-inline: calc(-1 * var(--space-1));
    padding: 0 var(--space-1);
    border: 1px solid transparent;
    border-radius: var(--radius-sm);
    background: transparent;
    color: inherit;
    font: inherit;
    text-align: start;
    cursor: pointer;
  }

  .value:hover {
    border-color: var(--color-border);
    background: var(--color-hover);
  }

  .missing {
    color: var(--color-danger);
  }
</style>
