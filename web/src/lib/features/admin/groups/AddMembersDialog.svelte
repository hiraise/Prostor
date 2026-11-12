<script lang="ts">
  // Adding people to a group (ACC-026). Unless they may change system settings, the person adding
  // must hold every permission the group gives; a refusal names what is missing (ACC-050).
  import type { AccountSummary, Group } from '#lib/api/schema.gen.ts';
  import { PeopleSearch, groupName } from '#lib/features/access/index.ts';
  import * as common from '#lib/i18n/messages/common.ts';
  import Button from '#lib/ui/Button.svelte';
  import Dialog from '#lib/ui/Dialog.svelte';
  import { toasts } from '#lib/ui/toast.svelte.ts';

  import { addMembers, problemOf, type Problem } from './groups.ts';
  import * as m from './messages.ts';
  import ProblemNotice from './ProblemNotice.svelte';

  interface Props {
    open?: boolean;
    group: Group;
    /** Members already shown, left out of the choices. */
    members?: readonly number[];
    onadded: (users: number[]) => unknown;
  }

  let { open = $bindable(false), group, members = [], onadded }: Props = $props();

  let chosen = $state<AccountSummary[]>([]);
  let busy = $state(false);
  let problem = $state<Problem | null>(null);

  // Every opening starts with nobody chosen.
  $effect(() => {
    if (!open) return;
    chosen = [];
    problem = null;
  });

  async function submit(): Promise<void> {
    if (chosen.length === 0) {
      problem = { message: m.choosePeople(), missing: [] };
      return;
    }
    busy = true;
    problem = null;
    try {
      const added = await addMembers(
        group.id,
        chosen.map((person) => person.id),
      );
      open = false;
      toasts.show({
        tone: 'success',
        message: added.length > 0 ? m.membersAdded({ count: added.length }) : m.alreadyMembers(),
      });
      await onadded(added);
    } catch (error) {
      problem = problemOf(error, { lacking: m.lackingToAdd });
    } finally {
      busy = false;
    }
  }
</script>

<Dialog bind:open title={m.addMembersTitle({ group: groupName(group) })} dismissible={!busy}>
  <div class="content">
    {#if problem}<ProblemNotice {problem} />{/if}
    <!-- Searches only while open. -->
    {#if open}<PeopleSearch label={m.peopleToAdd()} bind:chosen exclude={members} />{/if}
  </div>

  {#snippet footer()}
    <Button
      disabled={busy}
      onclick={() => {
        open = false;
      }}
    >
      {common.cancel()}
    </Button>
    <Button variant="primary" loading={busy} onclick={submit}>{m.addMembers()}</Button>
  {/snippet}
</Dialog>

<style>
  .content {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }
</style>
