<script lang="ts" module>
  /** One context the picker offers. */
  export interface ContextOption {
    key: string;
    kind: 'all' | 'project' | 'tag' | 'search';
    name: string;
    /** A project's key, shown next to its name. */
    detail: string | null;
    context: ListContext;
  }
</script>

<script lang="ts">
  // The list's context button (QRY-053): all issues, a project, a tag or a saved search. The
  // choices load when it opens; the search narrows them by name or key.
  import ChevronDown from '@lucide/svelte/icons/chevron-down';

  import { api } from '#lib/api/client.ts';
  import { findProjects } from '#lib/features/projects/index.ts';
  import OptionList from '#lib/ui/OptionList.svelte';
  import Popover from '#lib/ui/Popover.svelte';

  import type { ListContext, ResolvedContext } from './list-state.ts';
  import * as m from './messages.ts';

  interface Props {
    context: ResolvedContext;
    onchoose: (context: ListContext) => void;
  }

  let { context, onchoose }: Props = $props();

  let open = $state<boolean>(false);
  let query = $state('');
  let options: ContextOption[] = $state([]);
  let loading = $state(false);
  let failed = $state(false);

  const name = $derived(context.kind === 'all' ? m.allIssues() : context.name);
  const current = $derived(
    context.kind === 'all' ? 'all' : `${context.kind}:${String(context.id)}`,
  );

  const ALL: ContextOption = {
    key: 'all',
    kind: 'all',
    name: '',
    detail: null,
    context: { kind: 'all' },
  };

  async function load(): Promise<void> {
    loading = true;
    failed = false;
    try {
      const [projects, tags, searches] = await Promise.all([
        findProjects(''),
        api.get('/tags', { query: { scope: 'all', limit: 100 } }),
        api.get('/saved-searches', { query: { scope: 'all', limit: 100 } }),
      ]);
      options = [
        ALL,
        ...projects.items.map((p): ContextOption => ({
          key: `project:${String(p.id)}`,
          kind: 'project',
          name: p.name,
          detail: p.key,
          context: { kind: 'project', key: p.key },
        })),
        ...tags.items.map((t): ContextOption => ({
          key: `tag:${String(t.id)}`,
          kind: 'tag',
          name: t.name,
          detail: null,
          context: { kind: 'tag', id: t.id },
        })),
        ...searches.items.map((s): ContextOption => ({
          key: `search:${String(s.id)}`,
          kind: 'search',
          name: s.name,
          detail: null,
          context: { kind: 'search', id: s.id },
        })),
      ];
    } catch {
      failed = true;
    } finally {
      loading = false;
    }
  }

  $effect(() => {
    if (open) {
      query = '';
      void load();
    }
  });

  const shown = $derived.by(() => {
    const text = query.trim().toLocaleLowerCase();
    if (text === '') return options;
    return options.filter(
      (o) =>
        o.kind !== 'all' &&
        (o.name.toLocaleLowerCase().includes(text) ||
          (o.detail?.toLocaleLowerCase().startsWith(text) ?? false)),
    );
  });

  function kindName(option: ContextOption): string {
    switch (option.kind) {
      case 'all':
        return '';
      case 'project':
        return option.detail ? `${m.kindProject()} · ${option.detail}` : m.kindProject();
      case 'tag':
        return m.kindTag();
      case 'search':
        return m.kindSearch();
    }
  }
</script>

<Popover label={m.context()} bind:open>
  {#snippet trigger(props)}
    <button type="button" class="trigger" aria-label={m.contextOf({ name })} {...props}>
      <span class="name">{name}</span>
      <ChevronDown size={16} aria-hidden="true" />
    </button>
  {/snippet}
  {#snippet children({ close }: { close: () => void })}
    <OptionList
      label={m.context()}
      options={shown}
      key={(o: ContextOption) => o.key}
      checked={(o: ContextOption) => o.key === current}
      onchange={(o: ContextOption, on: boolean) => {
        if (!on) return;
        close();
        if (o.key !== current) onchoose(o.context);
      }}
      bind:query
      searchLabel={m.findContext()}
      {loading}
      problem={failed ? m.contextsFailed() : null}
    >
      {#snippet option(o: ContextOption)}{o.kind === 'all' ? m.allIssues() : o.name}{/snippet}
      {#snippet secondary(o: ContextOption)}{kindName(o)}{/snippet}
    </OptionList>
  {/snippet}
</Popover>

<style>
  .trigger {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
    max-width: 16rem;
    height: var(--control-md);
    padding: 0 var(--space-2) 0 var(--space-3);
    border: 1px solid var(--color-border-control);
    border-radius: var(--radius-md);
    background: var(--color-surface);
    color: var(--color-text);
    font-weight: var(--weight-medium);
    cursor: pointer;
  }

  .trigger:hover {
    background: var(--color-hover);
  }

  .name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
