<script lang="ts">
  // A project's fields (FLD-051…FLD-060): in the project's order with type, values, default value,
  // empty handling and privacy. The order changes by dragging or by "Move up/down" in each row's
  // menu (FLD-058). Choosing a field opens its settings and values beside the list on wide screens
  // and in a sheet on phones. "Add field" attaches an existing field or creates one (FLD-052,
  // FLD-053); removing one asks first (FLD-054). An archived project shows all of it read-only.
  import ArrowDown from '@lucide/svelte/icons/arrow-down';
  import ArrowUp from '@lucide/svelte/icons/arrow-up';
  import Asterisk from '@lucide/svelte/icons/asterisk';
  import Ellipsis from '@lucide/svelte/icons/ellipsis';
  import KeyRound from '@lucide/svelte/icons/key-round';
  import ListPlus from '@lucide/svelte/icons/list-plus';
  import Plus from '@lucide/svelte/icons/plus';
  import Settings2 from '@lucide/svelte/icons/settings-2';
  import Trash2 from '@lucide/svelte/icons/trash-2';
  import X from '@lucide/svelte/icons/x';
  import { onMount, tick, untrack } from 'svelte';
  import { MediaQuery } from 'svelte/reactivity';

  import type { Project, ProjectField } from '#lib/api/schema.gen.ts';
  import { personName } from '#lib/features/access/index.ts';
  import * as common from '#lib/i18n/messages/common.ts';
  import { announce } from '#lib/ui/announce.svelte.ts';
  import Button from '#lib/ui/Button.svelte';
  import Dialog from '#lib/ui/Dialog.svelte';
  import EmptyState from '#lib/ui/EmptyState.svelte';
  import ErrorState from '#lib/ui/ErrorState.svelte';
  import IconButton from '#lib/ui/IconButton.svelte';
  import Menu, { type MenuEntry } from '#lib/ui/Menu.svelte';
  import Notice from '#lib/ui/Notice.svelte';
  import PageHeader from '#lib/ui/PageHeader.svelte';
  import Spinner from '#lib/ui/Spinner.svelte';
  import { showError, toasts } from '#lib/ui/toast.svelte.ts';

  import AddFieldDialog from './AddFieldDialog.svelte';
  import Badge from './Badge.svelte';
  import { afterMovingDown, afterMovingUp, detachImpact } from './fields.ts';
  import * as m from './messages.ts';
  import { refusalText, typeLabel } from './problems.ts';
  import ProjectFieldPanel from './ProjectFieldPanel.svelte';
  import { ProjectFieldList } from './project-fields.svelte.ts';
  import ReorderList from './ReorderList.svelte';
  import ValueChip from './ValueChip.svelte';

  let { project }: { project: Project } = $props();

  const uid = $props.id();
  const list = new ProjectFieldList(untrack(() => project.id));
  /** Wide enough for the settings beside the list. */
  const wide = new MediaQuery('(width >= 64rem)', true);
  let selected = $state<number | null>(null);
  let sheetOpen = $state(false);
  let adding = $state(false);
  let detaching = $state<ProjectField | null>(null);
  let detachOpen = $state(false);
  let detachBusy = $state(false);
  let detachProblem = $state<string | null>(null);
  /** Issues of the project that lose a value of the field being removed; `null` while counting. */
  let detachIssues = $state<number | null>(null);
  let detachImpactError = $state<string | null>(null);
  let detachCount: AbortController | null = null;

  const archived = $derived(project.archived_at !== null);
  const items = $derived(list.items);
  const ids = $derived(items.map((item) => item.field.id));
  const current = $derived(selected === null ? undefined : list.find(selected));
  const states = $derived(items.filter((item) => item.field.kind === 'state').length);

  onMount(() => {
    list.load().catch(() => undefined);
  });

  function open(field: ProjectField): void {
    selected = field.field.id;
    sheetOpen = !wide.current;
  }

  async function closePanel(): Promise<void> {
    const id = selected;
    selected = null;
    sheetOpen = false;
    await tick();
    if (id !== null) document.getElementById(`${uid}-field-${String(id)}`)?.focus();
  }

  async function move(field: ProjectField, after: number | null | undefined): Promise<void> {
    if (after === undefined) return;
    try {
      await list.move(field.field.id, after);
      const order = list.items.map((item) => item.field.id);
      announce(
        m.moved({
          name: field.field.name,
          position: order.indexOf(field.field.id) + 1,
          total: order.length,
        }),
      );
    } catch (error) {
      showError(error);
    }
  }

  function askToDetach(field: ProjectField): void {
    detaching = field;
    detachProblem = null;
    detachIssues = null;
    detachImpactError = null;
    detachOpen = true;
    void countDetached(field.field.id);
  }

  /** Counts the issues that lose a value (FLD-054); a later opening drops an earlier count. */
  async function countDetached(fieldId: number): Promise<void> {
    detachCount?.abort();
    const controller = new AbortController();
    detachCount = controller;
    try {
      const impact = await detachImpact(project.id, fieldId, controller.signal);
      if (detachCount === controller) detachIssues = impact.issues;
    } catch (error) {
      if (detachCount === controller && !controller.signal.aborted) {
        detachImpactError = refusalText(error);
      }
    }
  }

  async function confirmDetach(): Promise<void> {
    const field = detaching;
    if (!field) return;
    detachBusy = true;
    detachProblem = null;
    try {
      await list.detach(field.field.id);
      detachOpen = false;
      if (selected === field.field.id) {
        selected = null;
        sheetOpen = false;
      }
      toasts.show({ tone: 'success', message: m.detached() });
    } catch (error) {
      detachProblem = refusalText(error);
    } finally {
      detachBusy = false;
    }
  }

  async function added(field: ProjectField): Promise<void> {
    await tick();
    open(field);
  }

  function menuOf(field: ProjectField): MenuEntry[] {
    const up = afterMovingUp(ids, field.field.id);
    const down = afterMovingDown(ids, field.field.id);
    return [
      { type: 'action', label: m.settings(), icon: Settings2, onSelect: () => open(field) },
      { type: 'separator' },
      {
        type: 'action',
        label: m.moveUp(),
        icon: ArrowUp,
        disabled: up === undefined,
        onSelect: () => void move(field, up),
      },
      {
        type: 'action',
        label: m.moveDown(),
        icon: ArrowDown,
        disabled: down === undefined,
        onSelect: () => void move(field, down),
      },
      { type: 'separator' },
      {
        type: 'action',
        label: m.detach(),
        icon: Trash2,
        danger: true,
        onSelect: () => askToDetach(field),
      },
    ];
  }

  /** Type, values and how many other projects share them, as one line. */
  function kindLine(field: ProjectField): string {
    const parts = [typeLabel(field.field)];
    if (field.set) {
      const shared =
        field.shared_with > 0 ? ` (${m.sharedWith({ count: field.shared_with })})` : '';
      parts.push(`${m.setLine({ name: field.set.name })}${shared}`);
    }
    return parts.join(' · ');
  }

  function emptyText(field: ProjectField): string | null {
    if (field.required) return null;
    if (!field.can_be_empty) return m.cannotBeEmpty();
    return field.empty_label ? m.emptyShown({ label: field.empty_label }) : m.canBeEmptyShort();
  }
