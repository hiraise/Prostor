<script lang="ts">
  // «Связать» (LNK-005, LNK-006, LNK-015): a link name of any type, either side, then issues found
  // by a search — the person's recent issues before they type — chosen with checkboxes and linked
  // together. Pairs linked before are named in a notice. A typed text can become a new issue in the
  // current project, created already linked; «Открыть полную форму» starts the new-issue form with
  // the link. In the `new` mode (ISS-009, «Новая связанная задача…») only the name is chosen and
  // the form opens.
  import type { Issue, NewIssueLink } from '#lib/api/schema.gen.ts';
  import { refusalText } from '#lib/features/fields/index.ts';
  import * as common from '#lib/i18n/messages/common.ts';
  import { can } from '#lib/session/index.ts';
  import { announce } from '#lib/ui/announce.svelte.ts';
  import Button from '#lib/ui/Button.svelte';
  import Dialog from '#lib/ui/Dialog.svelte';
  import Notice from '#lib/ui/Notice.svelte';
  import OptionList from '#lib/ui/OptionList.svelte';
  import Select from '#lib/ui/Select.svelte';
  import { describeError, toasts } from '#lib/ui/toast.svelte.ts';

  import IssueRef from '../IssueRef.svelte';
  import { issueRow } from '../issues.ts';
  import * as m from '../messages.ts';
  import {
    createLinked,
    findIssues,
    linkNames,
    linkTypes,
    nameKey,
    newIssueLink,
    recentIssues,
    type IssueLinksState,
    type LinkName,
  } from './links.svelte.ts';

  interface Props {
    open?: boolean;
    issue: Issue;
    links: IssueLinksState;
    /** `new`: choose the name and open the new-issue form. */
    mode?: 'link' | 'new';
    /** The name chosen at first, as `type:side` (the duplicate name for LNK-015). */
    preset?: string;
    title?: string;
    /** Opens the new-issue form with this link (ISS-009); no such button without it. */
    onnewform?: (link: NewIssueLink) => void;
  }

  let {
    open = $bindable(false),
    issue,
    links,
    mode = 'link',
    preset,
    title = m.linkIssuesTitle(),
    onnewform,
  }: Props = $props();

  let names = $state<LinkName[]>([]);
  let chosenName = $state('');
  let query = $state('');
  let found = $state<Issue[]>([]);
  let chosen = $state<Issue[]>([]);
  let loading = $state(false);
  let busy = $state(false);
  let problem = $state<string | null>(null);
  let existing = $state<string | null>(null);

  const name = $derived(names.find((entry) => nameKey(entry) === chosenName));
  const typed = $derived(query.trim());
  const options = $derived([
    ...chosen,
    ...found.filter(
      (other) => other.id !== issue.id && !chosen.some((picked) => picked.id === other.id),
    ),
  ]);
  const canCreate = $derived(can('issue.create', issue.project.id));

  // Each opening starts clean, with the names loaded once.
  $effect(() => {
    if (!open) return;
    query = '';
    chosen = [];
    found = [];
    problem = null;
    existing = null;
    linkTypes().then(
      (list) => {
        names = linkNames(list);
        chosenName =
          preset && names.some((entry) => nameKey(entry) === preset)
            ? preset
            : names[0]
              ? nameKey(names[0])
              : '';
      },
      () => {
        problem = m.linkTypesFailed();
      },
    );
  });

  // Searches as the person types; recent issues before.
  $effect(() => {
    if (!open || mode !== 'link') return;
    const text = query.trim();
    const controller = new AbortController();
    const timer = setTimeout(
      () => {
        loading = true;
        (text ? findIssues(text, controller.signal) : recentIssues(controller.signal)).then(
          (list) => {
            found = list;
            loading = false;
          },
          (error: unknown) => {
            if (controller.signal.aborted) return;
            problem = describeError(error).message;
            loading = false;
          },
        );
      },
      text ? 250 : 0,
    );
    return () => {
      clearTimeout(timer);
      controller.abort();
    };
  });

  function change(other: Issue, on: boolean): void {
    chosen = on ? [...chosen, other] : chosen.filter((picked) => picked.id !== other.id);
  }

  async function link(): Promise<void> {
    if (!name || chosen.length === 0) return;
    busy = true;
    problem = null;
    existing = null;
    try {
      const answer = await links.add(
        name,
        chosen.map((other) => other.id),
      );
      if (answer.added.length > 0) {
        toasts.show({ tone: 'success', message: m.issuesLinked({ count: answer.added.length }) });
      }
      if (answer.existing.length > 0) {
        existing = m.alreadyLinked({
          ids: answer.existing.map((id) => issueRow(id)?.readable_id ?? String(id)).join(', '),
        });
        announce(existing);
        chosen = [];
      } else {
        open = false;
      }
    } catch (error) {
      problem = refusalText(error);
      announce(problem, 'assertive');
    } finally {
      busy = false;
    }
  }

  async function create(): Promise<void> {
    if (!name || typed === '') return;
    busy = true;
    problem = null;
    try {
      const created = await createLinked(issue.project.id, typed, issue, name, links);
      toasts.show({
        tone: 'success',
        message: m.createdAndLinked({ id: created.readable_id ?? String(created.id) }),
      });
      open = false;
    } catch (error) {
      problem = refusalText(error);
      announce(problem, 'assertive');
    } finally {
      busy = false;
    }
  }

  function fullForm(): void {
    if (!name || !onnewform) return;
    open = false;
    onnewform(newIssueLink(issue.id, name));
  }
</script>

{#snippet option(other: Issue)}<IssueRef issue={other} link={false} />{/snippet}
{#snippet action()}
  {#if typed !== '' && canCreate}
    <Button size="sm" variant="ghost" loading={busy} disabled={!name} onclick={create}
      >{m.createAndLink({ summary: typed })}</Button
    >
  {/if}
{/snippet}

<Dialog bind:open {title} size="md" dismissible={!busy}>
  <div class="body">
    <Select
      label={m.linkName()}
      options={names.map((entry) => ({ value: nameKey(entry), label: entry.name }))}
      bind:value={chosenName}
    />
    {#if mode === 'link'}
      <OptionList
        label={typed ? m.foundIssues() : m.recentIssues()}
        mode="multiple"
        {options}
        key={(other: Issue) => other.id}
        checked={(other: Issue) => chosen.some((picked) => picked.id === other.id)}
        onchange={change}
        bind:query
        searchLabel={m.findIssue()}
        {loading}
        {option}
        {action}
      />
    {/if}
    {#if existing}<Notice tone="info">{existing}</Notice>{/if}
    {#if problem}<Notice tone="danger">{problem}</Notice>{/if}
  </div>
  {#snippet footer()}
    {#if onnewform && canCreate}
      <span class="full">
        <Button variant={mode === 'new' ? 'primary' : 'ghost'} disabled={!name} onclick={fullForm}
          >{m.openFullForm()}</Button
        >
      </span>
    {/if}
    <Button disabled={busy} onclick={() => (open = false)}>{common.cancel()}</Button>
    {#if mode === 'link'}
      <Button
        variant="primary"
        loading={busy}
        disabled={!name || chosen.length === 0}
        onclick={link}>{m.linkChosen({ count: chosen.length })}</Button
      >
    {/if}
  {/snippet}
</Dialog>

<style>
  .body {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }

  .full {
    margin-inline-end: auto;
  }
</style>
