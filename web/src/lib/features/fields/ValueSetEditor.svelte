<script lang="ts">
  // The values of a set and their properties (FLD-029…FLD-049), used on a project's fields page and
  // on the set's own page. It says which projects use the set and whether the viewer may change it
  // (FLD-031); a shared set offers "Make a copy for this project" when opened from a project
  // (FLD-030). Values are listed in their order with their color, description and the properties
  // of their kind; the order changes by dragging or by "Move up/down" in each value's menu
  // (FLD-037). User sets list people and groups instead (FLD-040).
  import ArrowDown from '@lucide/svelte/icons/arrow-down';
  import ArrowUp from '@lucide/svelte/icons/arrow-up';
  import Archive from '@lucide/svelte/icons/archive';
  import ArchiveRestore from '@lucide/svelte/icons/archive-restore';
  import Check from '@lucide/svelte/icons/check';
  import Copy from '@lucide/svelte/icons/copy';
  import Ellipsis from '@lucide/svelte/icons/ellipsis';
  import ListPlus from '@lucide/svelte/icons/list-plus';
  import Pencil from '@lucide/svelte/icons/pencil';
  import Plus from '@lucide/svelte/icons/plus';
  import TextCursorInput from '@lucide/svelte/icons/text-cursor-input';
  import Trash2 from '@lucide/svelte/icons/trash-2';
  import X from '@lucide/svelte/icons/x';
  import { tick } from 'svelte';
  import { resolve } from '$app/paths';

  import type { ValueSet } from '#lib/api/schema.gen.ts';
  import { PersonName } from '#lib/features/access/index.ts';
  import * as common from '#lib/i18n/messages/common.ts';
  import { fieldErrorText } from '#lib/i18n/messages/errors.ts';
  import { can, canSeeFields } from '#lib/session/index.ts';
  import Time from '#lib/time/Time.svelte';
  import { announce } from '#lib/ui/announce.svelte.ts';
  import Button from '#lib/ui/Button.svelte';
  import Checkbox from '#lib/ui/Checkbox.svelte';
  import Dialog from '#lib/ui/Dialog.svelte';
  import EmptyState from '#lib/ui/EmptyState.svelte';
  import IconButton from '#lib/ui/IconButton.svelte';
  import Menu, { type MenuEntry } from '#lib/ui/Menu.svelte';
  import Notice from '#lib/ui/Notice.svelte';
  import Select from '#lib/ui/Select.svelte';
  import Spinner from '#lib/ui/Spinner.svelte';
  import { showError, toasts } from '#lib/ui/toast.svelte.ts';

  import Badge from './Badge.svelte';
  import {
    afterMovingDown,
    afterMovingUp,
    deleteValue,
    editValue,
    moveValue,
    setValues,
    useCount,
    valueUsage,
    type SetValue,
  } from './fields.ts';
  import * as m from './messages.ts';
  import { refusal, refusalText } from './problems.ts';
  import ReorderList from './ReorderList.svelte';
  import SetMembers from './SetMembers.svelte';
  import ValueChip from './ValueChip.svelte';
  import ValueDialog from './ValueDialog.svelte';

  interface Props {
    set: ValueSet;
    /** Level of the section heading. */
    level?: 2 | 3;
    /** Shows the set's name with a link to its page (the set's own page has it as its title). */
    showName?: boolean;
    /** Nothing may change: the project is archived. */
    readonly?: boolean;
    /** Opened from a project: forking gives it its own copy (FLD-030). */
    onfork?: () => unknown;
    /** Something in the set changed. */
    onchanged?: () => unknown;
  }

  let { set, level = 2, showName = false, readonly = false, onfork, onchanged }: Props = $props();

  const uid = $props.id();
  let showArchived = $state(true);
  let showReleased = $state(true);
  let dialogOpen = $state(false);
  let editing = $state<SetValue | null>(null);
  let addAfter = $state<SetValue | null>(null);
  let renaming = $state<number | null>(null);
  let renameText = $state('');
  let renameError = $state<string | null>(null);
  let renameBusy = $state(false);
  let deleting = $state<SetValue | null>(null);
  let deleteOpen = $state(false);
  let deleteBusy = $state(false);
  let deleteProblem = $state<string | null>(null);
  /** Issues holding the value being deleted; `null` while counting (FLD-032). */
  let usage = $state<number | null>(null);
  let usageError = $state<string | null>(null);
  /** The value issues get instead, as the select's value; '' clears the field. */
  let replacement = $state('');
  let usageCount: AbortController | null = null;
  let forking = $state(false);

  const all = $derived(setValues(set));
  const allIds = $derived(all.map((value) => value.id));
  const shown = $derived(
    all.filter(
      (value) =>
        (showArchived || !value.archived) &&
        (set.kind !== 'version' || showReleased || !value.released),
    ),
  );
  const shownIds = $derived(shown.map((value) => value.id));
  const editable = $derived(set.editable && !readonly);
  const projects = $derived(useCount(set));
  const shared = $derived(onfork !== undefined && projects > 1);
  const heading = $derived(`h${String(level)}`);
  const replacements = $derived([
    { value: '', label: m.noReplacement() },
    ...all
      .filter((value) => value.id !== deleting?.id && !value.archived)
      .map((value) => ({ value: String(value.id), label: value.name })),
  ]);

  function openAdd(after: SetValue | null): void {
    editing = null;
    addAfter = after;
    dialogOpen = true;
  }

  function openEdit(value: SetValue): void {
    editing = value;
    addAfter = null;
    dialogOpen = true;
  }

  async function changed(): Promise<void> {
    await onchanged?.();
  }

  async function move(value: SetValue, after: number | null | undefined): Promise<void> {
    if (after === undefined) return;
    try {
      const saved = await moveValue(set.id, value.id, after);
      const order = setValues(saved).map((item) => item.id);
      announce(
        m.moved({ name: value.name, position: order.indexOf(value.id) + 1, total: order.length }),
      );
      await changed();
    } catch (error) {
      showError(error);
    }
  }

  async function setFlag(
    value: SetValue,
    flag: 'archived' | 'resolved' | 'released',
    on: boolean,
  ): Promise<void> {
    try {
      await editValue(set.id, value, { [flag]: on });
      if (flag === 'archived') {
        toasts.show({
          tone: 'success',
          message: on
            ? m.valueArchived({ name: value.name })
            : m.valueRestored({ name: value.name }),
        });
      }
      await changed();
    } catch (error) {
      showError(error);
    }
  }

  function startRename(value: SetValue): void {
    renaming = value.id;
    renameText = value.name;
    renameError = null;
  }

  async function stopRename(value: SetValue): Promise<void> {
    renaming = null;
    renameError = null;
    await tick();
    document.getElementById(`${uid}-menu-${String(value.id)}`)?.focus();
  }

  async function saveRename(event: SubmitEvent, value: SetValue): Promise<void> {
    event.preventDefault();
    const text = renameText.trim();
    if (!text) {
      renameError = fieldErrorText.required();
      return;
    }
    if (text === value.name) {
      await stopRename(value);
      return;
    }
    renameBusy = true;
    try {
      await editValue(set.id, value, { name: text });
      await stopRename(value);
      await changed();
    } catch (error) {
      const found = refusal(error, { own: { name: { taken: m.valueNameTaken } }, paths: ['name'] });
      renameError = found.fields.name ?? found.message;
    } finally {
      renameBusy = false;
    }
  }

  function askToDelete(value: SetValue): void {
    deleting = value;
    deleteProblem = null;
    usage = null;
    usageError = null;
    replacement = '';
    deleteOpen = true;
    void countUsage(value.id);
  }

  /** Counts the issues holding the value; a later opening drops an earlier count. */
  async function countUsage(valueId: number): Promise<void> {
    usageCount?.abort();
    const controller = new AbortController();
    usageCount = controller;
    try {
      const found = await valueUsage(set.id, valueId, controller.signal);
      if (usageCount === controller) usage = found.issues;
    } catch (error) {
      if (usageCount === controller && !controller.signal.aborted) usageError = refusalText(error);
    }
  }

  async function confirmDelete(): Promise<void> {
    const value = deleting;
    if (!value) return;
    deleteBusy = true;
    deleteProblem = null;
    try {
      const instead = usage !== null && usage > 0 && replacement ? Number(replacement) : null;
      await deleteValue(set.id, value.id, instead);
      deleteOpen = false;
      toasts.show({ tone: 'success', message: m.valueDeleted() });
      await changed();
    } catch (error) {
      deleteProblem = refusalText(error);
    } finally {
      deleteBusy = false;
    }
  }

  async function makeCopy(): Promise<void> {
    if (!onfork) return;
    forking = true;
    try {
      await onfork();
      toasts.show({ tone: 'success', message: m.forked() });
    } catch (error) {
      showError(error);
    } finally {
      forking = false;
    }
  }

  function menuOf(value: SetValue): MenuEntry[] {
    const up = afterMovingUp(allIds, value.id, shownIds);
    const down = afterMovingDown(allIds, value.id, shownIds);
    return [
      { type: 'action', label: m.edit(), icon: Pencil, onSelect: () => openEdit(value) },
      {
        type: 'action',
        label: m.rename(),
        icon: TextCursorInput,
        onSelect: () => startRename(value),
      },
      {
        type: 'action',
        label: m.addValueAfter(),
        icon: ListPlus,
        onSelect: () => openAdd(value),
      },
      { type: 'separator' },
      {
        type: 'action',
        label: m.moveUp(),
        icon: ArrowUp,
        disabled: up === undefined,
        onSelect: () => void move(value, up),
      },
      {
        type: 'action',
        label: m.moveDown(),
        icon: ArrowDown,
        disabled: down === undefined,
        onSelect: () => void move(value, down),
      },
      {
        type: 'action',
        label: value.archived ? m.restore() : m.archive(),
        icon: value.archived ? ArchiveRestore : Archive,
        onSelect: () => void setFlag(value, 'archived', !value.archived),
      },
      { type: 'separator' },
      {
        type: 'action',
        label: m.deleteAction(),
        icon: Trash2,
        danger: true,
        onSelect: () => askToDelete(value),
      },
    ];
  }

  function focusAndSelect(input: HTMLInputElement): void {
    input.focus();
    input.select();
  }
