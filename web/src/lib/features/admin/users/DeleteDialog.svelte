<script lang="ts">
  // Deleting one or many accounts for good (ACC-006): first what goes with them (architecture.md
  // §9.11), then the person who takes their place, then a destructive confirmation.
  import { untrack } from 'svelte';

  import type { AccountSummary, UserDeletionImpact } from '#lib/api/schema.gen.ts';
  import { PeopleSearch, personName } from '#lib/features/access/index.ts';
  import * as common from '#lib/i18n/messages/common.ts';
  import Button from '#lib/ui/Button.svelte';
  import { announce } from '#lib/ui/announce.svelte.ts';
  import Dialog from '#lib/ui/Dialog.svelte';
  import Notice from '#lib/ui/Notice.svelte';
  import Spinner from '#lib/ui/Spinner.svelte';
  import { toasts } from '#lib/ui/toast.svelte.ts';

  import * as m from './messages.ts';
  import { refusalText } from './problems.ts';
  import { deleteAccounts, deletionImpact } from './users.ts';

  interface Props {
    open?: boolean;
    accounts: readonly AccountSummary[];
    ondone: (ids: number[], replacement: AccountSummary) => unknown;
  }

  let { open = $bindable(false), accounts, ondone }: Props = $props();

  let impact = $state<UserDeletionImpact | null>(null);
  let impactProblem = $state<string | null>(null);
  let chosen = $state<AccountSummary[]>([]);
  let missing = $state(false);
  let busy = $state(false);
  let problem = $state<string | null>(null);
  let opened = 0;

  const ids = $derived(accounts.map((account) => account.id));
  const title = $derived.by(() => {
    const [first] = accounts;
    return accounts.length === 1 && first
      ? m.deleteOneTitle({ name: personName(first) })
      : m.deleteManyTitle({ count: accounts.length });
  });
  const counts = $derived(
    impact
      ? [
          impact.sessions > 0 ? m.impactSessions({ count: impact.sessions }) : null,
          impact.tokens > 0 ? m.impactTokens({ count: impact.tokens }) : null,
          impact.memberships > 0 ? m.impactMemberships({ count: impact.memberships }) : null,
          impact.assignments > 0 ? m.impactAssignments({ count: impact.assignments }) : null,
        ].filter((text) => text !== null)
      : [],
  );

  // Each opening counts the impact for the accounts chosen at that moment.
  $effect(() => {
    if (!open) return;
    const mine = ++opened;
    const users = untrack(() => [...ids]);
    impact = null;
    impactProblem = null;
    chosen = [];
    missing = false;
    problem = null;
    deletionImpact(users).then(
      (answer) => {
        if (mine === opened) impact = answer;
      },
      (error: unknown) => {
        if (mine === opened) impactProblem = refusalText(error);
      },
    );
  });

  async function confirm(): Promise<void> {
    problem = null;
    const [replacement] = chosen;
    missing = !replacement;
    if (!replacement) {
      announce(m.chooseReplacement(), 'assertive');
      return;
    }
    const users = [...ids];
    busy = true;
    try {
      await deleteAccounts(users, replacement.id);
      open = false;
      toasts.show({ tone: 'success', message: m.deletedDone({ count: users.length }) });
      await ondone(users, replacement);
    } catch (error) {
      problem = refusalText(error, m.deleteConflict);
    } finally {
      busy = false;
    }
  }
</script>

<Dialog bind:open {title} description={m.cannotUndo()} dismissible={!busy}>
  {#if open}
    <div class="body">
      <section class="impact" aria-busy={impact === null && impactProblem === null}>
        {#if impactProblem}
          <Notice tone="danger">{impactProblem}</Notice>
        {:else if !impact}
          <p class="muted"><Spinner /> {m.countingImpact()}</p>
        {:else if counts.length === 0}
          <p>{m.impactNothing()}</p>
        {:else}
          <p>{m.impactIntro()}</p>
          <ul>
            {#each counts as text (text)}<li>{text}</li>{/each}
          </ul>
        {/if}
      </section>
      <p>{m.replacementIntro()}</p>
      <PeopleSearch label={m.replacement()} mode="single" bind:chosen exclude={ids} />
      {#if missing && chosen.length === 0}
        <Notice tone="danger">{m.chooseReplacement()}</Notice>
      {/if}
      {#if problem}<Notice tone="danger">{problem}</Notice>{/if}
    </div>
  {/if}

  {#snippet footer()}
    <Button
      disabled={busy}
      onclick={() => {
        open = false;
      }}
    >
      {common.cancel()}
    </Button>
    <Button variant="danger" loading={busy} disabled={impact === null} onclick={confirm}>
      {m.deleteConfirm({ count: accounts.length })}
    </Button>
  {/snippet}
</Dialog>

<style>
  .body {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }

  .impact {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
  }

  .impact ul {
    margin: 0;
    padding-inline-start: var(--space-5);
  }

  .muted {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    color: var(--color-text-muted);
  }
</style>
