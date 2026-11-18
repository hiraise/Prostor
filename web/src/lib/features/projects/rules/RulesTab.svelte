<script lang="ts">
  // The project's built-in rules (AUT-023): one row per rule of the registry — its name, a switch
  // that PATCHes the rule with the project's version, and what the rule still lacks as a gray note
  // linking to the «Fields» tab. A `conflict` reloads the tab and asks to repeat; without the
  // settings right the switches are disabled with a tooltip. Rows are cards on phones (§11).
  import { resolve } from '$app/paths';
  import { onMount } from 'svelte';

  import { api, ApiError } from '#lib/api/client.ts';
  import type { NeedView, Project, RuleView } from '#lib/api/schema.gen.ts';
  import ErrorState from '#lib/ui/ErrorState.svelte';
  import PageHeader from '#lib/ui/PageHeader.svelte';
  import Spinner from '#lib/ui/Spinner.svelte';
  import { showError, toasts } from '#lib/ui/toast.svelte.ts';

  import { canConfigure } from '../projects.ts';
  import * as m from './messages.ts';

  let { project }: { project: Project } = $props();

  const uid = $props.id();
  /** The registry with the project's toggles; `null` before the first load, `failed` holds its error. */
  let items = $state<RuleView[] | null>(null);
  let failed = $state<unknown>(null);
  let version = $state(1);
  /** The rule being toggled; its switch waits. */
  let busy = $state<string | null>(null);
  const allowed = $derived(canConfigure(project));
  const fieldsHref = $derived(
    resolve('/(app)/projects/[key]/settings/fields', { key: project.key }),
  );

  async function load(): Promise<void> {
    failed = null;
    try {
      const answer = await api.get('/projects/{id}/rules', { path: { id: project.id } });
      items = answer.items;
      version = answer.version;
    } catch (error) {
      failed = error;
    }
  }

  onMount(() => {
    void load();
  });

  /** Turns a rule on or off; the answer is the whole tab again (§3.11). */
  async function toggle(rule: RuleView): Promise<void> {
    if (busy !== null) return;
    const enabled = !rule.enabled;
    busy = rule.rule;
    // The row shows the choice at once; a refusal puts the server's state back.
    const before = items;
    items = (items ?? []).map((row) => (row.rule === rule.rule ? { ...row, enabled } : row));
    try {
      const answer = await api.patch('/projects/{id}/rules', {
        path: { id: project.id },
        body: { rule: rule.rule, enabled, version },
      });
      items = answer.items;
      version = answer.version;
    } catch (error) {
      items = before;
      if (error instanceof ApiError && error.code === 'conflict') {
        toasts.show({ tone: 'error', message: m.changedRetry() });
        await load();
      } else {
        showError(error);
      }
    } finally {
      busy = null;
    }
  }

  /** What one lack of a rule reads as; the values are already names in the instance language. */
  function needText(need: NeedView): string {
    if (need.code === 'field') return m.needField({ name: need.params.name ?? '' });
    return m.needValue({ field: need.params.field ?? '', value: need.params.value ?? null });
  }
</script>

<svelte:head><title>{m.title()} · {project.name} · TaskMaster</title></svelte:head>

<PageHeader title={m.title()} />
<p class="lead">{m.lead()}</p>

{#if failed !== null}
  <ErrorState error={failed} onretry={load} />
{:else if items === null}
  <div class="loading"><Spinner /></div>
{:else}
  <ul class="rules">
    {#each items as rule (rule.rule)}
      <li class="row" class:off={!rule.enabled}>
        <div class="line">
          <span id="{uid}-{rule.rule}" class="name">{rule.name}</span>
          <button
            type="button"
            role="switch"
            aria-checked={rule.enabled}
            aria-labelledby="{uid}-{rule.rule}"
            aria-describedby={rule.needs_setup.length > 0 ? `${uid}-${rule.rule}-note` : undefined}
            disabled={!allowed || busy === rule.rule}
            title={!allowed ? m.noRights() : undefined}
            onclick={() => void toggle(rule)}
          >
            <span class="knob" aria-hidden="true"></span>
          </button>
        </div>
        {#if rule.needs_setup.length > 0}
          <p id="{uid}-{rule.rule}-note" class="note">
            {m.needSetup()}
            {rule.needs_setup.map(needText).join('; ')}.
            <a href={fieldsHref}>{m.openFields()}</a>
          </p>
        {/if}
      </li>
    {/each}
  </ul>
{/if}

<style>
  .lead {
    margin: 0;
    padding: 0 var(--space-6);
    color: var(--color-text-muted);
  }

  .loading {
    display: flex;
    justify-content: center;
    padding: var(--space-8);
  }

  .rules {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    margin: var(--space-3) var(--space-4) var(--space-6);
    padding: 0;
    list-style: none;
  }

  .row {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    padding: var(--space-2) var(--space-2);
    border-radius: var(--radius-md);
  }

  .row:hover {
    background: var(--color-hover);
  }

  .line {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3);
    min-height: var(--control-sm);
    padding-inline-start: var(--space-2);
  }

  .name {
    overflow-wrap: anywhere;
  }

  /* The switch: a track with a knob; the state never rests on color alone (UI-032). The whole
     control is the target, so it keeps the smallest touch size (§11). */
  button {
    flex: none;
    display: flex;
    align-items: center;
    width: 3rem;
    height: 2rem;
    padding: 2px;
    border: 1px solid var(--color-border-control);
    border-radius: var(--radius-full);
    background: var(--color-surface);
    cursor: pointer;
    transition: background 120ms ease;
  }

  button[aria-checked='true'] {
    background: var(--color-brand);
    border-color: var(--color-brand);
    justify-content: flex-end;
  }

  button:disabled {
    cursor: not-allowed;
    opacity: 0.6;
  }

  .knob {
    width: calc(2rem - 6px);
    height: calc(2rem - 6px);
    border-radius: var(--radius-full);
    background: var(--color-surface);
    box-shadow: 0 1px 2px rgb(0 0 0 / 25%);
    transition: background 120ms ease;
  }

  button:not([aria-checked='true']) .knob {
    background: var(--color-border-control);
  }

  .note {
    margin: 0;
    padding-inline-start: var(--space-2);
    color: var(--color-text-muted);
    font-size: var(--text-sm);
  }

  .note a {
    display: inline-flex;
    align-items: center;
    min-height: 2rem;
    color: var(--color-link);
  }

  @media (pointer: coarse) {
    .line {
      min-height: 2.75rem;
    }

    button {
      width: 3.5rem;
      height: 2.75rem;
    }

    .knob {
      width: calc(2.75rem - 8px);
      height: calc(2.75rem - 8px);
    }

    .note a {
      min-height: 2.75rem;
    }
  }

  /* Rows become cards on phones (§11). */
  @media (width < 48rem) {
    .rules {
      gap: var(--space-2);
      margin: var(--space-3) var(--space-4) var(--space-6);
    }

    .row {
      gap: var(--space-1_5);
      padding: var(--space-3);
      border: 1px solid var(--color-border);
      background: var(--color-surface);
    }

    .row:hover {
      background: var(--color-surface);
    }

    .note {
      padding-inline-start: 0;
    }
  }

  @media (forced-colors: active) {
    button {
      border-color: ButtonBorder;
    }

    button[aria-checked='true'] .knob {
      background: HighlightText;
    }
  }
</style>
