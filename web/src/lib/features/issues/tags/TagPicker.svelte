<script lang="ts">
  // «Добавить тег» (ISS-035): a search over the tags the viewer may put on issues — their own and
  // those shared with them for use — as checkboxes; a name nobody has yet offers «Создать тег «X»»
  // (ISS-034), which creates the tag and puts it on the issue. Changes show at once. On phones
  // «Ещё» opens it as a sheet, without a button of its own.
  import Tag from '@lucide/svelte/icons/tag';

  import type { Issue, TagRef } from '#lib/api/schema.gen.ts';
  import { refusalText } from '#lib/features/fields/index.ts';
  import { announce } from '#lib/ui/announce.svelte.ts';
  import Button from '#lib/ui/Button.svelte';
  import IconButton from '#lib/ui/IconButton.svelte';
  import OptionList from '#lib/ui/OptionList.svelte';
  import Popover from '#lib/ui/Popover.svelte';
  import { describeError } from '#lib/ui/toast.svelte.ts';

  import { createTag, findTags, setTag } from '../actions.ts';
  import * as m from '../messages.ts';

  interface Props {
    issue: Issue;
    open?: boolean;
    /** Without its own trigger: «Ещё» opens it. */
    bare?: boolean;
    /** The trigger is an icon button (the issue page's title row). */
    icon?: boolean;
  }

  let { issue, open = $bindable(false), bare = false, icon = false }: Props = $props();

  let query = $state('');
  let found = $state<TagRef[]>([]);
  let loading = $state(false);
  let problem = $state<string | null>(null);
  let creating = $state(false);

  /** The issue's tags first, then the others found. */
  const options = $derived([
    ...issue.tags,
    ...found.filter((tag) => !issue.tags.some((own) => own.id === tag.id)),
  ]);
  const typed = $derived(query.trim());
  const canCreate = $derived(
    typed !== '' &&
      !loading &&
      !options.some((tag) => tag.name.toLocaleLowerCase() === typed.toLocaleLowerCase()),
  );

  $effect(() => {
    if (!open) return;
    const text = query;
    const controller = new AbortController();
    const timer = setTimeout(
      () => {
        loading = true;
        problem = null;
        findTags(text, controller.signal).then(
          (tags) => {
            found = tags.map(({ id, name, color }) => ({ id, name, color }));
            loading = false;
          },
          (error: unknown) => {
            if (controller.signal.aborted) return;
            problem = describeError(error).message;
            loading = false;
          },
        );
      },
      text ? 250 : 0,
    );
    return () => {
      clearTimeout(timer);
      controller.abort();
    };
  });

  function change(tag: TagRef, on: boolean): void {
    problem = null;
    setTag(issue, tag, on).catch((error: unknown) => {
      problem = refusalText(error);
      announce(problem, 'assertive');
    });
  }

  async function create(): Promise<void> {
    creating = true;
    problem = null;
    try {
      const tag = await createTag(typed);
      const ref = { id: tag.id, name: tag.name, color: tag.color };
      found = [ref, ...found];
      query = '';
      await setTag(issue, ref, true);
      announce(m.valueAdded({ name: tag.name }));
    } catch (error) {
      problem = refusalText(error);
    } finally {
      creating = false;
    }
  }
</script>

{#snippet option(tag: TagRef)}{tag.name}{/snippet}
{#snippet action()}
  {#if canCreate}
    <Button size="sm" variant="ghost" loading={creating} onclick={create}
      >{m.createTag({ name: typed })}</Button
    >
  {/if}
{/snippet}

<Popover label={m.addTag()} bind:open>
  {#snippet trigger(props)}
    {#if icon}
      <IconButton icon={Tag} label={m.addTag()} {...props} />
    {:else if !bare}
      <Button icon={Tag} {...props}>{m.addTag()}</Button>
    {/if}
  {/snippet}
  <OptionList
    label={m.tags()}
    mode="multiple"
    {options}
    key={(tag: TagRef) => tag.id}
    checked={(tag: TagRef) => issue.tags.some((own) => own.id === tag.id)}
    onchange={change}
    bind:query
    searchLabel={m.findTag()}
    {loading}
    {problem}
    {option}
    {action}
  />
</Popover>
