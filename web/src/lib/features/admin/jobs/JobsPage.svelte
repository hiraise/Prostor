<script lang="ts">
  // The «Задания» screen of the administration area (ADM-031, ADM-032; plan step 1.11 §6.2): the
  // background jobs of the instance, newest first — the kind in our words with its target, the
  // start time, the initiator, a state badge and the progress line; a failed row says why. State
  // chips and a free-text filter (a leading `-` excludes) narrow it, «Показать ещё» turns the
  // page. No live topic: the list refreshes after an action, on the Refresh button and on revisit.
  import type { AdminJob, AdminJobState } from '#lib/api/schema.gen.ts';
  import { PersonName } from '#lib/features/access/index.ts';
  import * as common from '#lib/i18n/messages/common.ts';
  import { can, canSeeJobs } from '#lib/session/index.ts';
  import Button from '#lib/ui/Button.svelte';
  import Dialog from '#lib/ui/Dialog.svelte';
  import ErrorState from '#lib/ui/ErrorState.svelte';
  import ForbiddenState from '#lib/ui/ForbiddenState.svelte';
  import Notice from '#lib/ui/Notice.svelte';
  import PageHeader from '#lib/ui/PageHeader.svelte';
  import Spinner from '#lib/ui/Spinner.svelte';
  import TextField from '#lib/ui/TextField.svelte';
  import { showError, toasts } from '#lib/ui/toast.svelte.ts';
  import { getTimeContext } from '#lib/time/context.svelte.ts';
  import { formatInstant } from '#lib/time/format.ts';

  import * as m from './messages.ts';
  import { cancelJob, kindName, listJobs, mayCancel, mayRetry, retryJob } from './jobs.ts';

  const STATES: AdminJobState[] = ['queued', 'running', 'finished', 'failed', 'cancelled'];
  /** Typing pauses this long before the filter is searched (the fields list does the same). */
  const DEBOUNCE_MS = 250;

  /** Where “forbidden” offers to go. */
  let { home }: { home?: string } = $props();

  let jobs = $state<AdminJob[] | null>(null);
  let next = $state<number | null>(null);
  let failure = $state<unknown>(null);
  let loading = $state(false);
  let moreBusy = $state(false);
  let request = 0;

  let stateFilter = $state<AdminJobState | null>(null);
  let text = $state('');
  /** The words the list is filtered by now (typed text settles into it). */
  let settled = $state('');

  /** The job waiting for the «Отменить?» confirmation. */
  let cancelling = $state<AdminJob | null>(null);
  let acting = $state<number | null>(null);

  const mayWrite = $derived(can('system.write'));
  const context = $derived(getTimeContext());

  async function load(more: boolean): Promise<void> {
    const mine = ++request;
    if (!more) loading = true;
    else moreBusy = true;
    failure = null;
    // Read before the first await, so the effect below tracks the two filters (the page rides along).
    const filter = stateFilter;
    const query = settled;
    const before = more ? next : null;
    try {
      const page = await listJobs({
        state: filter ?? undefined,
        q: query || undefined,
        before: before ?? undefined,
      });
      if (mine !== request) return;
      jobs = more ? [...(jobs ?? []), ...page.items] : page.items;
      next = page.next;
    } catch (error) {
      if (mine === request) failure = error;
    } finally {
      if (mine === request) {
        loading = false;
        moreBusy = false;
      }
    }
  }

  // The text filter settles a moment after typing stops; a chip or a settled word reloads.
  $effect(() => {
    const typed = text;
    const timerNow = setTimeout(
      () => {
        settled = typed;
      },
      typed ? DEBOUNCE_MS : 0,
    );
    return () => {
      clearTimeout(timerNow);
    };
  });

  $effect(() => {
    void load(false);
  });

  function shown(job: AdminJob): string {
    return formatInstant(job.started_at ?? job.created_at, context, { year: 'always' });
  }

  async function confirmCancel(): Promise<void> {
    const job = cancelling;
    if (job === null || acting !== null) return;
    acting = job.id;
    try {
      await cancelJob(job.id);
      toasts.show({ tone: 'success', message: m.cancelledToast() });
      cancelling = null;
      // The job's state changed: the row as it stands is stale, so the list comes again.
      await load(false);
    } catch (error) {
      showError(error);
    } finally {
      acting = null;
    }
  }

  async function retry(id: number): Promise<void> {
    if (acting !== null) return;
    acting = id;
    try {
      await retryJob(id);
      toasts.show({ tone: 'success', message: m.retryQueued() });
      await load(false);
    } catch (error) {
      showError(error);
    } finally {
      acting = null;
    }
  }
