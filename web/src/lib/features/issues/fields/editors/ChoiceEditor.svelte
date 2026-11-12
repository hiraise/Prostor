<script lang="ts">
  // Editor of a field with options (FLD-007…FLD-014): the field's values, people or groups to
  // search and choose. One value: choosing saves and closes. Several: each toggle saves at once
  // and the list stays open. Held archived values are listed, marked and removable, never
  // offered (ISS-019); a field that cannot be empty has no empty choice (FLD-057). Values carry
  // their description (FLD-043) and versions their release (FLD-047). With the right to change
  // the set, a name that matches nothing can be added as a new value and chosen.
  import { api } from '#lib/api/client.ts';
  import type { GroupRef, IssueField, OptionValue, UserRef } from '#lib/api/schema.gen.ts';
  import { groupName, PersonName } from '#lib/features/access/index.ts';
  import { refusalText, ValueChip } from '#lib/features/fields/index.ts';
  import { currentUser } from '#lib/session/index.ts';
  import { parseIsoDate } from '#lib/time/calendar.ts';
  import Time from '#lib/time/Time.svelte';
  import { announce } from '#lib/ui/announce.svelte.ts';
  import Button from '#lib/ui/Button.svelte';
  import OptionList from '#lib/ui/OptionList.svelte';
  import { describeError } from '#lib/ui/toast.svelte.ts';

  import * as m from '../../messages.ts';
  import { choiceId, heldChoices, toInput, withChoices, type Choice } from '../field-values.ts';
  import type { FieldRequest } from '../save.ts';

  interface Props {
    issueId: number;
    entry: IssueField;
    onsave: (next: IssueField, request: FieldRequest) => void;
    close: () => void;
  }

  let { issueId, entry, onsave, close }: Props = $props();

  /** One line of the list: a choice, or the empty choice of a single-valued field. */
  interface Row {
    key: string;
    choice: Choice | null;
    /** The value as the picker offered it (release of versions). */
    offered?: OptionValue;
  }

  const multi = $derived(entry.field.multi);
  const held = $derived(heldChoices(entry));
  let query = $state('');
  let offered = $state<Choice[]>([]);
  let offeredValues = $state<OptionValue[]>([]);
  let canAdd = $state(false);
  let set = $state<number | null>(null);
  let loading = $state(true);
  let problem = $state<string | null>(null);
  let adding = $state(false);
  let controller: AbortController | null = null;

  async function load(text: string): Promise<void> {
    controller?.abort();
    const current = new AbortController();
    controller = current;
    loading = true;
    problem = null;
    try {
      const answer = await api.get('/issues/{id}/fields/{field_id}/options', {
        path: { id: issueId, field_id: entry.field.id },
        query: { q: text.trim() || undefined, limit: 20 },
        signal: current.signal,
      });
      if (current.signal.aborted) return;
      offeredValues = answer.values;
      offered = [
        ...answer.values.map((value): Choice => ({ kind: 'value', value })),
        ...answer.users.map((user): Choice => ({ kind: 'user', user })),
        ...answer.groups.map((group): Choice => ({ kind: 'group', group })),
      ];
      canAdd = answer.can_add_value;
      set = answer.set;
    } catch (error) {
      if (current.signal.aborted) return;
      problem = describeError(error).message;
    } finally {
      if (controller === current) loading = false;
    }
  }

  // Search a moment after typing stops; the first load runs at once.
  let first = true;
  $effect(() => {
    const typed = query;
    if (first) {
      first = false;
      void load(typed);
      return;
    }
    const timer = setTimeout(() => {
      void load(typed);
    }, 200);
    return () => {
      clearTimeout(timer);
    };
  });

  const rows = $derived.by((): Row[] => {
    const list: Row[] = [];
    if (!multi && !entry.required && query.trim() === '') list.push({ key: 'empty', choice: null });
    // Held archived values first: the server never offers them.
    for (const choice of held) {
      if (choice.kind === 'value' && choice.value.archived) {
        list.push({ key: `held-${String(choice.value.id)}`, choice });
      }
    }
    for (const choice of offered) {
      list.push({
        key: `${choice.kind}-${String(choiceId(choice))}`,
        choice,
        offered:
          choice.kind === 'value'
            ? offeredValues.find((value) => value.id === choice.value.id)
            : undefined,
      });
    }
    return list;
  });

  function isHeld(choice: Choice | null): boolean {
    if (choice === null) return held.length === 0;
    return held.some((other) => other.kind === choice.kind && choiceId(other) === choiceId(choice));
  }

  function choose(choice: Choice | null): void {
    if (isHeld(choice)) {
      close();
      return;
    }
    const choices = choice ? [choice] : [];
    onsave(withChoices(entry, choices), { kind: 'set', value: toInput(entry, choices) });
    close();
  }

  function toggle(choice: Choice, on: boolean): void {
    const id = choiceId(choice);
    const choices = on
      ? [...held, choice]
      : held.filter((other) => !(other.kind === choice.kind && choiceId(other) === id));
    onsave(withChoices(entry, choices), { kind: on ? 'add' : 'remove', ids: [id] });
  }

  // A held value of a field that cannot be empty stays while it is the only one.
  function locked(row: Row): boolean {
    return multi && entry.required && held.length === 1 && isHeld(row.choice);
  }

  const typedName = $derived(query.trim());
  const canOfferAdd = $derived(
    canAdd &&
      set !== null &&
      typedName !== '' &&
      !loading &&
      !offered.some(
        (choice) =>
          choice.kind === 'value' && choice.value.name.toLowerCase() === typedName.toLowerCase(),
      ),
  );

  async function addValue(): Promise<void> {
    if (set === null) return;
    adding = true;
    problem = null;
    try {
      const name = typedName;
      const answer = await api.post('/value-sets/{id}/values', {
        path: { id: set },
        body: { name },
      });
      const created = answer.values.find((value) => value.name === name);
      if (!created) return;
      announce(m.valueAdded({ name }));
      const choice: Choice = { kind: 'value', value: created };
      if (multi) {
        toggle(choice, true);
        query = '';
      } else {
        choose(choice);
      }
    } catch (error) {
      problem = refusalText(error);
    } finally {
      adding = false;
    }
  }

  const me = $derived(currentUser()?.id);
