<script lang="ts">
  // The Markdown editor (CNT-029): a labelled text field with formatting buttons and shortcuts,
  // a switch between the text and its preview as the server renders it, and — for texts of an
  // issue in the full profile — a list of people to mention after `@` (CNT-054). Formatting is
  // typed into the field as one step, so the browser's undo takes it back. With `upload`, pasted
  // pictures and dropped files are uploaded and named in the text (CNT-034); the IDs of those
  // uploads go to `uploads`, so the owner sends them with the text and the preview shows them.
  // After an opening fence — typed, or put by «Блок кода» — a list offers the languages code
  // blocks highlight (CNT-045); no choice keeps auto-detection.
  import BoldIcon from '@lucide/svelte/icons/bold';
  import CodeIcon from '@lucide/svelte/icons/code';
  import HeadingIcon from '@lucide/svelte/icons/heading';
  import ItalicIcon from '@lucide/svelte/icons/italic';
  import LinkIcon from '@lucide/svelte/icons/link';
  import ListIcon from '@lucide/svelte/icons/list';
  import ListChecksIcon from '@lucide/svelte/icons/list-checks';
  import ListOrderedIcon from '@lucide/svelte/icons/list-ordered';
  import SquareCodeIcon from '@lucide/svelte/icons/square-code';
  import StrikethroughIcon from '@lucide/svelte/icons/strikethrough';
  import TableIcon from '@lucide/svelte/icons/table';
  import TextQuoteIcon from '@lucide/svelte/icons/text-quote';
  import TypeIcon from '@lucide/svelte/icons/type';
  import PaperclipIcon from '@lucide/svelte/icons/paperclip';
  import CircleAlert from '@lucide/svelte/icons/circle-alert';
  import type { LucideIcon } from '@lucide/svelte';
  import { untrack } from 'svelte';

  import type {
    FileKind,
    MarkdownProfile,
    Rendered,
    Upload,
    UserRef,
  } from '#lib/api/schema.gen.ts';
  import { PersonName } from '#lib/features/access/index.ts';
  import { preferences } from '#lib/session/index.ts';
  import { createMatchState, match } from '#lib/shortcuts/match.ts';
  import { enter, leave } from '#lib/shortcuts/registry.svelte.ts';
  import { tables } from '#lib/shortcuts/tables/index.ts';
  import { isTypingFocus } from '#lib/shortcuts/typing.ts';
  import { announce } from '#lib/ui/announce.svelte.ts';
  import IconButton from '#lib/ui/IconButton.svelte';
  import Menu, { type MenuEntry } from '#lib/ui/Menu.svelte';
  import Notice from '#lib/ui/Notice.svelte';
  import Spinner from '#lib/ui/Spinner.svelte';
  import { describeError } from '#lib/ui/toast.svelte.ts';

  import { mentionAt, mentionCandidates, previewMarkdown } from './content.ts';
  import { fileMarkdown, linkOverSelection } from './files.ts';
  import { matchLanguages } from './language-names.ts';
  import { fenceAt, formatEdit, type FormatAction, type TextEdit } from './markdown-edit.ts';
  import MarkdownView from './MarkdownView.svelte';
  import * as m from './messages.ts';

  interface Props {
    label: string;
    value?: string;
    /** What the text may contain; `basic` has no mentions, issue IDs or files (CNT-039). */
    profile?: MarkdownProfile;
    /** The issue the text belongs to: mentions offer the people who can see it. */
    issue?: number;
    hint?: string;
    error?: string | null;
    maxlength?: number;
    rows?: number;
    required?: boolean;
    readonly?: boolean;
    placeholder?: string;
    /** Ctrl+Enter or ⌘+Enter in the field. */
    onsubmit?: () => unknown;
    oninput?: () => unknown;
    /** Uploads a pasted or dropped file; without it files are not taken (CNT-034). */
    upload?: (file: File) => Promise<Upload>;
    /** IDs of the uploads this editor made, for the owner to send with the text. */
    uploads?: number[];
    /** Files of the issue «Вложение» offers to name in the text (CNT-044). */
    attachments?: { id: number; name: string; kind: FileKind }[];
  }

  let {
    label,
    value = $bindable(''),
    profile = 'full',
    issue,
    hint,
    error,
    maxlength,
    rows = 6,
    required = false,
    readonly = false,
    placeholder,
    onsubmit,
    oninput,
    upload,
    uploads = $bindable([]),
    attachments = [],
  }: Props = $props();

  /** Typing pauses this long before the preview or the mention list asks the server. */
  const PREVIEW_MS = 300;
  const MENTION_MS = 150;

  /** `aria-keyshortcuts` of a letter with Ctrl (Windows, Linux) or ⌘ (Apple). */
  const keys = (key: string): string => `Control+${key} Meta+${key}`;
  const TABS = ['write', 'preview'] as const;

  const TOOLS: { action: FormatAction; icon: LucideIcon; keys?: string }[] = [
    { action: 'heading', icon: HeadingIcon },
    { action: 'bold', icon: BoldIcon, keys: keys('B') },
    { action: 'italic', icon: ItalicIcon, keys: keys('I') },
    { action: 'strikethrough', icon: StrikethroughIcon, keys: keys('Shift+S') },
    { action: 'code', icon: CodeIcon, keys: keys('E') },
    { action: 'link', icon: LinkIcon, keys: keys('K') },
    { action: 'quote', icon: TextQuoteIcon },
    { action: 'bullets', icon: ListIcon },
    { action: 'numbers', icon: ListOrderedIcon },
    { action: 'tasks', icon: ListChecksIcon },
    { action: 'codeBlock', icon: SquareCodeIcon },
    { action: 'table', icon: TableIcon },
  ];

  const id = $props.id();
  let textarea: HTMLTextAreaElement | undefined = $state();
  let toolbar: HTMLElement | undefined = $state();
  let mode = $state<'write' | 'preview'>('write');
  /** Whether formatting buttons are shown: a personal setting (UI-018). */
  const toolbarShown = $derived(preferences.value('editor_toolbar'));
  let toolIndex = $state(0);

  let rendered = $state.raw<{ key: string; result: Rendered } | null>(null);
  let previewProblem = $state<string | null>(null);

  let mention = $state.raw<{ from: number; query: string } | null>(null);
  let candidates = $state.raw<UserRef[]>([]);
  let candidatesFor = $state<string | null>(null);
  let active = $state(0);
  /** A language being typed after an opening fence (CNT-045). */
  let fence = $state.raw<{ from: number; query: string } | null>(null);
  const languages = $derived(fence ? matchLanguages(fence.query) : []);
  /** Where the line starts whose language was chosen or declined: not offered again there. */
  let settled: number | null = null;

  function lineStart(text: string, at: number): number {
    return text.lastIndexOf('\n', at - 1) + 1;
  }

  const mentions = $derived(profile === 'full' && issue !== undefined && !readonly);
  // Texts of the basic profile cannot name files.
  const takesFiles = $derived(upload !== undefined && profile === 'full' && !readonly);
  let running = $state(0);
  let uploadErrors = $state.raw<string[]>([]);
  let dragging = $state(false);
  const languageOpen = $derived(fence !== null && languages.length > 0);
  const listOpen = $derived(
    languageOpen || (mention !== null && candidatesFor !== null && candidates.length > 0),
  );
  const listCount = $derived(languageOpen ? languages.length : candidates.length);
  const describedBy = $derived(
    [`${id}-hint`, error ? `${id}-error` : null].filter(Boolean).join(' '),
  );

  // ---- Typing into the field ----

  /**
   * Applies an edit as typing, so undo works; falls back to a plain change of the value. With
   * `focus` false a field the person has left is changed without taking the focus back.
   */
  function apply(edit: TextEdit, focus = true): void {
    const area = textarea;
    if (!area) return;
    if (focus) area.focus();
    const expected = area.value.slice(0, edit.from) + edit.insert + area.value.slice(edit.to);
    if (expected !== area.value) {
      area.setSelectionRange(edit.from, edit.to);
      let typed = false;
      if (document.activeElement === area) {
        try {
          // The only way to change a field that keeps the browser's undo history.
          // eslint-disable-next-line @typescript-eslint/no-deprecated
          typed = document.execCommand(
            edit.insert === '' ? 'delete' : 'insertText',
            false,
            edit.insert,
          );
        } catch {
          typed = false;
        }
      }
      if (!typed || area.value !== expected) {
        area.value = expected;
        area.dispatchEvent(new Event('input', { bubbles: true }));
      }
    }
    area.setSelectionRange(edit.selection.start, edit.selection.end);
  }

  function format(action: FormatAction): void {
    const area = textarea;
    if (!area || readonly) return;
    const edit = formatEdit(
      action,
      area.value,
      { start: area.selectionStart, end: area.selectionEnd },
      { column: m.column() },
    );
    apply(edit);
    if (action === 'codeBlock') {
      // The caret goes after the opening fence, where the language is written (CNT-045).
      const at = edit.from + edit.insert.indexOf('\n');
      area.setSelectionRange(at, at);
      findSuggestion();
    }
  }

  // ---- Files and pasted addresses (CNT-034) ----

  /** Inserts `text` over the selection and puts the caret after it. */
  function insert(text: string, focus = true): void {
    const area = textarea;
    if (!area) return;
    const from = Math.min(area.selectionStart, area.selectionEnd);
    const to = Math.max(area.selectionStart, area.selectionEnd);
    const end = from + text.length;
    apply({ from, to, insert: text, selection: { start: end, end } }, focus);
  }

  /** «Вложение»: names a file of the issue at the caret (CNT-044). */
  const attachmentItems = $derived(
    attachments.map((file): MenuEntry => ({
      type: 'action',
      label: file.name,
      onSelect: () => {
        insert(fileMarkdown(file));
      },
    })),
  );

  async function uploadFiles(files: File[]): Promise<void> {
    const send = upload;
    if (!send || files.length === 0) return;
    if (running === 0) uploadErrors = [];
    running += files.length;
    announce(m.uploading({ count: running }));
    const results = await Promise.allSettled(files.map((file) => send(file)));
    running -= files.length;
    const done: Upload[] = [];
    const errors: string[] = [];
    results.forEach((result, index) => {
      if (result.status === 'fulfilled') done.push(result.value);
      else {
        const name = files[index]?.name ?? '';
        errors.push(m.uploadFailed({ name, reason: describeError(result.reason).message }));
      }
    });
    if (done.length > 0) {
      uploads = [...uploads, ...done.map((file) => file.id)];
      // The field takes the focus back only if the person is still in it.
      insert(done.map(fileMarkdown).join('\n'), document.activeElement === textarea);
    }
    uploadErrors = [...uploadErrors, ...errors];
    if (errors.length > 0) announce(errors.join(' '), 'assertive');
    else if (running === 0) announce(m.uploaded({ count: done.length }));
  }

  function paste(event: ClipboardEvent): void {
    const data = event.clipboardData;
    const area = textarea;
    if (!data || !area || readonly) return;
    const text = data.getData('text/plain');
    // Copied text often comes with a picture of itself (spreadsheets): the text wins.
    if (text === '' && takesFiles && data.files.length > 0) {
      event.preventDefault();
      void uploadFiles([...data.files]);
      return;
    }
    const selected = area.value.slice(area.selectionStart, area.selectionEnd);
    const link = linkOverSelection(selected, text);
    if (link !== null) {
      event.preventDefault();
      insert(link);
    }
  }

  function hasFiles(event: DragEvent): boolean {
    return takesFiles && (event.dataTransfer?.types.includes('Files') ?? false);
  }

  function dragover(event: DragEvent): void {
    if (!hasFiles(event)) return;
    event.preventDefault();
    if (event.dataTransfer) event.dataTransfer.dropEffect = 'copy';
    dragging = true;
  }

  function dragleave(event: DragEvent & { currentTarget: HTMLElement }): void {
    const to = event.relatedTarget;
    // Moving between parts of the field is not leaving it.
    if (!(to instanceof Node && event.currentTarget.contains(to))) dragging = false;
  }

  function drop(event: DragEvent): void {
    dragging = false;
    const files = event.dataTransfer?.files;
    if (!hasFiles(event) || !files || files.length === 0) return;
    event.preventDefault();
    textarea?.focus();
    void uploadFiles([...files]);
  }

  // ---- Mentions (CNT-054) ----

  function closeMention(): void {
    mention = null;
    candidates = [];
    candidatesFor = null;
    fence = null;
    active = 0;
  }

  /** Opens the language list after an opening fence, or the mention list after `@word`. */
  function findSuggestion(): void {
    const area = textarea;
    if (!area || readonly || area.selectionStart !== area.selectionEnd) {
      closeMention();
      return;
    }
    if (lineStart(area.value, area.selectionStart) === settled) {
      closeMention();
      return;
    }
    settled = null;
    const opening = fenceAt(area.value, area.selectionStart);
    if (opening) {
      if (opening.from !== fence?.from || opening.query !== fence.query) {
        closeMention();
        fence = opening;
      }
      return;
    }
    fence = null;
    const found = mentions ? mentionAt(area.value, area.selectionStart) : null;
    if (!found) closeMention();
    else if (found.from !== mention?.from || found.query !== mention.query) mention = found;
  }

  function chooseLanguage(name: string): void {
    const area = textarea;
    const at = fence;
    if (!area || !at) return;
    const caret = area.selectionStart;
    // The code goes on the next line: the caret moves there when the block already has one.
    const end = at.from + name.length + (area.value[caret] === '\n' ? 1 : 0);
    closeMention();
    settled = lineStart(area.value, at.from);
    apply({ from: at.from, to: caret, insert: name, selection: { start: end, end } });
  }

  $effect(() => {
    const query = mention?.query;
    const forIssue = issue;
    if (query === undefined || forIssue === undefined) return;
    const controller = new AbortController();
    const timer = setTimeout(() => {
      mentionCandidates(forIssue, query, controller.signal).then(
        (users) => {
          candidates = users.filter((user) => user.kind === 'person');
          candidatesFor = query;
          active = 0;
        },
        // Without suggestions the person types the username themselves.
        () => undefined,
      );
    }, MENTION_MS);
    return () => {
      clearTimeout(timer);
      controller.abort();
    };
  });

  function choose(person: UserRef): void {
    const area = textarea;
    const at = mention;
    if (!area || !at) return;
    const caret = area.selectionStart;
    const insert = `@${person.username} `;
    closeMention();
    apply({
      from: at.from,
      to: caret,
      insert,
      selection: { start: at.from + insert.length, end: at.from + insert.length },
    });
  }

  // ---- Keys ----

  // The editor context (§3): while the field holds the focus its bindings act over the guard's
  // inverted side — letters go to the text, the modifier combos of the binding table (§4, step
  // 2.2) format it. The table is the single source; `Ctrl/⌘+K` for a link stays the field's own
  // key (CNT-029), like the mention list's arrows.
  const EDITOR_TABLES = { ...tables, global: [] };
  const editorKeys = createMatchState(isTypingFocus);

  function keydown(event: KeyboardEvent): void {
    if (listOpen) {
      const count = listCount;
      if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
        event.preventDefault();
        active = (active + (event.key === 'ArrowDown' ? 1 : count - 1)) % count;
        return;
      }
      if (event.key === 'Enter' || event.key === 'Tab') {
        const language = languageOpen ? languages[active] : undefined;
        const person = languageOpen ? undefined : candidates[active];
        if ((language ?? person) && !event.shiftKey && !event.ctrlKey && !event.metaKey) {
          event.preventDefault();
          if (language) chooseLanguage(language);
          else if (person) choose(person);
          return;
        }
      }
      if (event.key === 'Escape') {
        // Closes the list only, not a dialog around the editor.
        event.preventDefault();
        event.stopPropagation();
        if (fence && textarea) settled = lineStart(textarea.value, fence.from);
        closeMention();
        return;
      }
    }
    const binding = match(event, EDITOR_TABLES, 'editor', editorKeys);
    if (binding === null) {
      // The link key of CNT-029, the field's own: physical position, like the table's keys.
      if (
        !readonly &&
        (event.ctrlKey || event.metaKey) &&
        !event.altKey &&
        !event.shiftKey &&
        event.code === 'KeyK'
      ) {
        event.preventDefault();
        format('link');
      }
      return;
    }
    if (readonly) return;
    switch (binding.id) {
      case 'editor.bold':
        format('bold');
        break;
      case 'editor.italic':
        format('italic');
        break;
      case 'editor.strike':
        format('strikethrough');
        break;
      case 'editor.code':
        format('code');
        break;
      case 'editor.submit':
        if (!onsubmit) return; // the key keeps its native meaning
        onsubmit();
        break;
      default:
        // editor.cancel: the form around the field owns Esc (the box folds, the edit cancels).
        return;
    }
    event.preventDefault();
  }

  function toolbarKeydown(event: KeyboardEvent): void {
    const last = TOOLS.length - 1;
    let next: number;
    if (event.key === 'ArrowRight') next = toolIndex === last ? 0 : toolIndex + 1;
    else if (event.key === 'ArrowLeft') next = toolIndex === 0 ? last : toolIndex - 1;
    else if (event.key === 'Home') next = 0;
    else if (event.key === 'End') next = last;
    else return;
    event.preventDefault();
    toolIndex = next;
    toolbar?.querySelectorAll<HTMLButtonElement>('button')[next]?.focus();
  }

  function toggleToolbar(): void {
    preferences.set({ editor_toolbar: toolbarShown ? false : null });
  }

  function tabKeydown(event: KeyboardEvent): void {
    if (!['ArrowLeft', 'ArrowRight', 'Home', 'End'].includes(event.key)) return;
    event.preventDefault();
    const next =
      event.key === 'Home'
        ? 'write'
        : event.key === 'End'
          ? 'preview'
          : mode === 'write'
            ? 'preview'
            : 'write';
    show(next);
    document.getElementById(`${id}-tab-${next}`)?.focus();
  }

  function show(next: 'write' | 'preview'): void {
    mode = next;
    closeMention();
  }

  // ---- Preview ----

  /** What a preview depends on besides the profile and the issue. */
  const previewKey = $derived(JSON.stringify([value, uploads]));

  $effect(() => {
    if (mode !== 'preview') return;
    const text = value;
    const key = previewKey;
    if (text.trim() === '' || untrack(() => rendered?.key) === key) return;
    const textProfile = profile;
    const textIssue = issue;
    const textUploads = uploads;
    const controller = new AbortController();
    const timer = setTimeout(() => {
      previewProblem = null;
      previewMarkdown(text, textProfile, textIssue, textUploads, controller.signal).then(
        (result) => {
          rendered = { key, result };
        },
        (problem: unknown) => {
          if (!controller.signal.aborted) previewProblem = describeError(problem).message;
        },
      );
    }, PREVIEW_MS);
    return () => {
      clearTimeout(timer);
      controller.abort();
    };
  });

  const current = $derived(rendered?.key === previewKey ? rendered.result : null);
