<script lang="ts">
  import { brand } from '#lib/i18n/messages/brand.ts';
  // The issue page (ISS-001, ISS-016, ISS-020, UI-005), read form: breadcrumbs, then a frame of
  // the head cluster (the metadata line above the summary, the summary with icon actions, tags,
  // the parent) and the content beside the field panel in its own tone — the panel runs from the
  // metadata line down. The panel's side, width and collapsed state are personal settings
  // (UI-018); on phones the panel is a section above the content. Opened by a former or internal
  // ID, the page puts the current ID in the address (ISS-003). «Изменить», F2 or `e` turns the
  // content into the edit form (ISS-018, ISS-047); the page is the `issue` context of the
  // shortcut framework — its keys act while no form or dialog has them — and the command dialog
  // is claimed for the shell's global combo and the letters of `commands_on_typing`. Tags, moving
  // and deleting are in the head and the field panel. Embedded in the board popup, the head's
  // corner carries «Открыть страницей» and the close button (`onclose`), level with the ID.
  import { goto } from '$app/navigation';
  import ExternalLink from '@lucide/svelte/icons/external-link';
  import Link2 from '@lucide/svelte/icons/link-2';
  import ListTree from '@lucide/svelte/icons/list-tree';
  import Pencil from '@lucide/svelte/icons/pencil';
  import SquarePlus from '@lucide/svelte/icons/square-plus';
  import X from '@lucide/svelte/icons/x';
  import { onMount, untrack } from 'svelte';

  import type { Issue, NewIssueLink, Project } from '#lib/api/schema.gen.ts';
  import { MarkdownView } from '#lib/features/content/index.ts';
  import { projectHref } from '#lib/features/projects/index.ts';
  import * as common from '#lib/i18n/messages/common.ts';
  import { can, currentUser, preferences } from '#lib/session/index.ts';
  import { createMatchState, match } from '#lib/shortcuts/match.ts';
  import {
    claim,
    enter,
    leave,
    setFocusedEntity,
    type FocusedEntity,
  } from '#lib/shortcuts/registry.svelte.ts';
  import { tables } from '#lib/shortcuts/tables/index.ts';
  import { isTypingFocus } from '#lib/shortcuts/typing.ts';
  import Breadcrumbs from '#lib/ui/Breadcrumbs.svelte';
  import Button from '#lib/ui/Button.svelte';
  import IconButton from '#lib/ui/IconButton.svelte';
  import type { MenuEntry } from '#lib/ui/menu.ts';
  import Notice from '#lib/ui/Notice.svelte';
  import Resizer from '#lib/ui/Resizer.svelte';
  import { showError, toasts } from '#lib/ui/toast.svelte.ts';
  import { tooltip } from '#lib/ui/tooltip.ts';

  import ActivityStream from '../activity/ActivityStream.svelte';
  import { ActivityStream as Stream } from '../activity/activity.svelte.ts';
  import { assignToMe, setStar, setTag } from '../actions.ts';
  import CommentBox from '../comments/CommentBox.svelte';
  import { asAuthor } from '../comments/comments.ts';
  import AttachmentsSection from '../files/AttachmentsSection.svelte';
  import { IssueFiles, setIssueFiles, type SendFile } from '../files/files.svelte.ts';
  import Lightbox from '../files/Lightbox.svelte';
  import PageDrop from '../files/PageDrop.svelte';
  import FieldPanel from '../fields/FieldPanel.svelte';
  import { isBlock, isEmpty } from '../fields/field-values.ts';
  import IssueRef from '../IssueRef.svelte';
  import { issueHref, projectIssuesHref, type IssuePageData } from '../issues.ts';
  import AddLinkDialog from '../links/AddLinkDialog.svelte';
  import LinksPanel from '../links/LinksPanel.svelte';
  import {
    duplicateName,
    IssueLinksState,
    linkTypes,
    nameKey,
    subtaskLink,
  } from '../links/links.svelte.ts';
  import CommandDialog from '../commands/CommandDialog.svelte';
  import { recentCommands } from '../commands/recent.svelte.ts';
  import { setIssueCreateExtra } from '../new/create-extra.svelte.ts';
  import { openNewIssue } from '../new/open.svelte.ts';
  import * as m from '../messages.ts';
  import TagChip from '../tags/TagChip.svelte';
  import TagPicker from '../tags/TagPicker.svelte';
  import DeleteDialog from './DeleteDialog.svelte';
  import EditMode from './EditMode.svelte';
  import IssueActions from './IssueActions.svelte';
  import IssueHeader from './IssueHeader.svelte';
  import MetaStrip from './MetaStrip.svelte';
  import { IssueLive, type LiveSource } from './live.svelte.ts';
  import MoveDialog from './MoveDialog.svelte';
  import VisibilityMenu from './VisibilityMenu.svelte';

  interface Props {
    issue: Issue;
    /** The ID the address asked for. */
    requested?: string;
    /** The page's activity, links and files from its one request. */
    page?: IssuePageData;
    /** Sends a file; the app's upload by default (tests give their own). */
    upload?: SendFile;
    /** Where live changes come from; the tab's connection by default (tests give their own). */
    connection?: LiveSource;
    /** Shown in a dialog (the board's issue popup, BRD-079): no breadcrumbs, title or address. */
    embedded?: boolean;
    /** Closes the embedded popup: the × of the head's corner. */
    onclose?: () => unknown;
    /** Opens in edit mode (BRD-080). */
    edit?: boolean;
  }

  let {
    issue,
    requested,
    page,
    upload,
    connection,
    embedded = false,
    onclose,
    edit = false,
  }: Props = $props();

  const id = $props.id();
  const PANEL_MIN = 160;
  const PANEL_MAX = 400;

  /** The width while the handle is dragged; saved when the drag ends. */
  let dragged = $state<number | null>(null);
  let editing = $state(untrack(() => edit && issue.can.update));
  let tagging = $state(false);
  let moving = $state(false);
  /** The project the panel's dropdown chose, when the move needs the preview dialog. */
  let moveTarget = $state<Project | null>(null);
  let deleting = $state(false);
  /** The command dialog and what it opens with (step 2.1 §6). */
  let commanding = $state(false);
  let commandPreset = $state<string | undefined>();
  /** The link dialog and what it opened for (LNK-005, ISS-009, LNK-015). */
  let linking = $state(false);
  let linkMode = $state<'link' | 'new' | 'duplicate'>('link');
  let duplicateKey = $state<string | undefined>();
  /** Each rise of `r` opens the comment box and puts the caret in it (§7). */
  let commentRequest = $state(0);

  const side = $derived(preferences.value('issue_panel_side'));
  const width = $derived(dragged ?? preferences.value('issue_panel_width'));
  const blocks = $derived(issue.fields.filter(isBlock));
  // Keyed by the ID only: edits of the issue keep the loaded pages of its stream.
  const issueId = $derived(issue.id);
  const me = $derived(currentUser());
  const author = $derived(me ? asAuthor(me) : null);
  const stream = $derived(page ? new Stream(issueId, page.activity, page.focusComment) : null);
  const files = $derived(page ? new IssueFiles(issueId, page.attachments, upload) : null);
  const links = $derived(page ? new IssueLinksState(issueId, page.links) : null);
  const canLink = $derived(links?.canLink ?? false);
  const canCreate = $derived(can('issue.create', issue.project.id));
  const canAttach = $derived(issue.can.update && can('attachment.create', issue.project.id));
  // Follows the issue while the page is open (ISS-048).
  const live = $derived(new IssueLive(issueId, { stream, links, files }));
  $effect(() => {
    const current = live;
    const source = connection;
    // The connection's status is state: reading it must not resubscribe.
    return untrack(() => current.follow(source));
  });
  let refreshing = $state(false);
  let picker: HTMLInputElement | undefined = $state();
  setIssueFiles(() => files);
  const crumbs = $derived([
    { label: issue.project.name, href: projectHref.overview(issue.project) },
    { label: m.issues(), href: projectIssuesHref(issue.project) },
  ]);
  const title = $derived(
    [issue.readable_id, issue.summary || m.noSummary()].filter(Boolean).join(' '),
  );

  $effect(() => {
    if (requested !== undefined && issue.readable_id && requested !== issue.readable_id) {
      // A deep link keeps its comment (CNT-007).
      const search = page?.focusComment ? `?comment=${String(page.focusComment)}` : '';
      // issueHref resolves the address.
      // eslint-disable-next-line svelte/no-navigation-without-resolve
      void goto(`${issueHref(issue)}${search}`, { replace: true, shallow: true, state: {} });
    }
  });

  function resize(value: number, done: boolean): void {
    if (!done) {
      dragged = value;
      return;
    }
    dragged = null;
    preferences.set({ issue_panel_width: value });
  }

  function openCommands(withLast: boolean): void {
    commandPreset = withLast ? (recentCommands.last() ?? '') : undefined;
    commanding = true;
  }

  // ---- The keys of the issue view (step 2.2 §7, ISS-047) ----

  /** The bindings of the issue view alone (§4): the shell's listener has already taken the
   * global ones and the letters of `commands_on_typing`, so global is emptied here — the page
   * never outvotes the shell. */
  const ISSUE_TABLES = { ...tables, global: [] };
  const keys = createMatchState(isTypingFocus);

  /** The keys of the page (§4): F2 stays from before the framework; the rest match through the
   * framework over the issue context. While the edit form is shown, it holds the keys; an open
   * dialog or popover owns them. */
  function keydown(event: KeyboardEvent): void {
    if (event.defaultPrevented) return;
    if (document.querySelector('dialog[open], :popover-open')) return;
    if (event.key === 'F2' && !editing && issue.can.update) {
      event.preventDefault();
      editing = true;
      return;
    }
    if (editing) return;
    const binding = match(event, ISSUE_TABLES, 'issue', keys);
    if (binding === null) return;
    if (runIssueKey(binding.id)) event.preventDefault();
  }

  /** Runs a binding of the issue context; false lets the key fall through untouched. */
  function runIssueKey(id: string): boolean {
    switch (id) {
      case 'issue.edit':
        if (!issue.can.update) return false;
        editing = true;
        return true;
      case 'issue.comment':
        if (stream === null || author === null || !can('comment.create', issue.project.id)) {
          return false;
        }
        commentRequest += 1;
        return true;
      case 'issue.history':
        if (stream === null) return false;
        // The tab choice of HIST-008 behind a key: away from «Все» hides the changes, back
        // from the comments-only tab shows them again.
        runAction(stream.setView(stream.view === 'all' ? 'comments' : 'all'));
        return true;
      case 'issue.comments':
        if (stream === null) return false;
        runAction(stream.setView(stream.view === 'all' ? 'changes' : 'all'));
        return true;
      case 'issue.assign-me':
        assignToMe(issue);
        return true;
      case 'issue.star':
        setStar(issue, !issue.starred).catch((error: unknown) => {
          showError(error);
        });
        return true;
      case 'issue.attach':
        if (files === null || !canAttach) return false;
        picker?.click();
        return true;
      case 'issue.tags':
        if (!issue.can.update) return false;
        tagging = true;
        return true;
      default:
        return false;
    }
  }

  function runAction(action: Promise<void>): void {
    action.catch((error: unknown) => {
      showError(error);
    });
  }

  // The issue context (§7): the lifecycle, not an effect — `enter` mutates the registry's
  // reactive stack. The open issue is the focused entity (the copy trio), the command dialog is
  // claimed for its ⌘/Ctrl+Alt+C and the letters of `commands_on_typing`, and «Создать» gains
  // the issue's subtask and linked issue.
  onMount(() => {
    enter('issue');
    setFocusedEntity(() => entityOf());
    setIssueCreateExtra(() => createEntries());
    const releaseCommand = claim('command', (preset?: string) => {
      // A letter never opens the dialog over the edit form (the rule of 2.1); the combo may.
      if (preset !== undefined && editing) return false;
      commandPreset = preset;
      commanding = true;
    });
    const releaseRepeat = claim('command.repeat', () => {
      if (recentCommands.last() === null) return false;
      openCommands(true);
    });
    return () => {
      leave('issue');
      setFocusedEntity(null);
      setIssueCreateExtra(null);
      releaseCommand();
      releaseRepeat();
    };
  });

  function entityOf(): FocusedEntity | null {
    return {
      id: issue.readable_id ?? String(issue.id),
      href: issueHref(issue),
      summary: issue.summary,
    };
  }

  /** What the shell's «Создать» menu adds while this issue is open (§7): its subtask and a
   * linked issue, for those who may — the same actions as the «Ещё» menu. */
  function createEntries(): MenuEntry[] {
    if (!canLink || !canCreate) return [];
    return [
      {
        type: 'action',
        label: m.newSubtask(),
        icon: ListTree,
        onSelect: () => {
          newSubtask();
        },
      },
      {
        type: 'action',
        label: m.newLinkedIssue(),
        icon: SquarePlus,
        onSelect: () => {
          startLinking('new');
        },
      },
    ];
  }

  function untag(tag: { id: number; name: string; color: number | null }): void {
    setTag(issue, tag, false).catch((error: unknown) => {
      showError(error);
    });
  }

  /** Attaches files from the drop, a paste, the menu or the section (CNT-017). */
  function attach(list: File[]): void {
    if (!files || !canAttach || list.length === 0) return;
    files
      .attach(list)
      .then((done) => {
        if (done.length > 0)
          toasts.show({ tone: 'success', message: m.filesAttached({ count: done.length }) });
      })
      // Each failed file says why in the section.
      .catch(() => undefined);
  }

  function startLinking(mode: 'link' | 'new' | 'duplicate'): void {
    linkMode = mode;
    linking = true;
    if (mode !== 'duplicate') return;
    linkTypes().then(
      (list) => {
        const name = duplicateName(list);
        duplicateKey = name ? nameKey(name) : undefined;
      },
      (error: unknown) => showError(error),
    );
  }

  /** A new issue that starts linked to this one (ISS-009). */
  function newLinked(link: NewIssueLink): void {
    openNewIssue({ project: issue.project.key, link });
  }

  function newSubtask(): void {
    linkTypes().then(
      (list) => {
        const link = subtaskLink(list, issue.id);
        if (link) newLinked(link);
      },
      (error: unknown) => showError(error),
    );
  }

  $effect(() => {
    if (!moving) moveTarget = null;
  });

  function refresh(): void {
    refreshing = true;
    live
      .refresh()
      .catch((error: unknown) => showError(error))
      .finally(() => (refreshing = false));
  }
