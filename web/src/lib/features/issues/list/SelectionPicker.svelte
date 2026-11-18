<script lang="ts">
  // The toolbar's pickers (step 2.2 §8.3, QRY-084): «Тег» searches the tags the person may use
  // and offers to create a missing one (ISS-034), «Исполнитель» searches the directory, and
  // «Добавить связь» chooses a link name, then one or more target issues — every chosen issue
  // gets a link to every target, one link command per pair, its rights, direction and duplicates
  // checked by the engine as in 2.1. Each choice becomes a command line the dialog previews
  // before it applies (D-064); an issue the line fails on lands in the dialog's per-issue rows.
  import type { AccountSummary, Issue, TagRef } from '#lib/api/schema.gen.ts';
  import { findPeople, personName, personUsername } from '#lib/features/access/index.ts';
  import * as common from '#lib/i18n/messages/common.ts';
  import Button from '#lib/ui/Button.svelte';
  import Dialog from '#lib/ui/Dialog.svelte';
  import OptionList from '#lib/ui/OptionList.svelte';
  import Select from '#lib/ui/Select.svelte';
  import { describeError } from '#lib/ui/toast.svelte.ts';

  import { createTag, findTags } from '../actions.ts';
  import IssueRef from '../IssueRef.svelte';
  import {
    findIssues,
    linkNames,
    linkTypes,
    nameKey,
    recentIssues,
    type LinkName,
  } from '../links/links.svelte.ts';
  import * as issueMessages from '../messages.ts';
  import * as m from './messages.ts';

  interface Props {
    open?: boolean;
    mode: 'tag' | 'assignee' | 'link';
    /** The assignee field's name as the person sees it; the line names the field, as the person
     * would type it. `null` until the fields load — then the username alone assigns (§4.2). */
    assigneeName: string | null;
    /** The composed line, for the command dialog's preview; closes the picker. */
    onchosen: (line: string) => void;
  }

  let { open = $bindable(false), mode, assigneeName, onchosen }: Props = $props();

  const title = $derived(
    mode === 'tag'
      ? m.tagPickerTitle()
      : mode === 'assignee'
        ? m.assigneePickerTitle()
        : m.linkPickerTitle(),
  );

  let query = $state('');
  let loading = $state(false);
  let problem = $state<string | null>(null);

  // «Тег» (§8.3): the tags found, and the creation of a name nobody has.
  let foundTags = $state<TagRef[]>([]);
  let creating = $state(false);

  // «Исполнитель» (§8.3): the accounts the directory right lets the person name (QRY-057).
  let foundPeople = $state<AccountSummary[]>([]);

  // «Добавить связь» (§8.3): the person's link names, then the issues found.
  let names = $state<LinkName[]>([]);
  let chosenName = $state('');
  let issues = $state<Issue[]>([]);
  let chosen = $state<Issue[]>([]);
  const name = $derived(names.find((entry) => nameKey(entry) === chosenName));
  /** The issues offered: the chosen first, then what the search found. */
  const options = $derived([
    ...chosen,
    ...issues.filter((i) => !chosen.some((c) => c.id === i.id)),
  ]);

  const typed = $derived(query.trim());
  const canCreate = $derived(
    mode === 'tag' &&
      typed !== '' &&
      !loading &&
      !foundTags.some((tag) => tag.name.toLocaleLowerCase() === typed.toLocaleLowerCase()),
  );

  // Every opening starts clean; the link names load once per opening.
  $effect(() => {
    if (!open) return;
    query = '';
    loading = false;
    problem = null;
    foundTags = [];
    creating = false;
    foundPeople = [];
    names = [];
    chosenName = '';
    issues = [];
    chosen = [];
    if (mode === 'link') {
      linkTypes().then(
        (list) => {
          names = linkNames(list);
          chosenName = names[0] ? nameKey(names[0]) : '';
        },
        () => {
          problem = issueMessages.linkTypesFailed();
        },
      );
    }
  });

  // The search of the tag picker: the tags the person may put on issues (ISS-035), debounced.
  $effect(() => {
    if (!open || mode !== 'tag') return;
    const text = query;
    const controller = new AbortController();
    const timer = setTimeout(
      () => {
        loading = true;
        problem = null;
        findTags(text, controller.signal).then(
          (tags) => {
            foundTags = tags.map(({ id, name: tagName, color }) => ({ id, name: tagName, color }));
            loading = false;
          },
          (failure: unknown) => {
            if (controller.signal.aborted) return;
            problem = describeError(failure).message;
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

  // The search of the assignee picker, over the same directory every people search uses.
  $effect(() => {
    if (!open || mode !== 'assignee') return;
    const text = query;
    const controller = new AbortController();
    const timer = setTimeout(
      () => {
        loading = true;
        problem = null;
        findPeople(text, { signal: controller.signal, limit: 20 }).then(
          (page) => {
            foundPeople = page.items;
            loading = false;
          },
          (failure: unknown) => {
            if (controller.signal.aborted) return;
            problem = describeError(failure).message;
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

  // The issue search of the link picker: the person's recent issues before typing, as the link
  // dialog offers them (LNK-005).
  $effect(() => {
    if (!open || mode !== 'link') return;
    const text = query.trim();
    const controller = new AbortController();
    const timer = setTimeout(
      () => {
        loading = true;
        (text ? findIssues(text, controller.signal) : recentIssues(controller.signal)).then(
          (list) => {
            issues = list;
            loading = false;
          },
          (failure: unknown) => {
            if (controller.signal.aborted) return;
            problem = describeError(failure).message;
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

  /** A value of the line: bare when it is one word the lexer reads back, quoted when not. */
  function atom(text: string): string {
    if (/^[\p{L}\p{N}][\p{L}\p{N}_.-]*$/u.test(text)) return text;
    return `"${text.replaceAll('\\', '\\\\').replaceAll('"', '\\"')}"`;
  }

  /** The tag line (§8.3), with the word the person would type. */
  function tagLine(tagName: string): string {
    return `${m.tagVerb()} ${atom(tagName)}`;
  }

  /** The assignee line: the field's name and the username, or the username alone (§4.2). */
  function assigneeLine(user: AccountSummary): string {
    const username = personUsername(user);
    if (username === null) return '';
    return assigneeName === null ? atom(username) : `${assigneeName} ${atom(username)}`;
  }

  /** The link line: one `name target` command per chosen target (§8.3). Link names and issue
   * IDs are dictionary entries the parser reads as runs of words — never quoted. */
  function linkLine(): string {
    if (!name) return '';
    return chosen
      .map((target) => `${name.name} ${target.readable_id ?? String(target.id)}`)
      .join(' ');
  }

  function chooseTag(tag: TagRef): void {
    open = false;
    onchosen(tagLine(tag.name));
  }

  /** A name nobody has: create the tag, then the line names it (§8.3, ISS-034). */
  async function create(): Promise<void> {
    if (!canCreate) return;
    creating = true;
    problem = null;
    try {
      const tag = await createTag(typed);
      open = false;
      onchosen(tagLine(tag.name));
    } catch (failure) {
      problem = describeError(failure).message;
    } finally {
      creating = false;
    }
  }

  function chooseAssignee(user: AccountSummary): void {
    const line = assigneeLine(user);
    if (line === '') return;
    open = false;
    onchosen(line);
  }

  function change(target: Issue, on: boolean): void {
    chosen = on ? [...chosen, target] : chosen.filter((picked) => picked.id !== target.id);
  }

  function link(): void {
    const line = linkLine();
    if (line === '') return;
    open = false;
    onchosen(line);
  }
</script>

{#snippet tagOption(tag: TagRef)}{tag.name}{/snippet}
{#snippet createTagAction()}
  {#if canCreate}
    <Button size="sm" variant="ghost" loading={creating} onclick={create}>
      {issueMessages.createTag({ name: typed })}
    </Button>
  {/if}
{/snippet}
{#snippet personOption(user: AccountSummary)}{personName(user)}{/snippet}
{#snippet personUsernameLine(user: AccountSummary)}{personUsername(user) ?? ''}{/snippet}
{#snippet issueOption(other: Issue)}<IssueRef issue={other} link={false} />{/snippet}

<Dialog bind:open {title} size="md">
  <div class="body">
    {#if mode === 'tag'}
      <OptionList
        label={issueMessages.tags()}
        mode="single"
        options={foundTags}
        key={(tag: TagRef) => tag.id}
        checked={() => false}
        onchange={(tag: TagRef) => chooseTag(tag)}
        bind:query
        searchLabel={issueMessages.findTag()}
        {loading}
        {problem}
        option={tagOption}
        action={createTagAction}
      />
    {:else if mode === 'assignee'}
      <OptionList
        label={m.people()}
        mode="single"
        options={foundPeople}
        key={(user: AccountSummary) => user.id}
        checked={() => false}
        onchange={(user: AccountSummary) => chooseAssignee(user)}
        bind:query
        searchLabel={m.findPerson()}
        {loading}
        {problem}
        option={personOption}
        secondary={personUsernameLine}
      />
    {:else}
      <Select
        label={issueMessages.linkName()}
        options={names.map((entry) => ({ value: nameKey(entry), label: entry.name }))}
        bind:value={chosenName}
      />
      <OptionList
        label={typed ? issueMessages.foundIssues() : issueMessages.recentIssues()}
        mode="multiple"
        {options}
        key={(other: Issue) => other.id}
        checked={(other: Issue) => chosen.some((picked) => picked.id === other.id)}
        onchange={change}
        bind:query
        searchLabel={issueMessages.findIssue()}
        {loading}
        {problem}
        option={issueOption}
      />
    {/if}
  </div>
  {#snippet footer()}
    <Button onclick={() => (open = false)}>{common.cancel()}</Button>
    {#if mode === 'link'}
      <Button variant="primary" disabled={!name || chosen.length === 0} onclick={link}>
        {issueMessages.linkChosen({ count: chosen.length })}
      </Button>
    {/if}
  {/snippet}
</Dialog>

<style>
  .body {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }
</style>
