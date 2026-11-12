<script lang="ts">
  // «Изменить» of a saved search (QRY-064): its name and query. The owner and the edit audience
  // change them; sharing has its own dialog. A query with unknown names is refused with spans.
  import type { SavedSearch } from '#lib/api/schema.gen.ts';
  import { focusFirstError, refusal } from '#lib/features/fields/index.ts';
  import { SourceSubscription } from '#lib/features/notifications/index.ts';
  import Button from '#lib/ui/Button.svelte';
  import Dialog from '#lib/ui/Dialog.svelte';
  import Notice from '#lib/ui/Notice.svelte';
  import TextField from '#lib/ui/TextField.svelte';
  import { toasts } from '#lib/ui/toast.svelte.ts';

  import { queryProblems } from '../list/search.svelte.ts';
  import * as listMessages from '../list/messages.ts';
  import * as m from './messages.ts';
  import { updateSearch } from './sidebar.ts';

  interface Props {
    open?: boolean;
    search: SavedSearch | null;
    onsaved: () => unknown;
  }

  let { open = $bindable(false), search, onsaved }: Props = $props();

  const id = $props.id();
  let form: HTMLFormElement | undefined = $state();
  let name = $state('');
  let query = $state('');
  let errors = $state<Record<string, string>>({});
  let problem = $state<string | null>(null);
  let busy = $state(false);

  $effect(() => {
    if (!open || !search) return;
    name = search.name;
    query = search.query;
    errors = {};
    problem = null;
  });

  async function submit(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    if (!search) return;
    errors = {};
    problem = null;
    const next: Record<string, string> = {};
    if (name.trim() === '') next.name = m.nameRequired();
    if (query.trim() === '') next.query = m.queryRequired();
    if (Object.keys(next).length > 0) {
      errors = next;
      focusFirstError(form);
      return;
    }
    const changes: { name?: string; query?: string } = {};
    if (name.trim() !== search.name) changes.name = name.trim();
    if (query.trim() !== search.query) changes.query = query.trim();
    busy = true;
    try {
      if (Object.keys(changes).length > 0) {
        await updateSearch(search, changes);
        toasts.show({ tone: 'success', message: m.searchSaved() });
      }
      open = false;
      await onsaved();
    } catch (error) {
      const problems = queryProblems(error);
      if (problems !== null) {
        const typed = query.trim();
        errors = {
          query: problems
            .map((p) => {
              const text = typed.slice(p.from, p.to).trim();
              const meaning = listMessages.problems[p.code]();
              return text === '' ? meaning : listMessages.problemAt({ text, problem: meaning });
            })
            .join(' '),
        };
      } else {
        const found = refusal(error, {
          own: { name: { taken: m.nameTaken } },
          conflict: m.conflict,
          paths: ['name', 'query'],
        });
        errors = found.fields;
        problem = found.message;
      }
      focusFirstError(form);
    } finally {
      busy = false;
    }
  }
</script>

<Dialog bind:open title={m.editSearch()} dismissible={!busy}>
  <form id="{id}-form" class="form" bind:this={form} onsubmit={(event) => void submit(event)}>
    {#if problem}<Notice tone="danger">{problem}</Notice>{/if}
    <TextField label={m.name()} bind:value={name} error={errors.name ?? null} maxlength={100} />
    <TextField label={m.query()} bind:value={query} error={errors.query ?? null} />
    <SourceSubscription open={open && search !== null} source="search" sourceId={search?.id ?? 0} />
  </form>
  {#snippet footer()}
    <Button
      variant="ghost"
      disabled={busy}
      onclick={() => {
        open = false;
      }}
    >
      {m.cancel()}
    </Button>
    <Button variant="primary" type="submit" form="{id}-form" loading={busy}>{m.save()}</Button>
  {/snippet}
</Dialog>

<style>
  .form {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }
</style>
