<script lang="ts">
  // The editor of one field by its kind (ISS-019, §3.5 of the step 1.7 plan): typed kinds get a
  // text input, kinds with options a searchable list. Text fields are edited with the
  // description in edit mode, not here.
  import type { IssueField } from '#lib/api/schema.gen.ts';

  import ChoiceEditor from './editors/ChoiceEditor.svelte';
  import TypedEditor from './editors/TypedEditor.svelte';
  import { isTyped, type TypedKind } from './field-values.ts';
  import type { FieldRequest } from './save.ts';

  interface Props {
    issueId: number;
    entry: IssueField;
    onsave: (next: IssueField, request: FieldRequest) => void;
    close: () => void;
  }

  let { issueId, entry, onsave, close }: Props = $props();

  function typed(field: IssueField): field is IssueField & { field: { kind: TypedKind } } {
    return isTyped(field.field.kind);
  }
</script>

{#if typed(entry)}
  <TypedEditor {entry} {onsave} {close} />
{:else if entry.field.kind !== 'text'}
  <ChoiceEditor {issueId} {entry} {onsave} {close} />
{/if}
