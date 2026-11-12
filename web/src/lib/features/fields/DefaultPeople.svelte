<script lang="ts">
  // Default people of a user field (FLD-055): chosen with the people search; the server refuses
  // anyone the field's set does not offer, and the form shows why.
  import UserMinus from '@lucide/svelte/icons/user-minus';
  import UserPlus from '@lucide/svelte/icons/user-plus';

  import type { AccountSummary } from '#lib/api/schema.gen.ts';
  import { PersonName, personName, type PersonLike } from '#lib/features/access/index.ts';
  import Button from '#lib/ui/Button.svelte';
  import IconButton from '#lib/ui/IconButton.svelte';

  import * as m from './messages.ts';
  import PeopleDialog from './PeopleDialog.svelte';

  type Person = PersonLike & { id: number };

  interface Props {
    multi: boolean;
    people?: Person[];
    disabled?: boolean;
    error?: string | null;
  }

  let { multi, people = $bindable([]), disabled = false, error = null }: Props = $props();

  const id = $props.id();
  let choosing = $state(false);

  function add(chosen: AccountSummary[]): void {
    if (!multi) {
      people = chosen.slice(0, 1);
      return;
    }
    const known = new Set(people.map((person) => person.id));
    people = [...people, ...chosen.filter((person) => !known.has(person.id))];
  }
</script>

<fieldset class="defaults" {disabled} aria-describedby="{id}-hint{error ? ` ${id}-error` : ''}">
  <legend>{multi ? m.defaultValues() : m.defaultValue()}</legend>
  <p id="{id}-hint" class="hint">{m.defaultPeopleHint()}</p>
  {#if people.length === 0}
    <p class="hint">{m.nobodyByDefault()}</p>
  {:else}
    <ul class="list">
      {#each people as person (person.id)}
        <li class="person">
          <span class="who"><PersonName {person} withUsername /></span>
          {#if !disabled}
            <IconButton
              icon={UserMinus}
              label={m.removeDefaultPerson({ name: personName(person) })}
              onclick={() => {
                people = people.filter((other) => other.id !== person.id);
              }}
            />
          {/if}
        </li>
      {/each}
    </ul>
  {/if}
  {#if error}<p id="{id}-error" class="error">{error}</p>{/if}
  {#if !disabled}
    <div>
      <Button
        icon={UserPlus}
        onclick={() => {
          choosing = true;
        }}
      >
        {multi ? m.choosePeople() : m.choosePerson()}
      </Button>
    </div>
  {/if}
</fieldset>

<PeopleDialog
  bind:open={choosing}
  title={multi ? m.choosePeople() : m.choosePerson()}
  label={multi ? m.defaultValues() : m.defaultValue()}
  mode={multi ? 'multiple' : 'single'}
  exclude={multi ? people.map((person) => person.id) : []}
  action={m.choose()}
  onconfirm={add}
/>

<style>
  .defaults {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    min-width: 0;
    margin: 0;
    padding: 0;
    border: none;
  }

  legend {
    margin-bottom: var(--space-1);
    padding: 0;
    font-weight: var(--weight-medium);
  }

  .hint {
    color: var(--color-text-muted);
    font-size: var(--text-sm);
  }

  .error {
    color: var(--color-danger);
    font-size: var(--text-sm);
  }

  .list {
    margin: 0;
    padding: 0;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    list-style: none;
  }

  .person {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    min-height: calc(var(--control-md) + var(--space-1));
    padding: var(--space-0_5) var(--space-1) var(--space-0_5) var(--space-3);
  }

  .person + .person {
    border-top: 1px solid var(--color-border);
  }

  .who {
    flex: 1;
    min-width: 0;
  }
</style>
