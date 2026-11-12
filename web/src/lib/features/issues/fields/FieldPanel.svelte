<script lang="ts">
  // The field panel of the issue page (ISS-020): the project first, with a link to its settings for
  // people who configure it (PRJ-016), then the issue's fields in the project's order. Private
  // fields carry a lock (FLD-062), empty required ones say so (FLD-057). Text fields are not here:
  // they are blocks under the description (ISS-001). Wide screens can collapse it to its header
  // (button or double click); on phones it is the section «Поля» between the header and content.
  // A value people may change is a button that opens its editor in a popover (a sheet on phones);
  // a saved change shows at once, is announced, and rolls back with the reason under the row
  // (ISS-019). Under the fields, the issue's boards and sprints (ISS-020, ISS-007).
  import ArrowRightLeft from '@lucide/svelte/icons/arrow-right-left';
  import PanelRightClose from '@lucide/svelte/icons/panel-right-close';
  import PanelRightOpen from '@lucide/svelte/icons/panel-right-open';
  import Lock from '@lucide/svelte/icons/lock';
  import Settings2 from '@lucide/svelte/icons/settings-2';
  import type { Snippet } from 'svelte';
  import { SvelteMap } from 'svelte/reactivity';

  import type { Issue, IssueBoard, IssueField } from '#lib/api/schema.gen.ts';
  import { IssueBoards } from '#lib/features/boards/index.ts';
  import { canConfigure, projectHref, ProjectLogo } from '#lib/features/projects/index.ts';
  import { refusalText } from '#lib/features/fields/index.ts';
  import { announce } from '#lib/ui/announce.svelte.ts';
  import IconButton from '#lib/ui/IconButton.svelte';
  import Popover from '#lib/ui/Popover.svelte';
  import { tooltip } from '#lib/ui/tooltip.ts';

  import * as m from '../messages.ts';
  import { isBlock, isEmpty, isMissing } from './field-values.ts';
  import FieldEditor from './FieldEditor.svelte';
  import FieldValue from './FieldValue.svelte';
  import { saveField, type FieldRequest } from './save.ts';

  interface Props {
    issue: Issue;
    collapsed: boolean;
    /** The panel is on the start side: the collapse icons point the other way. */
    side: 'start' | 'end';
    ontoggle: () => void;
    /** Opens the move dialog; without it the project cannot be changed here. */
    onmove?: () => void;
    /** Never collapsed: no toggle (the new-issue form). */
    fixed?: boolean;
    /** The project row's value instead of the link (the new-issue form's picker). */
    project?: Snippet;
    /** Fields a refused publish named as missing (FLD-057). */
    invalid?: readonly number[];
    /** The issue's boards from the page's answer; with `loadBoards` the section loads them. */
    boards?: IssueBoard[];
    loadBoards?: boolean;
    /** Fields whose values the place of creation sets (a board cell, BRD-077): shown, not edited. */
    locked?: { ids: readonly number[]; label: string };
  }

  let {
    issue,
    collapsed,
    side,
    ontoggle,
    onmove,
    fixed = false,
    project,
    invalid = [],
    boards,
    loadBoards = false,
    locked,
  }: Props = $props();

  const id = $props.id();
  const rows = $derived(issue.fields.filter((entry) => !isBlock(entry)));
  const settingsLabel = $derived(m.projectSettings({ name: issue.project.name }));
  // Collapsing moves the panel towards its own edge.
  const toggleIcon = $derived((side === 'end') === collapsed ? PanelRightOpen : PanelRightClose);

  /** Why the last change of a field failed, by field ID. */
  const problems = new SvelteMap<number, string>();

  function save(next: IssueField, request: FieldRequest): void {
    const fieldId = next.field.id;
    problems.delete(fieldId);
    saveField(issue.id, next, request).then(
      () => {
        announce(m.saved({ name: next.field.name }));
      },
      (error: unknown) => {
        const text = refusalText(error);
        problems.set(fieldId, text);
        announce(text, 'assertive');
      },
    );
  }
</script>

