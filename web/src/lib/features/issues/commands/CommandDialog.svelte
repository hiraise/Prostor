<script lang="ts">
  // The command dialog (step 2.1 §6, CMD-027…CMD-030): one line, its preview and its problems
  // while typing (the assist answers, CMD-028), completion at the caret (CMD-029) with the
  // person's recent lines, a comment with its visibility, silent application for those who may,
  // and — for a line that deletes — a confirm step. The bulk flow (CMD-026) takes a query instead
  // of issues and ends in the job's progress view, after which the silent-by-default prompt asks
  // once. Phones: the dialog is a bottom sheet, the preview folds to a count, changes are
  // announced politely, touch targets stay ≥ 44 px (D-019).
  import { untrack } from 'svelte';
  import { MediaQuery, SvelteSet } from 'svelte/reactivity';

  import type {
    ApplyAnswer,
    Audience,
    BulkProgress,
    CommandAssist,
    Issue,
    UserRef,
  } from '#lib/api/schema.gen.ts';
  import { api } from '#lib/api/client.ts';
  import * as common from '#lib/i18n/messages/common.ts';
  import { can, currentUser, preferences } from '#lib/session/index.ts';
  import { enter, leave } from '#lib/shortcuts/registry.svelte.ts';
  import { audienceInput } from '../actions.ts';
  import AudiencePicker from '../audience/AudiencePicker.svelte';
  import { asAuthor } from '../comments/comments.ts';
  import Button from '#lib/ui/Button.svelte';
  import Dialog from '#lib/ui/Dialog.svelte';
  import Notice from '#lib/ui/Notice.svelte';
  import TextArea from '#lib/ui/TextArea.svelte';
  import { showError, toasts } from '#lib/ui/toast.svelte.ts';
  import { showRuleMessages } from '#lib/ui/rule-messages.ts';
  import { NARROW } from '#lib/theme/breakpoints.ts';

  import { inserted, move, popupKey, utf16At, type Completion } from './completion.ts';
  import * as m from './messages.ts';
  import { deletes, rows } from './preview.ts';
  import { recentCommands } from './recent.svelte.ts';

  interface Props {
    open?: boolean;
    /** The text the dialog opens with; `undefined` shows the last applied command (§6). */
    preset?: string;
    /** The issues the line applies to; the dialog's single mode. */
    issues?: Issue[];
    /** The query every matching issue of which the line applies to; the bulk mode. */
    bulk?: { query: string; project: number | null; hideResolved: boolean; count: number } | null;
    onclose: () => void;
  }

  let { open = $bindable(false), preset, issues = [], bulk = null, onclose }: Props = $props();

  const id = $props.id();
  const phone = new MediaQuery(NARROW, false);
  const me = $derived(currentUser());
  const author: UserRef | null = $derived(me ? asAuthor(me) : null);
  /** The project the comment's visibility picker offers; the single mode's issues or the bulk's. */
  const project = $derived(issues[0]?.project.id ?? bulk?.project ?? null);

  // The line and its assist answer.
  let text = $state('');
  let answer = $state<CommandAssist | null>(null);
  let asking = $state(false);
  let input: HTMLInputElement | undefined = $state();
  let completion = $state<Completion>({ index: null });
  /** The quick fixes the person accepted: the apply makes them (§5). */
  let createMissing = $state(false);
  let comment = $state('');
  let visibility = $state<Audience>({ users: [], groups: [] });
  let restricted = $state(false);
  let silent = $state(false);
  let applying = $state(false);
  let step = $state<'form' | 'confirm' | 'progress'>('form');

  /** Fresh state every time the dialog opens; the line starts as the last applied one (§6). */
  $effect(() => {
    if (!open) return;
    untrack(() => {
      const start = preset ?? recentCommands.last() ?? '';
      if (text !== start) {
        text = start;
        completion = { index: null };
      }
      createMissing = false;
      applying = false;
      step = 'form';
      job = null;
      // Bulk applies the person chose to run silently start silent (§6's prompt).
      silent = bulk !== null && preferences.chosen('commands_silent_prompt') === 'silent';
    });
  });

  // The dialog's context (step 2.2 §7): its keys stay its own (§4, shipped in 2.1); the
  // registration is what lists them in the shortcut reference's dialog section.
  $effect(() => {
    if (!open) return;
    enter('dialog');
    return () => leave('dialog');
  });

  // The assist while typing (CMD-028, CMD-029): debounced, only the last call's answer shows —
  // and only while the dialog is open. The caret is state the input's own events keep — keyup
  // covers the arrows moving it.
  let caret = $state(0);
  let assistSeq = 0;
  $effect(() => {
    if (!open) return;
    const at = caret;
    const line = text;
    const withComment = comment;
    const targets = issues;
    const over = bulk;
    const sequence = ++assistSeq;
    const timer = setTimeout(() => {
      asking = true;
      const request = {
        text: line,
        caret: at,
        comment: withComment.trim() === '' ? null : withComment,
      };
      const body = over
        ? {
            ...request,
            issues: [],
            query: over.query,
            project: over.project,
            hide_resolved: over.hideResolved,
            time_zone: null,
          }
        : {
            ...request,
            issues: targets.map((issue) => issue.id),
            query: null,
            project: null,
            hide_resolved: false,
            time_zone: null,
          };
      api
        .post('/commands/assist', { body })
        .then((found: CommandAssist) => {
          if (sequence !== assistSeq) return;
          answer = found;
          // An open popup stays open over the fresh answer, back at its first row.
          completion = {
            index: completion.index !== null && found.suggestions.length > 0 ? 0 : null,
          };
        })
        .catch((error: unknown) => {
          if (sequence !== assistSeq) return;
          showError(error);
        })
        .finally(() => {
          if (sequence === assistSeq) asking = false;
        });
    }, 150);
    return () => {
      clearTimeout(timer);
    };
  });

  const problems = $derived(answer?.problems ?? []);
  /** What blocks the apply: problems no accepted quick fix excuses (§5). */
  const blocking = $derived(problems.filter((problem) => !(createMissing && problem.quick_fix)));
  const operations = $derived(answer?.operations ?? []);
  const previewRows = $derived(answer ? rows(answer, m.rowTitle, m.moveNote()) : []);
  const previewChanges = $derived(
    previewRows.reduce((sum, row) => sum + Math.max(row.parts.length, 1), 0),
  );
  const deleting = $derived(deletes(operations));
  /** The confirm names the count when the dialog's target is known (§8.2 of step 2.2). */
  const deleteCountTitle = $derived.by(() => {
    const count = bulk?.count ?? (issues.length > 0 ? issues.length : null);
    return count === null ? m.deleteConfirmTitle() : m.deleteConfirmCountTitle({ count });
  });
  const hasComment = $derived(operations.some((operation) => operation.kind === 'comment'));
  const canApply = $derived(
    !applying &&
      !asking &&
      text.trim() !== '' &&
      blocking.length === 0 &&
      !(hasComment && comment.trim() === ''),
  );
  /** Silent application needs `issue.silent` in every project the line touches (§4.4). */
  const maySilent = $derived.by(() => {
    if (bulk) return bulk.project === null || can('issue.silent', bulk.project);
    return issues.every((issue) => can('issue.silent', issue.project.id));
  });
  /** The preview folds to its count on phones (§6). */
  let previewOpen = $state(false);
  const showPreview = $derived(!phone.current || previewOpen);

  /** The problem ranges of the current text, in the input's UTF-16 units, as wavy underlines. */
  const underlines = $derived.by(() => {
    const marks: ('error' | null)[] = Array.from({ length: text.length }, () => null);
    for (const problem of problems) {
      const from = Math.min(utf16At(text, problem.start), Math.max(text.length - 1, 0));
      const to = Math.max(utf16At(text, problem.end), from + 1);
      for (let i = from; i < to && i < text.length; i += 1) marks[i] = 'error';
    }
    // One span per run of the same kind.
    const spans: { text: string; error: boolean }[] = [];
    for (let i = 0; i < text.length; i += 1) {
      const last = spans.at(-1);
      const error = marks[i] === 'error';
      if (last?.error === error) last.text += text.charAt(i);
      else spans.push({ text: text.charAt(i), error });
    }
    return spans;
  });

  // ---- Completion (CMD-029) ----

  const suggestions = $derived(answer?.suggestions ?? []);
  const active = $derived(
    completion.index === null ? null : (suggestions[completion.index] ?? null),
  );

  function take(suggestionIndex: number): void {
    const suggestion = suggestions[suggestionIndex];
    if (!suggestion || !input) return;
    const next = inserted(text, suggestion);
    const field = input;
    text = next.text;
    completion = { index: null };
    requestAnimationFrame(() => {
      field.focus();
      field.setSelectionRange(next.caret, next.caret);
    });
  }

  function onKeydown(event: KeyboardEvent): void {
    const key = popupKey(event);
    if (completion.index !== null && key !== null) {
      event.preventDefault();
      if (key === 'insert') take(completion.index);
      else
        completion = { index: move(completion.index, key === 'down' ? 1 : -1, suggestions.length) };
      return;
    }
    if (key === 'down' && suggestions.length > 0 && completion.index === null) {
      event.preventDefault();
      completion = { index: 0 };
      return;
    }
    // Escape closes the popup first; the dialog itself follows on the next press.
    if (event.key === 'Escape' && completion.index !== null) {
      event.preventDefault();
      completion = { index: null };
    }
    if (
      event.key === 'Enter' &&
      !event.shiftKey &&
      !event.metaKey &&
      !event.ctrlKey &&
      !event.altKey
    ) {
      event.preventDefault();
      void applyCommand(silent);
    }
  }

  // ---- Applying (CMD-023, CMD-025, CMD-026) ----

  async function applyCommand(withoutNotifications: boolean): Promise<void> {
    if (!canApply || step === 'progress') return;
    if (deleting && step === 'form') {
      step = 'confirm';
      return;
    }
    applying = true;
    try {
      const commentBody = comment.trim() === '' ? null : comment;
      const audience = restricted ? audienceInput(visibility) : null;
      if (bulk) {
        const queued = await api.post('/commands/bulk', {
          body: {
            text,
            query: bulk.query,
            project: bulk.project,
            hide_resolved: bulk.hideResolved,
            time_zone: null,
            comment: commentBody,
            comment_visibility: audience,
            silent: withoutNotifications,
            create_missing: createMissing,
            run_as: null,
          },
        });
        jobId = queued.job;
        job = null;
        step = 'progress';
        appliedSilent = withoutNotifications;
        return;
      }
      const applied = await api.post('/commands', {
        body: {
          text,
          issues: issues.map((issue) => issue.id),
          comment: commentBody,
          comment_visibility: audience,
          silent: withoutNotifications,
          create_missing: createMissing,
          time_zone: null,
          run_as: null,
        },
      });
      appliedToast(applied);
      recentCommands.remember(text);
      open = false;
    } catch (error) {
      showError(error);
    } finally {
      applying = false;
    }
  }

  function appliedToast(applied: ApplyAnswer): void {
    const done = applied.issues.filter((result) => result.ok);
    if (done.length === 0) {
      toasts.show({ tone: 'error', message: m.notAppliedToast() });
    } else {
      toasts.show({ tone: 'success', message: m.appliedToast({ count: done.length }) });
    }
    // What the projects' behaviors said, once per distinct message (§3.11).
    const seen = new SvelteSet<string>();
    for (const result of applied.issues) {
      for (const message of result.messages) {
        const key = `${message.rule}:${message.code}:${JSON.stringify(message.params)}`;
        if (seen.has(key)) continue;
        seen.add(key);
        showRuleMessages([message]);
      }
    }
  }

  // The dialog-level shortcuts: ⌘/Ctrl+Enter applies, +Shift without notifications (§6).
  function formKeydown(event: KeyboardEvent): void {
    if (!(event.metaKey || event.ctrlKey) || event.key !== 'Enter') return;
    event.preventDefault();
    void applyCommand(event.shiftKey && maySilent);
  }

  // ---- The bulk job's progress (CMD-026) ----

  let jobId = $state<number | null>(null);
  let job = $state<BulkProgress | null>(null);
  let appliedSilent = $state(false);
  const finished = $derived(
    job !== null && ['finished', 'cancelled', 'failed'].includes(job.state),
  );

  $effect(() => {
    const running = step === 'progress' && jobId !== null && !finished;
    if (!running) return;
    const every = setInterval(() => {
      const id = jobId;
      if (id === null) return;
      api
        .get('/commands/bulk/{id}', { path: { id } })
        .then((found: BulkProgress) => {
          job = found;
          if (['finished', 'cancelled', 'failed'].includes(found.state)) {
            bulkArrived(found);
          }
        })
        .catch(() => undefined);
    }, 1000);
    return () => {
      clearInterval(every);
    };
  });

  function bulkArrived(found: BulkProgress): void {
    if (found.state === 'finished') {
      toasts.show({
        tone: found.failed > 0 ? 'error' : 'success',
        message: m.bulkDone({ done: found.done, failed: found.failed }),
      });
    } else if (found.state === 'failed') {
      toasts.show({ tone: 'error', message: m.bulkFailed() });
    } else {
      toasts.show({ tone: 'info', message: m.bulkCancelled() });
    }
    // The silent-by-default prompt, once, after a first bulk apply without silent (§6).
    if (!appliedSilent && preferences.chosen('commands_silent_prompt') === undefined) {
      promptSilent = true;
    }
  }

  let promptSilent = $state(false);
