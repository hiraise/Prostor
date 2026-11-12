<script lang="ts">
  // One audience of a shared tag or search (QRY-065, ISS-037): «Только я», or chosen people and
  // groups found by name (D-038: accounts and groups, project teams among them). A search that
  // the viewer may not run for groups leaves only people.
  import {
    type PersonLike,
    findGroups,
    findPeople,
    groupName,
    personName,
    personUsername,
  } from '#lib/features/access/index.ts';
  import OptionList from '#lib/ui/OptionList.svelte';
  import RadioGroup from '#lib/ui/RadioGroup.svelte';
  import { describeError } from '#lib/ui/toast.svelte.ts';

  import * as m from './messages.ts';
  import type { Audience } from './sidebar.ts';

  interface Props {
    legend: string;
    value?: Audience;
    /** «Выбранные люди и группы» is on. */
    restricted?: boolean;
    error?: string | null;
  }

  let {
    legend,
    value = $bindable({ users: [], groups: [] }),
    restricted = $bindable(false),
    error = null,
  }: Props = $props();

  type Choice =
    { kind: 'group'; group: Audience['groups'][number] } | { kind: 'user'; user: PersonLike };

  const id = $props.id();
  let text = $state('');
  let found = $state<Choice[]>([]);
  let loading = $state(false);
  let problem = $state<string | null>(null);

  function key(choice: Choice): string {
    return choice.kind === 'group' ? `g${String(choice.group.id)}` : `u${String(choice.user.id)}`;
  }

  /** The chosen first, then what the search found. */
  const options = $derived.by(() => {
    const all: Choice[] = [
      ...value.groups.map((group): Choice => ({ kind: 'group', group })),
      ...value.users.map((user): Choice => ({ kind: 'user', user })),
      ...found,
    ];
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
        Promise.all([
          findPeople(typed, { signal: controller.signal, limit: 20 }),
          // Groups need more rights than people; without them only people are offered.
          findGroups(typed, null, 'all').catch(() => null),
        ]).then(
          ([people, groups]) => {
            if (controller.signal.aborted) return;
            found = [
              ...(groups?.items ?? []).map((group): Choice => ({ kind: 'group', group })),
              ...people.items.map((user): Choice => ({ kind: 'user', user })),
            ];
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

  function checked(choice: Choice): boolean {
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

<div class="audience" aria-describedby={error ? `${id}-error` : undefined}>
  <RadioGroup
    {legend}
    options={[
      { value: 'me', label: m.onlyMe() },
      { value: 'chosen', label: m.chosen() },
    ]}
    bind:value={
      () => (restricted ? 'chosen' : 'me'),
      (next: string) => {
        restricted = next === 'chosen';
        if (!restricted) value = { users: [], groups: [] };
      }
    }
  />
  {#if restricted}
    <OptionList
      label={m.sharedOptions({ audience: legend })}
      mode="multiple"
      {options}
      {key}
      {checked}
      onchange={change}
      bind:query={text}
      searchLabel={m.findShared()}
      {loading}
      {problem}
    >
      {#snippet option(choice: Choice)}
        {choice.kind === 'group' ? groupName(choice.group) : personName(choice.user)}
      {/snippet}
      {#snippet secondary(choice: Choice)}
        {choice.kind === 'group' ? m.group() : (personUsername(choice.user) ?? '')}
      {/snippet}
    </OptionList>
  {/if}
  {#if error}<p class="error" id="{id}-error">{error}</p>{/if}
</div>

<style>
  .audience {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }

  .error {
    color: var(--color-danger);
    font-size: var(--text-sm);
  }
</style>