<section class={['panel', { collapsed }]} aria-labelledby="{id}-title">
  <!-- Double click is a shortcut for the button beside it. -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="head" ondblclick={fixed ? undefined : ontoggle}>
    <h2 id="{id}-title">{m.fields()}</h2>
    {#if !fixed}
      <IconButton
        class="toggle"
        icon={toggleIcon}
        label={collapsed ? m.expandPanel() : m.collapsePanel()}
        aria-expanded={!collapsed}
        aria-controls="{id}-rows"
        onclick={ontoggle}
      />
    {/if}
  </div>
  <dl class="rows" id="{id}-rows">
    <div class="row">
      <dt>{m.project()}</dt>
      <dd class="project">
        {#if project}
          {@render project()}
        {:else}
          <!-- Hrefs come from projectHref, which resolves them. -->
          <!-- eslint-disable svelte/no-navigation-without-resolve -->
          <a href={projectHref.overview(issue.project)}>
            <ProjectLogo
              projectKey={issue.project.key}
              color={issue.project.logo_color}
              src={issue.project.logo_url}
              size="sm"
            />
            <span>{issue.project.name}</span>
          </a>
          {#if canConfigure(issue.project)}
            <a
              class="settings"
              href={projectHref.settings(issue.project)}
              aria-label={settingsLabel}
              {@attach tooltip(settingsLabel)}
            >
              <Settings2 size={16} aria-hidden="true" />
            </a>
          {/if}
          <!-- eslint-enable svelte/no-navigation-without-resolve -->
          {#if onmove && issue.can.update}
            <IconButton
              icon={ArrowRightLeft}
              label={m.changeProject({ name: issue.project.name })}
              onclick={onmove}
            />
          {/if}
        {/if}
      </dd>
    </div>
    {#each rows as entry (entry.field.id)}
      {@const problem =
        problems.get(entry.field.id) ??
        (invalid.includes(entry.field.id) && isEmpty(entry) ? m.fillRequired() : undefined)}
      <div class="row">
        <dt>
          <span id="{id}-name-{entry.field.id}">{entry.field.name}</span>
          {#if entry.private}
            <span class="lock" role="img" aria-label={m.privateField()}>
              <Lock size={12} aria-hidden="true" />
            </span>
          {/if}
          {#if locked?.ids.includes(entry.field.id)}
            <span class="lock" role="img" aria-label={locked.label}>
              <Lock size={12} aria-hidden="true" />
            </span>
          {/if}
        </dt>
        <dd>
          {#snippet value()}
            {#if isMissing(entry)}
              <span class="missing">{m.requiredField()}</span>
            {:else}
              <FieldValue {entry} />
            {/if}
          {/snippet}
          {#if entry.editable && entry.field.kind !== 'text' && !locked?.ids.includes(entry.field.id)}
            <Popover label={m.fieldEditor({ name: entry.field.name })}>
              {#snippet trigger(props)}
                <button
                  type="button"
                  class="value"
                  data-invalid={problem !== undefined && invalid.includes(entry.field.id)
                    ? ''
                    : undefined}
                  aria-describedby={[
                    `${id}-name-${String(entry.field.id)}`,
                    problem ? `${id}-problem-${String(entry.field.id)}` : '',
                  ]
                    .join(' ')
                    .trim()}
                  {...props}>{@render value()}</button
                >
              {/snippet}
              {#snippet children({ close })}
                <FieldEditor issueId={issue.id} {entry} onsave={save} {close} />
              {/snippet}
            </Popover>
          {:else}
            {@render value()}
          {/if}
          {#if problem}
            <p class="problem" id="{id}-problem-{entry.field.id}">{problem}</p>
          {/if}
        </dd>
      </div>
    {/each}
  </dl>
  {#if boards !== undefined || loadBoards}
    <div class="extra">
      <IssueBoards
        issue={issue.id}
        project={issue.project.id}
        editable={issue.can.update}
        {boards}
      />
    </div>
  {/if}
</section>

<style>
  .panel {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    min-width: 0;
  }

  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2);
    min-height: var(--control-md);
  }

  h2 {
    font-size: var(--text-md);
    font-weight: var(--weight-semibold);
  }

  .collapsed h2 {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip-path: inset(50%);
    white-space: nowrap;
  }

  .collapsed .rows,
  .collapsed .extra {
    display: none;
  }

  .rows {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    margin: 0;
  }

  .row {
    display: flex;
    flex-direction: column;
    gap: var(--space-0_5);
    min-width: 0;
  }

  dt {
    display: flex;
    align-items: center;
    gap: var(--space-1);
    color: var(--color-text-muted);
    font-size: var(--text-sm);
  }

  dd {
    min-width: 0;
    margin: 0;
  }

  .project {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }

  .project a {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    min-height: var(--control-md);
    color: var(--color-link);
  }

  .settings {
    justify-content: center;
    min-width: var(--control-md);
  }

  .lock {
    display: inline-flex;
  }

  .missing {
    color: var(--color-danger);
  }

  .value {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-1);
    width: 100%;
    min-height: var(--control-md);
    margin-inline: calc(-1 * var(--space-2));
    padding: var(--space-1) var(--space-2);
    border: 0;
    border-radius: var(--radius-sm);
    background: transparent;
    color: inherit;
    font: inherit;
    text-align: start;
    cursor: pointer;
  }

  .value:hover,
  .value[aria-expanded='true'] {
    background: var(--color-hover);
  }

  .problem {
    color: var(--color-danger);
    font-size: var(--text-sm);
  }

  /* Phones: a section of name/value pairs in two columns, never collapsed. */
  @media (width < 48rem) {
    .head :global(.toggle) {
      display: none;
    }

    .collapsed h2 {
      position: static;
      width: auto;
      height: auto;
      overflow: visible;
      clip-path: none;
    }

    .collapsed .extra {
      display: block;
    }

    .collapsed .rows,
    .rows {
      display: grid;
      grid-template-columns: minmax(0, 2fr) minmax(0, 3fr);
      gap: var(--space-2) var(--space-3);
    }

    .row {
      display: contents;
    }

    dt {
      min-height: var(--control-md);
    }

    dd {
      display: flex;
      flex-direction: column;
      justify-content: center;
      min-height: var(--control-md);
    }
  }
</style>