</script>

{#snippet person(user: UserRef)}
  <PersonName person={user} withUsername />
  {#if user.id === me}<span class="tag">{m.me()}</span>{/if}
{/snippet}

{#snippet group(found: GroupRef)}
  <span>{groupName(found)}</span>
{/snippet}

{#snippet option(row: Row)}
  {#if row.choice === null}
    <span class="empty">{entry.empty_label}</span>
  {:else if row.choice.kind === 'user'}
    {@render person(row.choice.user)}
  {:else if row.choice.kind === 'group'}
    {@render group(row.choice.group)}
  {:else}
    <ValueChip name={row.choice.value.name} color={row.choice.value.color} />
    {#if row.choice.value.archived}<span class="tag">{m.archived()}</span>{/if}
  {/if}
{/snippet}

{#snippet secondary(row: Row)}
  {#if row.choice?.kind === 'value'}
    {@const release = row.offered?.release_date ? parseIsoDate(row.offered.release_date) : null}
    {#if row.offered?.released}
      <span>{m.released()}</span>
    {:else if release}
      <span>{m.releaseOn()} <Time value={release} kind="date" /></span>
    {/if}
    {#if row.choice.value.description}<span>{row.choice.value.description}</span>{/if}
  {/if}
{/snippet}

{#snippet action()}
  {#if canOfferAdd}
    <Button size="sm" variant="ghost" loading={adding} onclick={addValue}
      >{m.addValue({ name: typedName })}</Button
    >
  {/if}
{/snippet}

<OptionList
  label={m.fieldOptions({ name: entry.field.name })}
  mode={multi ? 'multiple' : 'single'}
  options={rows}
  key={(row: Row) => row.key}
  checked={(row: Row) => isHeld(row.choice)}
  disabled={locked}
  onchange={(row: Row, on: boolean) => {
    if (!multi) choose(row.choice);
    else if (row.choice) toggle(row.choice, on);
  }}
  bind:query
  {loading}
  {problem}
  {option}
  {secondary}
  action={canAdd ? action : undefined}
/>

<style>
  .tag {
    color: var(--color-text-muted);
    font-size: var(--text-sm);
  }

  .empty {
    color: var(--color-text-muted);
  }
</style>