</script>

<svelte:window onkeydown={open ? formKeydown : undefined} />

<Dialog
  bind:open
  title={bulk ? m.bulkTitle({ count: bulk.count }) : m.dialogTitle()}
  size="lg"
  dismissible={!applying && step !== 'progress'}
  onclose={() => {
    step = 'form';
    onclose();
    open = false;
  }}
>
  {#if step === 'form'}
    <form class="form" onsubmit={(event) => event.preventDefault()}>
      <div class="command">
        <label class="sr-only" for="{id}-input">{m.inputLabel()}</label>
        <div class="field">
          <div class="layer" aria-hidden="true">
            {#each underlines as span, i (i)}<span class:error={span.error}>{span.text}</span
              >{/each}
          </div>
          <input
            id="{id}-input"
            bind:this={input}
            bind:value={text}
            type="text"
            role="combobox"
            placeholder={m.placeholder()}
            autocomplete="off"
            autocapitalize="none"
            spellcheck="false"
            aria-invalid={problems.length > 0 ? 'true' : undefined}
            aria-describedby={problems.length > 0 ? `${id}-problems` : undefined}
            aria-autocomplete="list"
            aria-expanded={completion.index !== null}
            aria-controls={`${id}-list`}
            aria-activedescendant={active !== null && completion.index !== null
              ? `${id}-option-${String(completion.index)}`
              : undefined}
            oninput={() => {
              completion = { index: null };
              caret = input?.selectionStart ?? text.length;
            }}
            onkeyup={() => {
              caret = input?.selectionStart ?? text.length;
            }}
            onkeydown={onKeydown}
          />
        </div>
        {#if suggestions.length > 0 && completion.index !== null}
          <ul
            id="{id}-list"
            class="popup"
            role="listbox"
            aria-label={m.completionLabel()}
            onmousedown={(event) => event.preventDefault()}
          >
            {#each suggestions as suggestion, i (i)}
              <!-- svelte-ignore a11y_click_events_have_key_events -->
              <li
                id="{id}-option-{i}"
                role="option"
                aria-selected={i === completion.index}
                class="option"
                data-active={i === completion.index || undefined}
                onclick={() => take(i)}
              >
                <span class="insert">{suggestion.text}</span>
                {#if suggestion.description}<span class="describe">{suggestion.description}</span
                  >{/if}
                {#if suggestion.resolved}<span class="describe">{suggestion.resolved}</span>{/if}
                {#if suggestion.present}<span class="mark">{m.presentMark()}</span>{/if}
                <span class="group">{m.groups[suggestion.group]()}</span>
              </li>
            {/each}
          </ul>
        {/if}
      </div>

      {#if problems.length > 0}
        <div id="{id}-problems" class="problems">
          {#each problems as problem, i (i)}
            <p class="problem">
              {m.problemText(problem)}
              {#if problem.quick_fix}
                <Button
                  size="sm"
                  variant="ghost"
                  onclick={() => {
                    createMissing = true;
                  }}
                  disabled={createMissing}
                >
                  {m.createValue({ name: problem.quick_fix.name })}
                </Button>
              {/if}
            </p>
          {/each}
        </div>
      {/if}

      <section class="preview" aria-label={m.previewTitle()}>
        <div class="preview-head">
          <h3>{m.previewTitle()}</h3>
          {#if phone.current && previewRows.length > 0}
            <Button
              size="sm"
              variant="ghost"
              aria-expanded={showPreview}
              onclick={() => (previewOpen = !previewOpen)}
            >
              {previewOpen ? m.previewCollapse() : m.previewCount({ count: previewChanges })}
            </Button>
          {/if}
        </div>
        <p class="count" aria-live="polite">
          {#if previewRows.length === 0}
            {m.previewEmpty()}
          {:else if showPreview}
            {m.previewCount({ count: previewChanges })}
          {/if}
        </p>
        {#if showPreview && previewRows.length > 0}
          <ul class="rows">
            {#each previewRows as row, i (i)}
              <li class="row">
                <span class="row-title">{row.title}</span>
                {#if row.parts.length > 0}
                  <ul class="parts">
                    {#each row.parts as part, j (j)}
                      <li class="part" data-sign={part.sign ?? undefined}>
                        {part.sign === '+' ? '+' : part.sign === '-' ? '−' : ''}{part.text}
                      </li>
                    {/each}
                  </ul>
                {/if}
                {#if row.note}<p class="note">{row.note}</p>{/if}
              </li>
            {/each}
          </ul>
        {/if}
      </section>

      <div class="comment">
        <TextArea
          label={m.commentTitle()}
          bind:value={comment}
          rows={2}
          placeholder={m.commentPlaceholder()}
        />
        {#if project !== null && author}
          <AudiencePicker {project} reporter={author} bind:value={visibility} bind:restricted />
        {/if}
        {#if hasComment && comment.trim() === ''}
          <p class="problem">{m.commentMissing()}</p>
        {/if}
      </div>
    </form>
  {:else if step === 'confirm'}
    <div class="confirm">
      <h3>{deleteCountTitle}</h3>
      <Notice tone="danger">{m.deleteNote()}</Notice>
      {#if previewRows.length > 0}
        <ul class="rows">
          {#each previewRows as row, i (i)}
            <li class="row">
              <span class="row-title">{row.title}</span>
              {row.parts.map((part) => `${part.sign ?? ''}${part.text}`).join(' ')}
            </li>
          {/each}
        </ul>
      {/if}
    </div>
  {:else if bulk}
    <div class="progress">
      <p class="count" aria-live="polite">
        {#if !finished}
          {m.bulkRunning({ done: job?.done ?? 0, total: job?.total ?? null })}
        {:else if job?.state === 'finished'}
          {m.bulkDone({ done: job.done, failed: job.failed })}
        {:else if job?.state === 'failed'}
          {m.bulkFailed()}
        {:else}
          {m.bulkCancelled()}
        {/if}
      </p>
      {#if job?.failures.length}
        <p class="failures">
          {m.bulkFailures()}
          {job.failures.map((failure) => `#${String(failure.issue)}`).join(', ')}
        </p>
      {/if}
      {#if finished}
        <Button variant="secondary" onclick={() => (open = false)}>{m.back()}</Button>
      {/if}
    </div>
  {/if}

  {#snippet footer()}
    {#if step === 'form'}
      <Button variant="ghost" disabled={applying} onclick={() => (open = false)}>
        {common.cancel()}
      </Button>
      {#if maySilent}
        <Button
          variant="secondary"
          loading={applying}
          disabled={!canApply}
          onclick={() => void applyCommand(true)}
        >
          {m.applySilent()}
        </Button>
      {/if}
      <Button
        variant="primary"
        loading={applying}
        disabled={!canApply}
        onclick={() => void applyCommand(silent)}
      >
        {deleting ? m.confirmDelete() : m.apply()}
      </Button>
    {:else if step === 'confirm'}
      <Button variant="ghost" onclick={() => (step = 'form')}>{m.back()}</Button>
      <Button variant="primary" loading={applying} onclick={() => void applyCommand(silent)}>
        {m.confirmDelete()}
      </Button>
    {:else}
      <Button variant="ghost" onclick={() => (open = false)}>{m.back()}</Button>
    {/if}
  {/snippet}
</Dialog>

<Dialog bind:open={promptSilent} title={m.silentPromptTitle()} size="sm">
  <p>{m.silentPromptAbout()}</p>
  {#snippet footer()}
    <Button
      variant="ghost"
      onclick={() => {
        preferences.set({ commands_silent_prompt: 'never' });
        promptSilent = false;
      }}
    >
      {m.silentPromptNever()}
    </Button>
    <Button
      variant="secondary"
      onclick={() => {
        preferences.set({ commands_silent_prompt: 'dismissed' });
        promptSilent = false;
      }}
    >
      {m.silentPromptNo()}
    </Button>
    <Button
      variant="primary"
      onclick={() => {
        preferences.set({ commands_silent_prompt: 'silent' });
        promptSilent = false;
      }}
    >
      {m.silentPromptYes()}
    </Button>
  {/snippet}
</Dialog>

<style>
  .form,
  .confirm,
  .progress {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }

  .command {
    position: relative;
  }

  .field {
    position: relative;
  }

  .layer,
  input {
    box-sizing: border-box;
    width: 100%;
    height: var(--control-md);
    padding: 0 var(--space-2);
    font-family: var(--font-mono);
    /* 16 px on touch screens keeps mobile Safari from zooming into the field. */
    font-size: max(var(--text-md), 1rem);
    line-height: var(--control-md);
    white-space: pre;
  }

  .layer {
    position: absolute;
    inset: 0;
    overflow: hidden;
    color: var(--color-text);
    pointer-events: none;
  }

  .layer span {
    text-decoration-skip-ink: none;
  }

  .layer span.error {
    text-decoration: underline wavy;
    text-decoration-color: var(--color-danger);
    text-underline-offset: 0.25em;
  }

  input {
    position: relative;
    border: 1px solid var(--color-border-control);
    border-radius: var(--radius-md);
    background: var(--color-surface);
    color: var(--color-text);
  }

  input:focus-visible {
    outline-offset: 0;
    border-color: var(--color-focus);
  }

  input[aria-invalid='true'] {
    border-color: var(--color-danger);
  }

  .popup {
    position: absolute;
    z-index: 1;
    inset-inline: 0;
    top: calc(100% + var(--space-1));
    max-height: 16rem;
    margin: 0;
    overflow: auto;
    overscroll-behavior: contain;
    padding: var(--space-1);
    list-style: none;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    background: var(--color-surface-raised);
    box-shadow: var(--shadow-lg);
  }

  .option {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-1) var(--space-2);
    border-radius: var(--radius-sm);
    cursor: pointer;
  }

  .option[data-active] {
    background: var(--color-surface-sunken);
  }

  .insert {
    font-family: var(--font-mono);
  }

  .describe,
  .mark {
    color: var(--color-text-muted);
    font-size: var(--text-sm);
  }

  .group {
    margin-inline-start: auto;
    color: var(--color-text-faint);
    font-size: var(--text-sm);
  }

  .problems {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
  }

  .problem {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-2);
    color: var(--color-danger);
  }

  .preview {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    padding: var(--space-3);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    background: var(--color-surface-sunken);
  }

  .preview-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2);
  }

  .preview h3,
  .confirm h3 {
    font-size: var(--text-md);
    font-weight: var(--weight-medium);
  }

  .count {
    color: var(--color-text-muted);
    font-size: var(--text-sm);
  }

  .rows {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .row {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
  }

  .row-title {
    color: var(--color-text-muted);
    font-size: var(--text-sm);
  }

  .parts {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .part {
    padding-inline: var(--space-2);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-full);
    background: var(--color-surface);
    font-family: var(--font-mono);
    font-size: var(--text-sm);
  }

  .part[data-sign='+'] {
    border-color: var(--color-success, var(--color-border));
  }

  .part[data-sign='-'] {
    border-color: var(--color-danger);
    text-decoration: line-through;
  }

  .note {
    margin: 0;
    color: var(--color-text-faint);
    font-size: var(--text-sm);
  }

  .comment {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }

  .failures {
    color: var(--color-text-muted);
    font-size: var(--text-sm);
  }

  /* Phones (D-019): touch targets of at least 44 px, in the list and the field. */
  @media (pointer: coarse) {
    .option {
      min-height: 44px;
    }

    .layer,
    input {
      height: 44px;
      line-height: 44px;
    }
  }
</style>