</script>

<svelte:head><title>{m.jobsTitle()} · trackflow</title></svelte:head>

<div class="jobs-page">
  {#if !canSeeJobs()}
    <ForbiddenState {home} />
  {:else}
    <PageHeader title={m.jobsTitle()}>
      {#snippet actions()}
        <Button onclick={() => void load(false)} {loading}>{m.refresh()}</Button>
      {/snippet}
    </PageHeader>
    <p class="intro">{m.jobsIntro()}</p>

    {#if !mayWrite}<Notice tone="info">{m.readOnlyNote()}</Notice>{/if}

    <div class="filters">
      <div class="states" role="group" aria-label={m.filterLabel()}>
        <Button
          variant={stateFilter === null ? 'secondary' : 'ghost'}
          aria-pressed={stateFilter === null}
          onclick={() => {
            stateFilter = null;
          }}
        >
          {m.allStates()}
        </Button>
        {#each STATES as one (one)}
          <Button
            variant={stateFilter === one ? 'secondary' : 'ghost'}
            aria-pressed={stateFilter === one}
            onclick={() => {
              stateFilter = one;
            }}
          >
            {m.stateNames[one]()}
          </Button>
        {/each}
      </div>
      <TextField
        label={m.filterLabel()}
        hint={m.filterHint()}
        type="search"
        autocomplete="off"
        spellcheck="false"
        bind:value={text}
      />
    </div>

    {#if failure !== null}
      <ErrorState error={failure} level={3} onretry={() => void load(false)} />
    {:else if jobs === null}
      <p class="loading"><Spinner label={common.loading()} /></p>
    {:else if jobs.length === 0}
      <p class="empty">
        {settled.trim() !== '' || stateFilter !== null ? m.nothingFound() : m.noJobs()}
      </p>
    {:else}
      <ul class="jobs">
        {#each jobs as job (job.id)}
          <li class="job">
            <div class="what">
              <span class="kind">{kindName(job.kind)}</span>
              {#if job.status_text}<span class="target">{job.status_text}</span>{/if}
            </div>
            <div class="who">
              <span class="cell">
                <span class="label">{m.startedColumn()}</span>
                {shown(job)}
              </span>
              <span class="cell">
                <span class="label">{m.initiatorColumn()}</span>
                {#if job.initiator}
                  <PersonName person={job.initiator} />
                {:else}
                  <span>{m.byServer()}</span>
                {/if}
              </span>
            </div>
            <div class="meta">
              <span class="state" data-state={job.state} aria-label={m.stateNames[job.state]()}
                >{m.stateNames[job.state]()}</span
              >
              {#if job.progress.done > 0 || job.progress.total !== null}
                <span class="progress"
                  >{m.progressOf({ done: job.progress.done, total: job.progress.total })}</span
                >
              {/if}
              {#if (job.state === 'running' || job.state === 'queued') && job.max_attempts > 1}
                <span class="progress"
                  >{m.attemptsOf({ attempts: job.attempts, max: job.max_attempts })}</span
                >
              {/if}
              {#if job.cancel_requested && job.state === 'running'}
                <span class="progress">{m.stoppedAsk()}</span>
              {/if}
            </div>
            {#if job.state === 'failed' && job.last_error}
              <p class="error">{job.last_error}</p>
            {/if}
            {#if mayWrite}
              <div class="actions">
                {#if mayCancel(job)}
                  <Button
                    variant="secondary"
                    disabled={job.cancel_requested || acting === job.id}
                    onclick={() => {
                      cancelling = job;
                    }}
                  >
                    {m.cancelJob()}
                  </Button>
                {/if}
                {#if mayRetry(job)}
                  <Button
                    variant="secondary"
                    loading={acting === job.id}
                    onclick={() => void retry(job.id)}
                  >
                    {m.retryJob()}
                  </Button>
                {/if}
              </div>
            {/if}
          </li>
        {/each}
      </ul>
      {#if next !== null}
        <div class="more">
          <Button variant="ghost" loading={moreBusy} onclick={() => void load(true)}>
            {m.showMore()}
          </Button>
        </div>
      {/if}
    {/if}

    {#if cancelling !== null}
      <Dialog
        open={true}
        title={m.cancelTitle()}
        dismissible={acting === null}
        onclose={() => {
          if (acting === null) cancelling = null;
        }}
      >
        <p>{cancelling.state === 'running' ? m.cancelRunningText() : m.cancelQueuedText()}</p>
        {#snippet footer()}
          <Button
            disabled={acting !== null}
            onclick={() => {
              cancelling = null;
            }}
          >
            {common.cancel()}
          </Button>
          <Button variant="danger" loading={acting !== null} onclick={() => void confirmCancel()}>
            {m.cancelConfirm()}
          </Button>
        {/snippet}
      </Dialog>
    {/if}
  {/if}
</div>

<style>
  .jobs-page {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }
  .intro {
    margin: 0;
    padding: 0 var(--space-6);
    color: var(--color-text-muted);
  }
  .loading,
  .empty {
    margin: 0;
    color: var(--color-text-muted);
  }
  .filters {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    padding: 0 var(--space-6);
  }
  .states {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-1);
  }

  /* A short chip still makes a full touch target (design §11). */
  .states :global(.button) {
    min-width: var(--control-md);
  }
  .jobs {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    margin: 0;
    padding: 0 var(--space-6);
    list-style: none;
  }
  .job {
    display: flex;
    flex-direction: column;
    gap: var(--space-1_5);
    padding: var(--space-3);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
  }
  .what {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: var(--space-1) var(--space-3);
  }
  .kind {
    font-weight: var(--weight-medium);
  }
  .target {
    color: var(--color-text-muted);
    overflow-wrap: anywhere;
  }
  .who {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-1) var(--space-6);
  }
  .cell {
    display: inline-flex;
    align-items: baseline;
    gap: var(--space-2);
  }
  .label {
    color: var(--color-text-muted);
    font-size: var(--text-sm);
  }
  .meta {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-1) var(--space-3);
  }
  .state {
    display: inline-flex;
    align-items: center;
    padding: 0 var(--space-1_5);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-full);
    background: var(--color-surface-sunken);
    color: var(--color-text-muted);
    font-size: var(--text-xs);
    font-weight: var(--weight-medium);
    line-height: 1.5;
    white-space: nowrap;
  }
  .state[data-state='failed'] {
    border-color: var(--color-danger);
    color: var(--color-danger);
  }
  .state[data-state='running'] {
    border-color: var(--color-brand);
    color: var(--color-text);
  }
  .progress {
    color: var(--color-text-muted);
    font-size: var(--text-sm);
  }
  .error {
    margin: 0;
    color: var(--color-danger);
    font-size: var(--text-sm);
    overflow-wrap: anywhere;
  }
  .actions {
    display: flex;
    gap: var(--space-2);
  }
  .more {
    display: flex;
    justify-content: center;
  }
  @media (width < 48rem) {
    .intro,
    .filters,
    .jobs {
      padding-inline: var(--space-4);
    }
  }
</style>
