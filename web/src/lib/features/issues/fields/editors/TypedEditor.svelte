<script lang="ts">
  // Editor of a typed field (FLD-002…FLD-006): strings, numbers (`,` or `.` as the decimal mark),
  // dates and date-times in the viewer's order or ISO, with a calendar button that opens the
  // browser's own picker, and periods such as "1h 30m". Enter saves, Escape closes the popover.
  import CalendarDays from '@lucide/svelte/icons/calendar-days';

  import { knownBootstrap } from '#lib/api/system.ts';
  import type { IssueField } from '#lib/api/schema.gen.ts';
  import * as common from '#lib/i18n/messages/common.ts';
  import { DEFAULT_SCHEDULE } from '#lib/time/duration.ts';
  import { getTimeContext } from '#lib/time/context.svelte.ts';
  import { formatCalendarDateForInput, formatInstantForInput } from '#lib/time/parse.ts';
  import { calendarDateInZone } from '#lib/time/zone.ts';
  import Button from '#lib/ui/Button.svelte';
  import IconButton from '#lib/ui/IconButton.svelte';
  import TextField from '#lib/ui/TextField.svelte';

  import * as m from '../../messages.ts';
  import { readTyped, typedText, type TypedKind } from '../field-values.ts';
  import type { FieldRequest } from '../save.ts';

  interface Props {
    entry: IssueField & { field: { kind: TypedKind } };
    onsave: (next: IssueField, request: FieldRequest) => void;
    close: () => void;
  }

  let { entry, onsave, close }: Props = $props();

  const context = $derived({
    time: getTimeContext(),
    schedule: knownBootstrap()?.work_schedule ?? DEFAULT_SCHEDULE,
  });
  const kind = $derived(entry.field.kind);
  // Starts from the value the editor opened with.
  // svelte-ignore state_referenced_locally
  let text = $state(typedText(entry, context));
  let error = $state<string | null>(null);
  let picker: HTMLInputElement | undefined = $state();

  const hint = $derived.by(() => {
    const now = Date.now();
    switch (kind) {
      case 'date':
        return m.exampleHint({
          example: formatCalendarDateForInput(
            calendarDateInZone(now, context.time.timeZone),
            context.time,
          ),
        });
      case 'date_time':
        return m.exampleHint({ example: formatInstantForInput(now, context.time) });
      case 'duration':
        return m.exampleHint({ example: m.durationExample() });
      default:
        return undefined;
    }
  });

  function submit(event?: SubmitEvent): void {
    event?.preventDefault();
    const read = readTyped(kind, text, context);
    if (!read.ok) {
      error = m.typedProblems[read.problem]();
      return;
    }
    if (read.value === null && entry.required) {
      error = m.cannotBeEmpty();
      return;
    }
    if (text.trim() !== typedText(entry, context).trim()) {
      onsave({ ...entry, ...read.shown }, { kind: 'set', value: read.value });
    }
    close();
  }

  function openPicker(): void {
    if (!picker) return;
    try {
      picker.showPicker();
    } catch {
      // Browsers without `showPicker` still take the typed date.
      picker.focus();
    }
  }

  // The native picker answers `YYYY-MM-DD` or `YYYY-MM-DDTHH:mm`, which the reader takes as ISO.
  function picked(): void {
    if (!picker?.value) return;
    text = picker.value.replace('T', ' ');
    submit();
  }
</script>

<form class="editor" onsubmit={submit} novalidate>
  <div class="input">
    <TextField
      label={entry.field.name}
      bind:value={text}
      {hint}
      {error}
      required={entry.required}
      autofocus
      autocomplete="off"
      inputmode={kind === 'integer' ? 'numeric' : kind === 'float' ? 'decimal' : undefined}
      oninput={() => {
        error = null;
      }}
    />
    {#if kind === 'date' || kind === 'date_time'}
      <IconButton class="calendar" icon={CalendarDays} label={m.pickDate()} onclick={openPicker} />
      <input
        bind:this={picker}
        class="native"
        type={kind === 'date' ? 'date' : 'datetime-local'}
        tabindex="-1"
        aria-hidden="true"
        onchange={picked}
      />
    {/if}
  </div>
  <div class="actions">
    <Button variant="ghost" size="sm" onclick={close}>{common.cancel()}</Button>
    <Button variant="primary" size="sm" type="submit">{m.save()}</Button>
  </div>
</form>

<style>
  .editor {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    min-width: 0;
  }

  .input {
    position: relative;
    display: flex;
    align-items: flex-start;
    gap: var(--space-1);
  }

  .input > :global(.field) {
    flex: 1;
  }

  .input :global(.calendar) {
    /* Level with the input under its label. */
    margin-top: calc(var(--text-sm) * 1.5 + var(--space-1));
  }

  .native {
    position: absolute;
    right: 0;
    bottom: 0;
    width: 1px;
    height: 1px;
    padding: 0;
    border: 0;
    opacity: 0;
    pointer-events: none;
  }

  .actions {
    display: flex;
    justify-content: flex-end;
    gap: var(--space-2);
  }
</style>
