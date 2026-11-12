<script lang="ts">
  // The issue page (ISS-001, ISS-016, ISS-020, UI-005), read form: breadcrumbs, the header, the
  // metadata strip, then the content beside the field panel. The panel's side, width and collapsed
  // state are personal settings (UI-018); on phones the panel is a section above the content.
  // Opened by a former or internal ID, the page puts the current ID in the address (ISS-003).
  // «Изменить» or F2 turns the header's content into the edit form (ISS-018); tags, moving and
  // deleting are in the header and the field panel.
  import { goto } from '$app/navigation';
  import Link2 from '@lucide/svelte/icons/link-2';
  import Pencil from '@lucide/svelte/icons/pencil';
  import { untrack } from 'svelte';
  import { MediaQuery } from 'svelte/reactivity';

  import type { Issue, NewIssueLink } from '#lib/api/schema.gen.ts';
  import { MarkdownView } from '#lib/features/content/index.ts';
  import { projectHref } from '#lib/features/projects/index.ts';
  import { can, currentUser, preferences } from '#lib/session/index.ts';
  import { NARROW } from '#lib/theme/breakpoints.ts';
  import Breadcrumbs from '#lib/ui/Breadcrumbs.svelte';
  import Button from '#lib/ui/Button.svelte';
  import Notice from '#lib/ui/Notice.svelte';
  import Resizer from '#lib/ui/Resizer.svelte';
  import { showError, toasts } from '#lib/ui/toast.svelte.ts';

  import ActivityStream from '../activity/ActivityStream.svelte';
  import { ActivityStream as Stream } from '../activity/activity.svelte.ts';
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
  import { openNewIssue } from '../new/open.svelte.ts';
  import * as m from '../messages.ts';
  import { setTag } from '../actions.ts';
  import TagChip from '../tags/TagChip.svelte';
  import TagPicker from '../tags/TagPicker.svelte';
  import DeleteDialog from './DeleteDialog.svelte';
  import EditMode from './EditMode.svelte';
  import IssueActions from './IssueActions.svelte';
  import IssueHeader from './IssueHeader.svelte';
  import { IssueLive, type LiveSource } from './live.svelte.ts';
  import MetaStrip from './MetaStrip.svelte';
  import MoveDialog from './MoveDialog.svelte';

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
  let deleting = $state(false);
  /** The link dialog and what it opened for (LNK-005, ISS-009, LNK-015). */
  let linking = $state(false);
  let linkMode = $state<'link' | 'new' | 'duplicate'>('link');
  let duplicateKey = $state<string | undefined>();
  const narrow = new MediaQuery(NARROW, false);

  const side = $derived(preferences.value('issue_panel_side'));
  const collapsed = $derived(preferences.value('issue_panel_collapsed'));
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

  /** F2 starts editing (ISS-018), unless a dialog or another form has the keys. */
  function keydown(event: KeyboardEvent): void {
    if (event.key !== 'F2' || event.defaultPrevented || editing || !issue.can.update) return;
    if (document.querySelector('dialog[open], :popover-open')) return;
    event.preventDefault();
    editing = true;
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

  function refresh(): void {
    refreshing = true;
    live
      .refresh()
      .catch((error: unknown) => showError(error))
      .finally(() => (refreshing = false));
  }

  function toggle(): void {
    preferences.set({ issue_panel_collapsed: collapsed ? null : true });
  }
</script>

<svelte:head>
  {#if !embedded}<title>{title} · trackflow</title>{/if}
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
  <IssueHeader {issue}>
    {#snippet actions()}
      {#if issue.can.update && !editing}
        <Button icon={Pencil} onclick={() => (editing = true)}>{m.edit()}</Button>
        <TagPicker {issue} bind:open={tagging} bare={narrow.current} />
      {/if}
      {#if canLink && !editing && !narrow.current}
        <Button icon={Link2} onclick={() => startLinking('link')}>{m.linkAction()}</Button>
      {/if}
      <IssueActions
        {issue}
        onaddtag={issue.can.update && narrow.current ? () => (tagging = true) : undefined}
        ondelete={() => (deleting = true)}
        onattach={files && canAttach ? () => picker?.click() : undefined}
        onlink={canLink && narrow.current ? () => startLinking('link') : undefined}
        onsubtask={canLink && canCreate ? newSubtask : undefined}
        onnewlinked={canLink && canCreate ? () => startLinking('new') : undefined}
        onduplicate={canLink ? () => startLinking('duplicate') : undefined}
      />
    {/snippet}
  </IssueHeader>
  {#if issue.parent}
    <p class="parent"><span>{m.subtaskOf()}</span> <IssueRef issue={issue.parent} /></p>
  {/if}
  <MetaStrip {issue} />
  {#if issue.tags.length > 0}
    <ul class="tags" aria-label={m.tags()}>
      {#each issue.tags as tag (tag.id)}
        <li>
          <TagChip {tag} onremove={issue.can.update ? () => untag(tag) : undefined} />
        </li>
      {/each}
    </ul>
  {/if}

  <div class={['body', side, { collapsed }]} style:--panel-width="{width / 10}rem">
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
                onposted={() => stream?.posted().catch((error: unknown) => showError(error))}
              />
            {/if}
          {/snippet}
        </ActivityStream>
      {/if}
    </div>
    {#if !collapsed}
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
    {/if}
    <aside class="side" aria-label={m.fields()}>
      <FieldPanel
        {issue}
        {collapsed}
        {side}
        ontoggle={toggle}
        onmove={() => (moving = true)}
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
<MoveDialog bind:open={moving} {issue} />
<DeleteDialog bind:open={deleting} {issue} />

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

  .parent {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: var(--space-1_5);
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

  .body {
    display: flex;
    align-items: stretch;
    gap: var(--space-2);
    margin-block-start: var(--space-3);
  }

  .body.start {
    flex-direction: row-reverse;
  }

  .content {
    display: flex;
    flex: 1 1 0;
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

  .side {
    flex: none;
    width: var(--panel-width);
    padding-inline: var(--space-2);
  }

  .collapsed .side {
    width: auto;
  }

  @media (width < 64rem) {
    .side {
      width: min(var(--panel-width), 30%);
    }
  }

  /* Phones: the fields as a section between the header and the content. */
  @media (width < 48rem) {
    .page:not(.embedded) {
      padding: var(--space-2) var(--space-4) var(--space-12);
    }

    .body,
    .body.start {
      flex-direction: column;
      gap: var(--space-6);
    }

    .side,
    .collapsed .side {
      order: -1;
      width: auto;
      padding-inline: 0;
    }

    .handle {
      display: none;
    }
  }
</style>