</script>

<svelte:head>
  {#if !embedded}<title>{title} · {brand()}</title>{/if}
</svelte:head>
<svelte:window onkeydown={embedded ? undefined : keydown} />

<div class={['page', { embedded }]}>
  {#if !embedded}
    <Breadcrumbs
      label={m.breadcrumbs()}
      items={crumbs}
      back={{
        label: m.backToIssues({ key: issue.project.key }),
        href: projectIssuesHref(issue.project),
      }}
    />
  {/if}
  {#if live.stale}
    <Notice tone="warning">
      <span class="line">
        {m.mayBeOutdated()}
        <Button size="sm" loading={refreshing} onclick={refresh}>{m.refreshPage()}</Button>
      </span>
    </Notice>
  {/if}
  <MetaStrip {issue}>
    {#snippet corner()}
      {#if embedded && onclose}
        <!-- issueHref resolves the address. -->
        <!-- eslint-disable svelte/no-navigation-without-resolve -->
        <a
          class="open"
          href={issueHref(issue)}
          target="_blank"
          rel="noopener"
          aria-label={m.openIssuePage()}
          {@attach tooltip(m.openIssuePage())}
        >
          <ExternalLink size={18} aria-hidden="true" />
        </a>
        <!-- eslint-enable svelte/no-navigation-without-resolve -->
        <IconButton icon={X} label={common.close()} onclick={onclose} />
      {/if}
    {/snippet}
  </MetaStrip>
  <div class={['frame', side]} style:--panel-width="{width / 10}rem">
    <div class="main">
      <div class="title">
        <IssueHeader {issue}>
          {#snippet actions()}
            {#if issue.can.update && !editing}
              <IconButton icon={Pencil} label={m.edit()} onclick={() => (editing = true)} />
            {/if}
            {#if !editing}
              <VisibilityMenu {issue} />
              {#if issue.can.update}
                <TagPicker {issue} bind:open={tagging} icon />
              {/if}
            {/if}
            {#if canLink && !editing}
              <IconButton
                icon={Link2}
                label={m.linkAction()}
                onclick={() => startLinking('link')}
              />
            {/if}
            <IssueActions
              {issue}
              ondelete={() => (deleting = true)}
              onattach={files && canAttach ? () => picker?.click() : undefined}
              oncommands={() => openCommands(false)}
              onrepeat={recentCommands.last() !== null ? () => openCommands(true) : undefined}
              onsubtask={canLink && canCreate ? newSubtask : undefined}
              onnewlinked={canLink && canCreate ? () => startLinking('new') : undefined}
              onduplicate={canLink ? () => startLinking('duplicate') : undefined}
            />
          {/snippet}
          {#snippet below()}
            {#if issue.tags.length > 0}
              <ul class="tags" aria-label={m.tags()}>
                {#each issue.tags as tag (tag.id)}
                  <li>
                    <TagChip {tag} onremove={issue.can.update ? () => untag(tag) : undefined} />
                  </li>
                {/each}
              </ul>
            {/if}
            {#if issue.parent}
              <p class="parent"><span>{m.subtaskOf()}</span> <IssueRef issue={issue.parent} /></p>
            {/if}
          {/snippet}
        </IssueHeader>
      </div>
      <div class="content">
        {#if editing}
          <EditMode {issue} onclose={() => (editing = false)} />
        {:else}
          <section class="block" aria-labelledby="{id}-description">
            <h2 class="sr-only" id="{id}-description">{m.description()}</h2>
            {#if issue.description}
              <MarkdownView html={issue.description_html} refs={issue.refs} headingLevel={3} />
            {:else}
              <p class="muted">{m.noDescription()}</p>
            {/if}
          </section>
          {#each blocks as entry (entry.field.id)}
            <section class="block" aria-labelledby="{id}-field-{entry.field.id}">
              <h2 id="{id}-field-{entry.field.id}">{entry.field.name}</h2>
              {#if isEmpty(entry) || entry.html === null}
                <p class="muted">{entry.empty_label}</p>
              {:else}
                <MarkdownView html={entry.html} refs={issue.refs} headingLevel={3} />
              {/if}
            </section>
          {/each}
        {/if}
        {#if files}
          <AttachmentsSection {issue} {files} {canAttach} onattach={attach} />
        {/if}
        {#if links && links.total > 0}
          <LinksPanel {issue} {links} />
        {/if}
        {#if stream}
          <ActivityStream {issue} {stream}>
            {#snippet box()}
              {#if author && can('comment.create', issue.project.id)}
                <CommentBox
                  {issue}
                  {author}
                  openRequest={commentRequest}
                  onposted={() => stream?.posted().catch((error: unknown) => showError(error))}
                />
              {/if}
            {/snippet}
          </ActivityStream>
        {/if}
      </div>
    </div>
    <div class="handle">
      <Resizer
        value={width}
        min={PANEL_MIN}
        max={PANEL_MAX}
        {side}
        label={m.panelWidth()}
        onchange={resize}
      />
    </div>
    <aside class="side" aria-label={m.fields()}>
      <FieldPanel
        {issue}
        onmove={(project: Project) => {
          moveTarget = project;
          moving = true;
        }}
        boards={page?.boards}
      />
    </aside>
  </div>
</div>

{#if files}
  <Lightbox {files} />
  {#if canAttach}
    <PageDrop onfiles={attach} />
    <input
      class="sr-only"
      type="file"
      multiple
      tabindex="-1"
      aria-hidden="true"
      bind:this={picker}
      onchange={(event) => {
        const input = event.currentTarget;
        attach([...(input.files ?? [])]);
        input.value = '';
      }}
    />
  {/if}
{/if}
{#if links}
  <AddLinkDialog
    bind:open={linking}
    {issue}
    {links}
    mode={linkMode === 'new' ? 'new' : 'link'}
    preset={linkMode === 'duplicate' ? duplicateKey : undefined}
    title={linkMode === 'new'
      ? m.newLinkedTitle()
      : linkMode === 'duplicate'
        ? m.markDuplicateTitle()
        : m.linkIssuesTitle()}
    onnewform={canCreate ? newLinked : undefined}
  />
{/if}
<MoveDialog bind:open={moving} {issue} preset={moveTarget} />
<DeleteDialog bind:open={deleting} {issue} />
<!-- The board's popup embeds this page: its «Ещё» offers «Команды…» too, so the dialog renders
     on both surfaces (the owner's report of 2026-11-15). -->
<CommandDialog
  bind:open={commanding}
  preset={commandPreset}
  issues={[issue]}
  onclose={() => {
    commanding = false;
  }}
/>

<style>
  .page {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    max-width: var(--content-max);
    padding: var(--space-4) var(--space-6) var(--space-12);
  }

  .page.embedded {
    padding: 0;
  }

  .line {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-2);
  }

  /* Snippet content of the head: tags and the parent line sit tight under the summary. */
  .parent {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: var(--space-1_5);
    margin: 0;
    color: var(--color-text-muted);
  }

  .tags {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-1_5);
    margin: 0;
    padding: 0;
    list-style: none;
  }

  /* The metadata line spans the full width; the frame under it holds the main column (the title
     cluster tight above the content) beside the panel in its own tone, with the Resizer handle as
     the gutter between the columns. The panel's height never pushes the content down. */
  .frame {
    display: flex;
    align-items: stretch;
  }

  .frame.start {
    flex-direction: row-reverse;
  }

  .main {
    display: flex;
    flex: 1 1 0;
    flex-direction: column;
    gap: var(--space-3);
    min-width: 0;
  }

  .content {
    display: flex;
    flex-direction: column;
    gap: var(--space-6);
    min-width: 0;
  }

  .block {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }

  .block h2 {
    font-size: var(--text-lg);
  }

  .muted {
    color: var(--color-text-muted);
  }

  .handle {
    display: flex;
  }

  /* The panel's own tone and the border facing the content mark it as one zone. */
  .side {
    flex: none;
    align-self: stretch;
    width: var(--panel-width);
    padding: var(--space-2) var(--space-3);
    border-inline-start: 1px solid var(--color-border);
    background: var(--color-surface-sunken);
  }

  .frame.start .side {
    border-inline-start: 0;
    border-inline-end: 1px solid var(--color-border);
  }

  /* The popup's corner: «Открыть страницей» beside the close button, level with the ID. */
  .open {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: var(--control-md);
    height: var(--control-md);
    border-radius: var(--radius-md);
    color: var(--color-text-muted);
  }

  .open:hover {
    background: var(--color-hover);
    color: var(--color-text);
  }

  @media (width < 64rem) {
    .side {
      width: min(var(--panel-width), 30%);
    }
  }

  /* Phones: the main column opens up so the fields sit between the title cluster and the content. */
  @media (width < 48rem) {
    .page:not(.embedded) {
      padding: var(--space-2) var(--space-4) var(--space-12);
    }

    .frame,
    .frame.start {
      flex-direction: column;
      gap: var(--space-4);
    }

    .main {
      display: contents;
    }

    .content {
      order: 1;
    }

    .side,
    .frame.start .side {
      width: auto;
      padding: var(--space-3);
      border: 0;
      border-radius: var(--radius-md);
    }

    .handle {
      display: none;
    }
  }
</style>