</script>

<section class="editor" aria-labelledby="{uid}-title">
  <div class="head">
    <svelte:element this={heading} id="{uid}-title" class="title">{m.values()}</svelte:element>
    {#if editable && set.kind !== 'user'}
      <Button icon={Plus} onclick={() => openAdd(null)}>{m.addValue()}</Button>
    {/if}
  </div>

  <div class="about">
    {#if showName}
      <p>
        {m.valueSet()}:
        {#if canSeeFields()}
          <a href={resolve('/(app)/admin/value-sets/[id]', { id: String(set.id) })}>{set.name}</a>
        {:else}
          {set.name}
        {/if}
      </p>
    {/if}
    {#if set.used_in.items.length > 0}
      <p>{m.usedInNames({ names: set.used_in.items.map((project) => project.name).join(', ') })}</p>
    {/if}
    {#if set.used_in.hidden > 0}<p>{m.usedInHidden({ count: set.used_in.hidden })}</p>{/if}
    {#if projects === 0}<p>{m.setNotUsed()}</p>{/if}
    {#if set.field_count > 0}<p>{m.defaultSetOf({ count: set.field_count })}</p>{/if}
  </div>

  {#if !readonly && !set.editable}
    <Notice>
      <p>{onfork ? m.notEditableInProject() : m.notEditable()}</p>
      {#if onfork}
        <p>{m.forkOffer()}</p>
        <div class="notice-action">
          <Button icon={Copy} loading={forking} onclick={makeCopy}>{m.fork()}</Button>
        </div>
      {/if}
    </Notice>
  {:else if editable && shared}
    <Notice tone="warning">
      <p>{m.sharedNotice({ count: projects })}</p>
      <p>{m.forkOffer()}</p>
      <div class="notice-action">
        <Button icon={Copy} loading={forking} onclick={makeCopy}>{m.fork()}</Button>
      </div>
    </Notice>
  {/if}

  {#if set.kind === 'user'}
    <SetMembers
      {set}
      level={level === 2 ? 3 : 4}
      editable={editable && can('user.directory')}
      onchanged={changed}
    />
  {:else}
    {#if all.some((value) => value.archived) || (set.kind === 'version' && all.some((value) => value.released))}
      <div class="options">
        {#if all.some((value) => value.archived)}
          <Checkbox label={m.showArchived()} bind:checked={showArchived} />
        {/if}
        {#if set.kind === 'version' && all.some((value) => value.released)}
          <Checkbox label={m.showReleased()} bind:checked={showReleased} />
        {/if}
      </div>
    {/if}

    {#if all.length === 0}
      <EmptyState level={3} title={m.noValues()} description={m.noValuesText()} />
    {:else if shown.length === 0}
      <p class="note">{m.allValuesHidden()}</p>
    {:else}
      <ReorderList
        items={shown}
        key={(value: SetValue) => value.id}
        label={m.valueList({ name: set.name })}
        sortable={editable}
        onmove={(id: number, after: number | null) => {
          const value = all.find((item) => item.id === id);
          if (value) void move(value, after);
        }}
      >
        {#snippet row(value: SetValue)}
          <div class={['value', { archived: value.archived }]}>
            <div class="line">
              <div class="name">
                {#if renaming === value.id}
                  <form class="rename" novalidate onsubmit={(event) => saveRename(event, value)}>
                    <input
                      class="rename-input"
                      aria-label={m.renameLabel({ name: value.name })}
                      bind:value={renameText}
                      maxlength={100}
                      autocomplete="off"
                      aria-invalid={renameError ? 'true' : undefined}
                      aria-describedby={renameError ? `${uid}-rename-error` : undefined}
                      onkeydown={(event) => {
                        if (event.key === 'Escape') {
                          event.preventDefault();
                          void stopRename(value);
                        }
                      }}
                      {@attach focusAndSelect}
                    />
                    <IconButton icon={Check} label={m.save()} type="submit" disabled={renameBusy} />
                    <IconButton
                      icon={X}
                      label={common.cancel()}
                      onclick={() => stopRename(value)}
                    />
                  </form>
                {:else}
                  <ValueChip name={value.name} color={value.color} />
                {/if}
                {#if value.archived}<Badge icon={Archive} text={m.archivedBadge()} />{/if}
                {#if !editable && set.kind === 'state' && value.resolved}
                  <Badge icon={Check} text={m.resolved()} />
                {/if}
                {#if !editable && set.kind === 'version' && value.released}
                  <Badge icon={Check} text={m.released()} />
                {/if}
              </div>
              {#if editable && (set.kind === 'state' || set.kind === 'version')}
                {@const flag = set.kind === 'state' ? 'resolved' : 'released'}
                {@const text = set.kind === 'state' ? m.resolved() : m.released()}
                <label class="flag">
                  <input
                    type="checkbox"
                    checked={value[flag]}
                    aria-label={m.flagFor({ flag: text, name: value.name })}
                    onchange={(event) => setFlag(value, flag, event.currentTarget.checked)}
                  />
                  <span aria-hidden="true">{text}</span>
                </label>
              {/if}
              {#if editable}
                <Menu label={m.valueActions({ name: value.name })} items={menuOf(value)}>
                  {#snippet trigger(props)}
                    <IconButton
                      id="{uid}-menu-{value.id}"
                      icon={Ellipsis}
                      label={m.valueActions({ name: value.name })}
                      {...props}
                    />
                  {/snippet}
                </Menu>
              {/if}
            </div>
            {#if renaming === value.id && renameError}
              <p id="{uid}-rename-error" class="error">{renameError}</p>
            {/if}
            {#if value.description}<p class="description">{value.description}</p>{/if}
            {#if set.kind === 'version' && (value.release_date !== null || value.start_date !== null)}
              <p class="details">
                {#if value.release_date}
                  <span>{m.releaseLine()} <Time value={value.release_date} kind="date" /></span>
                {/if}
                {#if value.start_date}
                  <span>{m.startLine()} <Time value={value.start_date} kind="date" /></span>
                {/if}
              </p>
            {:else if set.kind === 'build' && value.assembled_on}
              <p class="details">
                <span>{m.builtLine()} <Time value={value.assembled_on} kind="date" /></span>
              </p>
            {:else if set.kind === 'owned' && value.owner}
              <p class="details owner">
                <span>{m.ownerLine()}</span>
                <PersonName person={value.owner} />
              </p>
            {/if}
          </div>
        {/snippet}
      </ReorderList>
    {/if}
  {/if}
</section>

{#if editable && set.kind !== 'user'}
  <ValueDialog bind:open={dialogOpen} {set} value={editing} after={addAfter} onsaved={changed} />

  <Dialog
    bind:open={deleteOpen}
    title={m.deleteValueTitle({ name: deleting?.name ?? '' })}
    size="sm"
    dismissible={!deleteBusy}
  >
    <div class="confirm">
      {#if deleteProblem}<Notice tone="danger">{deleteProblem}</Notice>{/if}
      {#if usageError}
        <Notice tone="danger">{usageError}</Notice>
        <p>{m.deleteValueText()}</p>
      {:else if usage === null}
        <Spinner label={m.impactLoading()} />
      {:else if usage > 0}
        <p>{m.deleteValueUsed({ count: usage })}</p>
        <Select
          label={m.replacementLabel()}
          options={replacements}
          hint={m.replacementHint()}
          disabled={deleteBusy}
          bind:value={replacement}
        />
        <p>{m.deleteValueUsedNote()}</p>
      {:else}
        <p>{m.deleteValueText()}</p>
      {/if}
      {#if projects > 1}<p>{m.sharedNotice({ count: projects })}</p>{/if}
    </div>
    {#snippet footer()}
      <Button
        disabled={deleteBusy}
        onclick={() => {
          deleteOpen = false;
        }}
      >
        {common.cancel()}
      </Button>
      <Button
        variant="danger"
        loading={deleteBusy}
        disabled={usage === null && !usageError}
        onclick={confirmDelete}
      >
        {m.deleteValueButton()}
      </Button>
    {/snippet}
  </Dialog>
{/if}

<style>
  .editor {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }

  .head {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2);
  }

  .title {
    font-size: var(--text-lg);
  }

  .about,
  .note {
    display: flex;
    flex-direction: column;
    gap: var(--space-0_5);
    color: var(--color-text-muted);
    font-size: var(--text-sm);
  }

  .about a {
    color: var(--color-link);
  }

  .notice-action {
    margin-top: var(--space-2);
  }

  .options {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-1) var(--space-4);
  }

  .value {
    display: flex;
    flex-direction: column;
    gap: var(--space-0_5);
    padding: var(--space-1) var(--space-2) var(--space-1) var(--space-3);
  }

  .line {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    min-height: var(--control-md);
  }

  .name {
    display: flex;
    flex: 1;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-1) var(--space-2);
    min-width: 0;
  }

  .archived .name :global(.chip) {
    opacity: 0.75;
  }

  .flag {
    display: inline-flex;
    flex: none;
    align-items: center;
    gap: var(--space-1_5);
    min-height: var(--control-md);
    padding: 0 var(--space-1);
    color: var(--color-text-muted);
    font-size: var(--text-sm);
    cursor: pointer;
  }

  .flag input {
    width: 1rem;
    height: 1rem;
    margin: 0;
    accent-color: var(--color-ink);
  }

  .description,
  .details {
    color: var(--color-text-muted);
    font-size: var(--text-sm);
    overflow-wrap: anywhere;
  }

  .details {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-1) var(--space-3);
  }

  .details.owner {
    gap: var(--space-1) var(--space-2);
  }

  .rename {
    display: flex;
    flex: 1;
    align-items: center;
    gap: var(--space-1);
    min-width: 0;
  }

  .rename-input {
    flex: 1;
    min-width: 0;
    height: var(--control-md);
    padding: 0 var(--space-2);
    border: 1px solid var(--color-border-control);
    border-radius: var(--radius-md);
    background: var(--color-surface);
    color: var(--color-text);
    font: inherit;
  }

  .rename-input[aria-invalid='true'] {
    border-color: var(--color-danger);
  }

  .error {
    color: var(--color-danger);
    font-size: var(--text-sm);
  }

  .confirm {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }

  @media (pointer: coarse) {
    .flag input {
      width: 1.25rem;
      height: 1.25rem;
    }

    .rename-input {
      font-size: var(--text-lg);
    }
  }
</style>
