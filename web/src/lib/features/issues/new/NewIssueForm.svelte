<script lang="ts">
  // The new-issue form (ISS-005…ISS-011), the same in the dialog and on the page: the summary
  // (focused), the description with files, the text fields, the draft's field panel with the
  // project at its top, tags, visibility and the star. Everything is saved to the draft as it
  // changes (`DraftSession`). «Создать» or Ctrl/⌘+Enter publishes it; the menu beside it creates
  // and starts another form, empty or with the same texts. A refusal for required fields marks
  // their rows and focuses the first (FLD-057).
  import { goto } from '$app/navigation';
  import ChevronDown from '@lucide/svelte/icons/chevron-down';
  import Maximize2 from '@lucide/svelte/icons/maximize-2';
  import Star from '@lucide/svelte/icons/star';
  import { tick } from 'svelte';

  import { ApiError } from '#lib/api/client.ts';
  import type { Audience, Issue, Project, ProjectRef } from '#lib/api/schema.gen.ts';
  import { MarkdownEditor } from '#lib/features/content/index.ts';
  import { refusalText } from '#lib/features/fields/index.ts';
  import { announce } from '#lib/ui/announce.svelte.ts';
  import Button from '#lib/ui/Button.svelte';
  import IconButton from '#lib/ui/IconButton.svelte';
  import Menu from '#lib/ui/Menu.svelte';
  import Notice from '#lib/ui/Notice.svelte';
  import TextField from '#lib/ui/TextField.svelte';
  import { showError, toasts } from '#lib/ui/toast.svelte.ts';

  import { moveIssue, previewMove, setStar, setTag } from '../actions.ts';
  import AudiencePicker from '../audience/AudiencePicker.svelte';
  import { isRestricted } from '../audience/audience.ts';
  import FieldPanel from '../fields/FieldPanel.svelte';
  import { isMissing } from '../fields/field-values.ts';
  import { issueHref } from '../issues.ts';
  import AddLinkDialog from '../links/AddLinkDialog.svelte';
  import LinksPanel from '../links/LinksPanel.svelte';
  import { IssueLinksState } from '../links/links.svelte.ts';
  import * as m from '../messages.ts';
  import TagChip from '../tags/TagChip.svelte';
  import TagPicker from '../tags/TagPicker.svelte';
  import type { DraftSession } from './drafts.svelte.ts';
  import DraftsMenu from './DraftsMenu.svelte';
  import ProjectChoice from './ProjectChoice.svelte';

  interface Props {
    session: DraftSession;
    /** The entry point's project key: first in the picker. */
    first?: string;
    /** The form is done: published (the issue to open), or discarded from `project`. */
    onfinish: (created?: Issue, project?: ProjectRef) => void;
    /** The dialog's «Открыть на всю страницу». */
    onexpand?: () => void;
  }

  let { session, first, onfinish, onexpand }: Props = $props();

  const id = $props.id();
  const issue = $derived(session.issue);
  const textFields = $derived(
    issue?.fields.filter((entry) => entry.field.kind === 'text' && entry.editable) ?? [],
  );
  const canCreate = $derived(session.summary.trim() !== '');

  let form = $state<HTMLFormElement>();
  let summaryInput = $state<HTMLInputElement>();
  let busy = $state(false);
  let moving = $state(false);
  let problem = $state<string | null>(null);
  let summaryError = $state<string | null>(null);
  /** Fields the last refused publish named. */
  let invalid = $state<number[]>([]);
  /** Fields the last change of project cleared. */
  let dropped = $state<string[]>([]);
  let uploads = $state<number[]>([]);
  let restricted = $state(false);
  let linking = $state(false);

  /** The draft's links (LNK-007): those it started with, then those added here. */
  const draftLinks = $derived(
    session.id === null
      ? null
      : new IssueLinksState(session.id, { groups: [], can_link: true }, false),
  );

  $effect(() => {
    draftLinks?.reload().catch(failed);
  });

  // An empty draft is deleted on close; its links count as content.
  $effect(() => {
    if (draftLinks?.loaded) session.links = draftLinks.total;
  });

  const statusText = $derived(
    session.status === 'saving'
      ? m.draftSaving()
      : session.status === 'saved'
        ? m.draftSaved()
        : session.status === 'failed'
          ? m.draftNotSaved()
          : '',
  );

  // A new draft (opened, or after «Создать и открыть новую») starts at the summary.
  $effect(() => {
    if (session.id === null) return;
    restricted = isRestricted(session.issue?.audience ?? { users: [], groups: [] });
    problem = null;
    summaryError = null;
    invalid = [];
    dropped = [];
    void tick().then(() => summaryInput?.focus());
  });

  function failed(error: unknown): void {
    showError(error);
  }

  async function pick(project: Project): Promise<void> {
    const draft = issue;
    if (!draft) return;
    moving = true;
    problem = null;
    try {
      const preview = await previewMove(draft.id, project.id);
      await session.flush();
      await moveIssue(draft.id, project.id);
      dropped = preview.changes.map((change) => change.field.name);
      invalid = [];
    } catch (error) {
      problem = refusalText(error);
    } finally {
      moving = false;
    }
  }

  function changeAudience(next: Audience): void {
    if (restricted && next.users.length === 0 && next.groups.length === 0) return;
    session.setAudience(next).catch(failed);
  }

  /** Which fields a refusal names as missing; the empty required ones when it names none. */
  function missingOf(error: ApiError, draft: Issue): number[] {
    const named = (error.body?.fields ?? [])
      .map((entry) => /^fields\.(\d+)$/.exec(entry.path)?.[1])
      .filter((value): value is string => value !== undefined)
      .map(Number);
    return named.length > 0
      ? named
      : draft.fields.filter((entry) => isMissing(entry)).map((entry) => entry.field.id);
  }

  async function create(variant: 'open' | 'another' | 'same' = 'open'): Promise<void> {
    const draft = issue;
    if (!draft || busy || !canCreate) return;
    busy = true;
    problem = null;
    summaryError = null;
    invalid = [];
    const seed = {
      project: draft.project.id,
      summary: session.summary,
      description: session.description,
    };
    try {
      const created = await session.publish();
      const readable = created.readable_id ?? String(created.id);
      toasts.show({
        tone: 'success',
        message: m.issueCreated({ id: readable }),
        action: {
          label: m.open(),
          // issueHref resolves the address.
          // eslint-disable-next-line svelte/no-navigation-without-resolve
          run: () => goto(issueHref(created)),
        },
      });
      if (variant === 'open') {
        onfinish(created);
        return;
      }
      uploads = [];
      await session.start(variant === 'same' ? seed : { project: seed.project });
    } catch (error) {
      if (error instanceof ApiError && error.code === 'invalid_request') {
        const paths = (error.body?.fields ?? []).map((entry) => entry.path);
        if (paths.includes('summary')) summaryError = refusalText(error);
        invalid = paths.includes('summary') && paths.length === 1 ? [] : missingOf(error, draft);
        problem = invalid.length > 0 ? m.requiredMissing() : refusalText(error);
        announce(problem, 'assertive');
        await tick();
        form?.querySelector<HTMLElement>('[aria-invalid="true"], [data-invalid]')?.focus();
      } else {
        problem = refusalText(error);
      }
    } finally {
      busy = false;
    }
  }

  async function discard(): Promise<void> {
    busy = true;
    const project = issue?.project;
    try {
      await session.discard();
      onfinish(undefined, project);
    } catch (error) {
      problem = refusalText(error);
    } finally {
      busy = false;
    }
  }

  async function switchTo(draft: number): Promise<void> {
    await session.close().catch(failed);
    uploads = [];
    const found = await session.open(draft).catch((error: unknown) => {
      failed(error);
      return false;
    });
    if (!found) onfinish();
  }

  async function deleteAll(project: number): Promise<void> {
    await session.deleteAll();
    toasts.show({ tone: 'success', message: m.draftsDeleted() });
    uploads = [];
    await session.start({ project }).catch(failed);
  }

  function keydown(event: KeyboardEvent): void {
    if (event.key !== 'Enter' || !(event.ctrlKey || event.metaKey) || event.defaultPrevented) {
      return;
    }
    event.preventDefault();
    void create();
  }