</script>

<svelte:head><title>{m.fieldsTitle()} · {project.name} · TaskMaster</title></svelte:head>

<PageHeader title={m.fieldsTitle()}>
  {#snippet actions()}
    {#if !archived}
      <Button
        variant="primary"
        icon={Plus}
        onclick={() => {
          adding = true;
        }}
      >
        {m.addField()}
      </Button>
    {/if}
  {/snippet}
</PageHeader>

<div class={['page', { split: wide.current && current }]}>
  <div class="main">
    <p class="intro">{m.projectFieldsIntro()}</p>
    {#if states > 1}<Notice tone="warning">{m.severalStates()}</Notice>{/if}

    {#if !list.loaded && list.error}
      <ErrorState error={list.error} level={2} onretry={() => list.load()} />
    {:else if !list.loaded}
      <p class="note"><Spinner label={m.fieldsLoading()} /></p>
    {:else if items.length === 0}
      <EmptyState icon={ListPlus} title={m.noProjectFields()} description={m.noProjectFieldsText()}>
        {#snippet actions()}
          {#if !archived}
            <Button
              icon={Plus}
              onclick={() => {
                adding = true;
              }}
            >
              {m.addField()}
            </Button>
          {/if}
        {/snippet}
      </EmptyState>
    {:else}
      <ReorderList
        {items}
        key={(item: ProjectField) => item.field.id}
        label={m.projectFieldList()}
        sortable={!archived}
        current={selected}
        busy={list.loading}
        onmove={(id: number, after: number | null) => {
          const field = list.find(id);
          if (field) void move(field, after);
        }}
      >
        {#snippet row(field: ProjectField)}
          {@const empty = emptyText(field)}
          <div class="field">
            <div class="line">
              <span class="title">
                <button
                  id="{uid}-field-{field.field.id}"
                  type="button"
                  class="name"
                  aria-current={selected === field.field.id ? 'true' : undefined}
                  aria-describedby="{uid}-meta-{field.field.id}"
                  onclick={() => {
                    open(field);
                  }}
                >
                  {field.field.name}
                </button>
                {#if field.private}<Badge icon={KeyRound} text={m.privateBadge()} />{/if}
                {#if field.required}<Badge icon={Asterisk} text={m.requiredBadge()} />{/if}
              </span>
              {#if !archived}
                <Menu label={m.fieldActions({ name: field.field.name })} items={menuOf(field)}>
                  {#snippet trigger(props)}
                    <IconButton
                      icon={Ellipsis}
                      label={m.fieldActions({ name: field.field.name })}
                      {...props}
                    />
                  {/snippet}
                </Menu>
              {/if}
            </div>
            <div id="{uid}-meta-{field.field.id}" class="meta">
              <p>{kindLine(field)}</p>
              {#if field.set}
                <p class="defaults">
                  <span>{m.defaultLine()}</span>
                  {#if field.field.kind === 'user' && field.default_users.length > 0}
                    <span>{field.default_users.map((person) => personName(person)).join(', ')}</span
                    >
                  {:else if field.default_values.length > 0}
                    {#each field.default_values as value (value.id)}
                      <ValueChip name={value.name} color={value.color} />
                    {/each}
                  {:else}
                    <span>{m.noDefault()}</span>
                  {/if}
                </p>
              {/if}
              {#if empty}<p>{empty}</p>{/if}
            </div>
          </div>
        {/snippet}
      </ReorderList>
    {/if}
  </div>

  {#if wide.current && current}
    <aside class="side" aria-labelledby="{uid}-panel-title">
      <div class="side-head">
        <h2 id="{uid}-panel-title">{m.fieldSettings({ name: current.field.name })}</h2>
        <IconButton icon={X} label={m.closeSettings()} onclick={closePanel} />
      </div>
      {#key current.field.id}
        {@const shown = current}
        <ProjectFieldPanel
          field={shown}
          {list}
          readonly={archived}
          ondetach={() => {
            askToDetach(shown);
          }}
        />
      {/key}
    </aside>
  {/if}
</div>

{#if !wide.current}
  <Dialog
    bind:open={sheetOpen}
    title={current ? m.fieldSettings({ name: current.field.name }) : m.settings()}
    size="lg"
    onclose={() => {
      selected = null;
    }}
  >
    {#if current}
      {#key current.field.id}
        {@const shown = current}
        <ProjectFieldPanel
          field={shown}
          {list}
          readonly={archived}
          ondetach={() => {
            askToDetach(shown);
          }}
        />
      {/key}
    {/if}
  </Dialog>
{/if}

{#if !archived}
  <AddFieldDialog bind:open={adding} {list} onadded={added} />

  <Dialog
    bind:open={detachOpen}
    title={m.detachTitle({ name: detaching?.field.name ?? '' })}
    size="sm"
    dismissible={!detachBusy}
  >
    <div class="confirm">
      {#if detachProblem}<Notice tone="danger">{detachProblem}</Notice>{/if}
      {#if detachImpactError}
        <Notice tone="danger">{detachImpactError}</Notice>
        <p>{m.detachText()}</p>
      {:else if detachIssues === null}
        <Spinner label={m.impactLoading()} />
      {:else if detachIssues > 0}
        <p>{m.detachIssues({ count: detachIssues })}</p>
      {:else}
        <p>{m.detachNoIssues()}</p>
      {/if}
    </div>
    {#snippet footer()}
      <Button
        disabled={detachBusy}
        onclick={() => {
          detachOpen = false;
        }}
      >
        {common.cancel()}
      </Button>
      <Button
        variant="danger"
        loading={detachBusy}
        disabled={detachIssues === null && !detachImpactError}
        onclick={confirmDetach}
      >
        {m.detachButton()}
      </Button>
    {/snippet}
  </Dialog>
{/if}

<style>
  .page {
    display: grid;
    grid-template-columns: minmax(0, 1fr);
    gap: var(--space-6);
    padding: var(--space-2) var(--space-6) var(--space-12);
  }

  .page.split {
    grid-template-columns: minmax(0, 1fr) minmax(22rem, 28rem);
    align-items: start;
  }

  .main {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
    min-width: 0;
  }

  .intro,
  .note {
    max-width: 44rem;
    color: var(--color-text-muted);
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: var(--space-0_5);
    padding: var(--space-1) var(--space-2) var(--space-2) var(--space-3);
  }

  .line {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }

  .title {
    display: flex;
    flex: 1;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-1) var(--space-2);
    min-width: 0;
  }

  .name {
    display: inline-flex;
    align-items: center;
    min-width: var(--control-md);
    min-height: var(--control-md);
    padding: 0;
    border: none;
    background: none;
    color: var(--color-text);
    font: inherit;
    font-weight: var(--weight-medium);
    text-align: start;
    overflow-wrap: anywhere;
    cursor: pointer;
  }

  .name:hover {
    color: var(--color-link);
    text-decoration: underline;
  }

  .name[aria-current='true'] {
    font-weight: var(--weight-semibold);
  }

  .meta {
    display: flex;
    flex-direction: column;
    gap: var(--space-0_5);
    color: var(--color-text-muted);
    font-size: var(--text-sm);
    overflow-wrap: anywhere;
  }

  .defaults {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-1);
  }

  .side {
    position: sticky;
    top: var(--space-4);
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
    max-height: calc(100dvh - var(--space-8));
    padding: var(--space-4) var(--space-5) var(--space-5);
    overflow-y: auto;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-lg);
    background: var(--color-surface);
  }

  .side-head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: var(--space-2);
  }

  .side-head h2 {
    font-size: var(--text-lg);
    overflow-wrap: anywhere;
  }

  .confirm {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }

  @media (width < 48rem) {
    .page {
      padding: var(--space-2) var(--space-4) var(--space-8);
    }
  }
</style>