</script>

<div class="editor">
  <div class="head">
    <label for={id} id="{id}-label">
      {label}{#if required}<span class="required" aria-hidden="true"> *</span>{/if}
    </label>
    <div class="tabs" role="tablist" aria-label={m.modes({ label })}>
      {#each TABS as tab (tab)}
        <button
          type="button"
          role="tab"
          id="{id}-tab-{tab}"
          class="tab"
          aria-selected={mode === tab}
          aria-controls="{id}-{tab}"
          tabindex={mode === tab ? 0 : -1}
          onclick={() => {
            show(tab);
          }}
          onkeydown={tabKeydown}
        >
          {tab === 'write' ? m.write() : m.preview()}
        </button>
      {/each}
    </div>
  </div>

  <!-- Files dropped anywhere on the field are taken; the field itself is the keyboard's way. -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class={['frame', { invalid: !!error, dragging }]}
    ondragover={dragover}
    ondragleave={dragleave}
    ondrop={drop}
  >
    <div
      class="panel"
      role="tabpanel"
      id="{id}-write"
      aria-labelledby="{id}-tab-write"
      hidden={mode !== 'write'}
    >
      {#if !readonly}
        <div class="bar">
          {#if toolbarShown}
            <!-- Arrow keys move between the buttons; Tab leaves the toolbar (roving focus). -->
            <div
              class="toolbar"
              role="toolbar"
              tabindex="-1"
              aria-label={m.formatting({ label })}
              aria-controls={id}
              bind:this={toolbar}
              onkeydown={toolbarKeydown}
            >
              {#each TOOLS as tool, index (tool.action)}
                <IconButton
                  icon={tool.icon}
                  label={m.actions[tool.action]()}
                  size="md"
                  tabindex={index === toolIndex ? 0 : -1}
                  aria-keyshortcuts={tool.keys}
                  onmousedown={(event: MouseEvent) => {
                    // Keeps the focus and the selection in the field.
                    event.preventDefault();
                  }}
                  onfocus={() => {
                    toolIndex = index;
                  }}
                  onclick={() => {
                    format(tool.action);
                  }}
                />
              {/each}
            </div>
          {/if}
          <IconButton
            class="toolbar-toggle"
            icon={TypeIcon}
            size="md"
            label={toolbarShown ? m.hideFormatting() : m.showFormatting()}
            pressed={toolbarShown}
            onclick={toggleToolbar}
          />
          {#if attachmentItems.length > 0 && profile === 'full'}
            <Menu label={m.attachmentList()} items={attachmentItems}>
              {#snippet trigger(props)}
                <IconButton
                  icon={PaperclipIcon}
                  size="md"
                  label={m.insertAttachment()}
                  {...props}
                />
              {/snippet}
            </Menu>
          {/if}
        </div>
      {/if}
      <textarea
        {id}
        bind:this={textarea}
        bind:value
        {rows}
        {required}
        {readonly}
        {maxlength}
        {placeholder}
        aria-invalid={error ? 'true' : undefined}
        aria-describedby={describedBy}
        aria-autocomplete={readonly ? undefined : 'list'}
        aria-controls={listOpen ? `${id}-mentions` : undefined}
        aria-activedescendant={listOpen ? `${id}-mention-${String(active)}` : undefined}
        onfocus={() => {
          enter('editor');
        }}
        onkeydown={keydown}
        onpaste={paste}
        oninput={() => {
          findSuggestion();
          oninput?.();
        }}
        onclick={findSuggestion}
        onkeyup={(event: KeyboardEvent) => {
          if (['ArrowLeft', 'ArrowRight', 'Home', 'End'].includes(event.key)) findSuggestion();
        }}
        onblur={() => {
          leave('editor');
          closeMention();
        }}></textarea>
      {#if languageOpen}
        <ul class="mentions" role="listbox" id="{id}-mentions" aria-label={m.languageList()}>
          {#each languages as language, index (language)}
            <!-- The field keeps the focus: the list is driven by its keys (combobox pattern). -->
            <!-- svelte-ignore a11y_click_events_have_key_events -->
            <li
              id="{id}-mention-{index}"
              role="option"
              class="option"
              tabindex="-1"
              aria-selected={index === active}
              onmousedown={(event: MouseEvent) => {
                event.preventDefault();
              }}
              onclick={() => {
                chooseLanguage(language);
              }}
            >
              <code>{language}</code>
            </li>
          {/each}
        </ul>
      {:else if listOpen}
        <ul class="mentions" role="listbox" id="{id}-mentions" aria-label={m.mentionList()}>
          {#each candidates as person, index (person.id)}
            <!-- The field keeps the focus: the list is driven by its keys (combobox pattern). -->
            <!-- svelte-ignore a11y_click_events_have_key_events -->
            <li
              id="{id}-mention-{index}"
              role="option"
              class="option"
              tabindex="-1"
              aria-selected={index === active}
              onmousedown={(event: MouseEvent) => {
                event.preventDefault();
              }}
              onclick={() => {
                choose(person);
              }}
            >
              <PersonName {person} withUsername />
            </li>
          {/each}
        </ul>
      {/if}
      {#if !readonly}
        <p class="sr-only" role="status">
          {fence !== null
            ? m.languageCount({ count: languages.length })
            : mention !== null && candidatesFor !== null
              ? m.mentionCount({ count: candidates.length })
              : ''}
        </p>
      {/if}
    </div>

    <div
      class="panel preview"
      role="tabpanel"
      id="{id}-preview"
      aria-labelledby="{id}-tab-preview"
      tabindex="0"
      hidden={mode !== 'preview'}
    >
      {#if mode === 'preview'}
        {#if value.trim() === ''}
          <p class="muted">{m.nothingToPreview()}</p>
        {:else if current}
          <MarkdownView html={current.html} refs={current.refs} headingLevel={3} />
        {:else if previewProblem}
          <Notice tone="danger">{previewProblem}</Notice>
        {:else}
          <p class="muted loading"><Spinner />{m.previewLoading()}</p>
        {/if}
      {/if}
    </div>
  </div>

  {#if running > 0}
    <!-- Announced through announce(); the line shows the same. -->
    <p class="upload-status"><Spinner />{m.uploading({ count: running })}</p>
  {/if}
  {#each uploadErrors as problem, index (index)}
    <p class="error"><CircleAlert aria-hidden="true" />{problem}</p>
  {/each}
  <p id="{id}-hint" class="hint">
    {[hint, m.markdownHint(), takesFiles ? m.filesHint() : null].filter(Boolean).join(' ')}
  </p>
  {#if error}
    <p id="{id}-error" class="error"><CircleAlert aria-hidden="true" />{error}</p>
  {/if}
</div>

<style>
  .editor {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    min-width: 0;
  }

  .head {
    display: flex;
    flex-wrap: wrap;
    align-items: flex-end;
    justify-content: space-between;
    gap: var(--space-1) var(--space-3);
  }

  label {
    font-weight: var(--weight-medium);
  }

  .required {
    margin-inline-start: 0.2em;
    color: var(--color-danger);
  }

  .tabs {
    display: flex;
    gap: var(--space-0_5);
    padding: var(--space-0_5);
    border-radius: var(--radius-md);
    background: var(--color-surface-sunken);
  }

  .tab {
    min-width: var(--control-md);
    min-height: var(--control-md);
    padding: 0 var(--space-3);
    border: 1px solid transparent;
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--color-text-muted);
    font-size: var(--text-sm);
    font-weight: var(--weight-medium);
    cursor: pointer;
  }

  .tab:hover {
    color: var(--color-text);
  }

  .tab[aria-selected='true'] {
    border-color: var(--color-border);
    background: var(--color-surface);
    color: var(--color-text);
    box-shadow: var(--shadow-sm);
  }

  .frame {
    position: relative;
    border: 1px solid var(--color-border-control);
    border-radius: var(--radius-md);
    background: var(--color-surface);
  }

  .frame:focus-within {
    border-color: var(--color-focus);
  }

  .frame.invalid {
    border-color: var(--color-danger);
  }

  /* Files held over the field: a dashed edge says they will be taken. */
  .frame.dragging {
    border-style: dashed;
    border-color: var(--color-focus);
    background: var(--color-selected);
  }

  .upload-status {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    color: var(--color-text-muted);
    font-size: var(--text-sm);
  }

  .panel[hidden] {
    display: none;
  }

  .bar {
    display: flex;
    align-items: center;
    gap: var(--space-1);
    padding: var(--space-0_5) var(--space-1);
    border-bottom: 1px solid var(--color-border);
  }

  /* One row that scrolls sideways on narrow screens. */
  .toolbar {
    display: flex;
    flex: 1;
    gap: var(--space-0_5);
    min-width: 0;
    overflow-x: auto;
    scrollbar-width: thin;
  }

  .toolbar > :global(button),
  .bar :global(.toolbar-toggle) {
    flex: none;
  }

  .bar :global(.toolbar-toggle) {
    margin-inline-start: auto;
  }

  textarea {
    display: block;
    width: 100%;
    min-height: calc(var(--control-md) * 3);
    padding: var(--space-2);
    border: none;
    border-radius: var(--radius-md);
    background: transparent;
    color: var(--color-text);
    font: inherit;
    /* 16 px on touch screens keeps mobile Safari from zooming into the field. */
    font-size: max(var(--text-md), 1rem);
    line-height: var(--leading-normal);
    resize: vertical;
  }

  @media (pointer: fine) {
    textarea {
      font-size: var(--text-md);
    }
  }

  textarea:focus-visible {
    outline: none;
  }

  textarea::placeholder {
    color: var(--color-text-faint);
  }

  textarea[readonly] {
    background: var(--color-surface-sunken);
  }

  .mentions {
    position: absolute;
    z-index: var(--z-sticky);
    inset-inline: var(--space-2);
    top: calc(100% + var(--space-1));
    max-height: 16rem;
    margin: 0;
    padding: var(--space-1);
    overflow-y: auto;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-lg);
    background: var(--color-surface-raised);
    box-shadow: var(--shadow-md);
    list-style: none;
  }

  .option {
    display: flex;
    align-items: center;
    min-height: var(--control-md);
    padding: var(--space-1) var(--space-2);
    border-radius: var(--radius-md);
    cursor: pointer;
  }

  .option:hover {
    background: var(--color-hover);
  }

  .option[aria-selected='true'] {
    background: var(--color-selected);
  }

  .preview {
    min-height: calc(var(--control-md) * 3);
    padding: var(--space-2) var(--space-3);
  }

  .preview:focus-visible {
    outline-offset: -2px;
  }

  .muted {
    color: var(--color-text-muted);
  }

  .loading {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }

  .hint {
    color: var(--color-text-muted);
    font-size: var(--text-sm);
  }

  .error {
    display: flex;
    align-items: flex-start;
    gap: var(--space-1);
    color: var(--color-danger);
    font-size: var(--text-sm);
    overflow-wrap: anywhere;
  }

  .error :global(svg) {
    flex: none;
    width: var(--icon-sm);
    height: var(--icon-sm);
    margin-top: 0.15em;
  }
</style>