</script>

{#if issue}
  <!-- Ctrl/⌘+Enter anywhere in the form creates the issue. -->
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <form
    class="form"
    aria-label={m.newIssueForm()}
    aria-describedby="{id}-hint"
    bind:this={form}
    onsubmit={(event) => {
      event.preventDefault();
      void create();
    }}
    onkeydown={keydown}
    onfocusout={() => void session.flush().catch(() => undefined)}
  >
    <div class="bar">
      <p class="status" role="status">{statusText}</p>
      <DraftsMenu
        current={session.id}
        onchoose={(draft: number) => void switchTo(draft)}
        ondeleteall={() => deleteAll(issue.project.id)}
      />
      {#if onexpand}
        <IconButton icon={Maximize2} label={m.openFullPage()} onclick={onexpand} />
      {/if}
    </div>

    {#if problem}<Notice tone="danger">{problem}</Notice>{/if}

    <div class="layout">
      <div class="main">
        <TextField
          label={m.summary()}
          bind:value={session.summary}
          bind:input={summaryInput}
          error={summaryError}
          required
          maxlength={255}
          oninput={() => {
            session.typed();
          }}
        />
        <MarkdownEditor
          label={m.description()}
          bind:value={session.description}
          issue={issue.id}
          rows={8}
          oninput={() => {
            session.typed();
          }}
          upload={(file: File) => session.upload(file)}
          bind:uploads
        />
        {#each textFields as entry (entry.field.id)}
          <MarkdownEditor
            label={entry.field.name}
            bind:value={
              () => session.texts[entry.field.id] ?? entry.text ?? '',
              (text: string) => {
                session.texts[entry.field.id] = text;
              }
            }
            issue={issue.id}
            rows={4}
            oninput={() => {
              session.typedField(entry.field.id);
            }}
          />
        {/each}

        <div class="extras">
          {#if issue.tags.length > 0}
            <ul class="tags" aria-label={m.tags()}>
              {#each issue.tags as tag (tag.id)}
                <li>
                  <TagChip
                    {tag}
                    onremove={() => {
                      setTag(issue, tag, false).catch(failed);
                    }}
                  />
                </li>
              {/each}
            </ul>
          {/if}
          <div class="row">
            <TagPicker {issue} />
            <Button
              icon={Star}
              aria-pressed={issue.starred}
              onclick={() => {
                setStar(issue, !issue.starred).catch(failed);
              }}>{m.starForMe()}</Button
            >
          </div>
          <AudiencePicker
            project={issue.project.id}
            reporter={issue.reporter}
            bind:value={() => issue.audience, changeAudience}
            bind:restricted
          />
        </div>
        {#if draftLinks}
          <LinksPanel {issue} links={draftLinks} onadd={() => (linking = true)} />
        {/if}
      </div>

      <aside class="side" aria-label={m.fields()}>
        <FieldPanel
          {issue}
          collapsed={false}
          side="end"
          ontoggle={() => undefined}
          fixed
          {invalid}
          loadBoards
        >
          {#snippet project()}
            <ProjectChoice value={issue.project} {first} disabled={moving} onpick={pick} />
          {/snippet}
        </FieldPanel>
        {#if dropped.length > 0}
          <Notice tone="warning">{m.movedDropped({ names: dropped.join(', ') })}</Notice>
        {/if}
      </aside>
    </div>

    <p class="hint" id="{id}-hint">{m.createHint()}</p>
    <div class="buttons">
      <Button variant="ghost" class="discard" disabled={busy} onclick={discard}
        >{m.discardDraft()}</Button
      >
      <div class="create">
        <Button type="submit" variant="primary" loading={busy} disabled={!canCreate}
          >{m.createIssue()}</Button
        >
        <Menu
          label={m.createVariants()}
          placement={{ side: 'top', align: 'end' }}
          items={[
            {
              type: 'action',
              label: m.createAndNew(),
              disabled: !canCreate,
              onSelect: () => void create('another'),
            },
            {
              type: 'action',
              label: m.createAndNewSameText(),
              disabled: !canCreate,
              onSelect: () => void create('same'),
            },
          ]}
        >
          {#snippet trigger(props)}
            <Button
              icon={ChevronDown}
              variant="primary"
              aria-label={m.createVariants()}
              disabled={busy || !canCreate}
              {...props}
            />
          {/snippet}
        </Menu>
      </div>
    </div>
  </form>
  {#if draftLinks}
    <!-- Outside the form: Enter in its search must not create the issue. -->
    <AddLinkDialog bind:open={linking} {issue} links={draftLinks} />
  {/if}
{/if}

<style>
  .form {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
    min-width: 0;
  }

  .bar {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }

  .status {
    flex: 1;
    min-width: 0;
    color: var(--color-text-muted);
    font-size: var(--text-sm);
  }

  .layout {
    display: flex;
    gap: var(--space-6);
  }

  .main {
    display: flex;
    flex: 1 1 0;
    flex-direction: column;
    gap: var(--space-4);
    min-width: 0;
  }

  .side {
    display: flex;
    flex: none;
    flex-direction: column;
    gap: var(--space-3);
    width: 16rem;
  }

  .extras {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }

  .row {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
  }

  .tags {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-1_5);
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .hint {
    color: var(--color-text-muted);
    font-size: var(--text-sm);
  }

  .buttons {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2);
  }

  .buttons :global(.discard) {
    color: var(--color-danger);
  }

  .create {
    display: flex;
    gap: var(--space-0_5);
  }

  /* Phones: the fields as a section above the texts. */
  @media (width < 48rem) {
    .layout {
      flex-direction: column;
    }

    .side {
      order: -1;
      width: auto;
    }
  }
</style>
