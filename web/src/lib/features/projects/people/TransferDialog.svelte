<script lang="ts">
  // Giving the project a new owner (PRJ-053): pick one person, read what happens, confirm. The new
  // owner becomes a project administrator; the previous owner keeps their roles.
  import { untrack } from 'svelte';

  import type { AccountSummary, Project, UserRef } from '#lib/api/schema.gen.ts';
  import { PeopleSearch, personName } from '#lib/features/access/index.ts';
  import * as common from '#lib/i18n/messages/common.ts';
  import { announce } from '#lib/ui/announce.svelte.ts';
  import Button from '#lib/ui/Button.svelte';
  import Dialog from '#lib/ui/Dialog.svelte';
  import { toasts } from '#lib/ui/toast.svelte.ts';

  import { transferProject } from '../projects.ts';
  import * as m from './messages.ts';
  import { problemOf, type Problem } from './people.ts';
  import ProblemNotice from './ProblemNotice.svelte';

  interface Props {
    open?: boolean;
    project: Project;
    owner: UserRef | null;
    ondone: () => unknown;
  }

  let { open = $bindable(false), project, owner, ondone }: Props = $props();

  const formId = $props.id();
  let chosen = $state<AccountSummary[]>([]);
  let missing = $state(false);
  let busy = $state(false);
  let problem = $state<Problem | null>(null);

  const next = $derived(chosen[0] ?? null);
  const text = $derived.by(() => {
    if (!next) return m.transferHint();
    return owner
      ? m.transferText({ name: personName(next), owner: personName(owner) })
      : m.transferToText({ name: personName(next) });
  });

  $effect(() => {
    if (!open) return;
    untrack(() => {
      chosen = [];
      missing = false;
      problem = null;
    });
  });

  async function submit(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    problem = null;
    missing = !next;
    if (!next) {
      announce(m.chooseOwner(), 'assertive');
      return;
    }
    busy = true;
    try {
      await transferProject(project.id, next.id);
      open = false;
      toasts.show({ tone: 'success', message: m.ownershipTransferred({ name: personName(next) }) });
      await ondone();
    } catch (error) {
      problem = problemOf(error);
    } finally {
      busy = false;
    }
  }
</script>

<Dialog bind:open title={m.transferTitle({ project: project.name })} dismissible={!busy}>
  <form id={formId} class="form" novalidate onsubmit={submit}>
    {#if problem}<ProblemNotice {problem} />{/if}
    <PeopleSearch
      label={m.newOwner()}
      mode="single"
      bind:chosen
      exclude={owner ? [owner.id] : []}
    />
    {#if missing && !next}<p class="error">{m.chooseOwner()}</p>{/if}
    <p class="what" aria-live="polite">{text}</p>
  </form>
  {#snippet footer()}
    <Button
      disabled={busy}
      onclick={() => {
        open = false;
      }}
    >
      {common.cancel()}
    </Button>
    <Button type="submit" form={formId} variant="primary" loading={busy}>
      {m.transferOwnership()}
    </Button>
  {/snippet}
</Dialog>

<style>
  .form {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }

  .what {
    color: var(--color-text-muted);
  }

  .error {
    color: var(--color-danger);
    font-size: var(--text-sm);
  }
</style>
