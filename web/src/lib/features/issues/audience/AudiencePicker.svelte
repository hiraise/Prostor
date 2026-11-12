<script lang="ts">
  // Whom an issue is visible to (ISS-041…ISS-043): everyone who sees the project, or only the
  // chosen groups and people. The reporter always sees it, so it is shown as included and cannot
  // be removed. `offer` narrows the candidates (1.7b: a comment of a restricted issue).
  import type { Audience, GroupRef, UserRef } from '#lib/api/schema.gen.ts';
  import { groupName, personName, personUsername } from '#lib/features/access/index.ts';
  import OptionList from '#lib/ui/OptionList.svelte';
  import RadioGroup from '#lib/ui/RadioGroup.svelte';
  import { describeError } from '#lib/ui/toast.svelte.ts';

  import { audienceCandidates } from '../actions.ts';
  import * as m from '../messages.ts';

  interface Props {
    project: number;
    reporter: UserRef;
    value?: Audience;
    /** «Только выбранные» is on; with nothing chosen the form must say so. */
    restricted?: boolean;
    error?: string | null;
    /** Only these may be offered; all the project's candidates when absent. */
    offer?: Audience;
    /** The first choice; «Все, кто видит проект» by default. */
    allLabel?: string;
  }

  let {
    project,
    reporter,
    value = $bindable({ users: [], groups: [] }),
    restricted = $bindable(false),
    error = null,
    offer,
    allLabel = m.audienceAll(),
  }: Props = $props();

  type Choice = { kind: 'group'; group: GroupRef } | { kind: 'user'; user: UserRef };

  const id = $props.id();
  let text = $state('');
  let found = $state<Choice[]>([]);
  let loading = $state(false);
  let problem = $state<string | null>(null);

  const mode = $derived(restricted ? 'chosen' : 'all');

  function key(choice: Choice): string {
    return choice.kind === 'group' ? `g${String(choice.group.id)}` : `u${String(choice.user.id)}`;
  }

  /** The chosen first, then the reporter, then what the search found. */
  const options = $derived.by(() => {
    const chosen: Choice[] = [
      ...value.groups.map((group): Choice => ({ kind: 'group', group })),
      ...value.users.map((user): Choice => ({ kind: 'user', user })),
    ];
    const all = [...chosen, { kind: 'user', user: reporter } as Choice, ...found];
    const keys = all.map(key);
    return all.filter((choice, index) => keys.indexOf(key(choice)) === index);
  });

  $effect(() => {
    if (!restricted) return;
    const typed = text;
    const controller = new AbortController();
    const timer = setTimeout(
      () => {
        loading = true;
        problem = null;
        audienceCandidates(project, typed, controller.signal).then(
          (answer) => {
            const offered = (choice: Choice): boolean =>
              !offer ||
              (choice.kind === 'group'
                ? offer.groups.some((group) => group.id === choice.group.id)
                : offer.users.some((user) => user.id === choice.user.id));
            found = [
              ...answer.groups.map((group): Choice => ({ kind: 'group', group })),
              ...answer.users.map((user): Choice => ({ kind: 'user', user })),
            ].filter(offered);
            loading = false;
          },
          (failure: unknown) => {
            if (controller.signal.aborted) return;
            problem = describeError(failure).message;
            loading = false;
          },
        );
      },
      typed ? 250 : 0,
    );
    return () => {
      clearTimeout(timer);
      controller.abort();
    };
  });

  function isReporter(choice: Choice): boolean {
    return choice.kind === 'user' && choice.user.id === reporter.id;
  }

  function checked(choice: Choice): boolean {
    if (isReporter(choice)) return true;
    return choice.kind === 'group'
      ? value.groups.some((group) => group.id === choice.group.id)
      : value.users.some((user) => user.id === choice.user.id);
  }

  function change(choice: Choice, on: boolean): void {
    if (choice.kind === 'group') {
      const others = value.groups.filter((group) => group.id !== choice.group.id);
      value = { ...value, groups: on ? [...others, choice.group] : others };
    } else {
      const others = value.users.filter((user) => user.id !== choice.user.id);
      value = { ...value, users: on ? [...others, choice.user] : others };
    }
  }
</script>

<div class="picker" aria-describedby={error ? `${id}-error` : undefined}>
  <RadioGroup
    legend={m.visibility()}
    options={[
      { value: 'all', label: allLabel },
      { value: 'chosen', label: m.audienceChosen() },
    ]}
    bind:value={
      () => mode,
      (next: string) => {
        restricted = next === 'chosen';
        if (!restricted) value = { users: [], groups: [] };
      }
    }
  />
  {#if restricted}
    <OptionList
      label={m.audienceOptions()}
      mode="multiple"
      {options}
      {key}
      {checked}
      disabled={isReporter}
      onchange={change}
      bind:query={text}
      searchLabel={m.findAudience()}
      {loading}
      {problem}
    >
      {#snippet option(choice: Choice)}
        {choice.kind === 'group' ? groupName(choice.group) : personName(choice.user)}
      {/snippet}
      {#snippet secondary(choice: Choice)}
        {#if isReporter(choice)}{m.reporterIncluded()}{:else if choice.kind === 'user'}{personUsername(
            choice.user,
          )}{/if}
      {/snippet}
    </OptionList>
  {/if}
  {#if error}<p class="error" id="{id}-error">{error}</p>{/if}
</div>

<style>
  .picker {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }

  .error {
    color: var(--color-danger);
    font-size: var(--text-sm);
  }
</style>
