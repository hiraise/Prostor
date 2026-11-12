<script lang="ts">
  // A popover holding an option list, as the field panel uses them: one choice or several, with a
  // search over the options and a row for adding the typed name.
  import Button from '#lib/ui/Button.svelte';
  import OptionList from '#lib/ui/OptionList.svelte';
  import Popover from '#lib/ui/Popover.svelte';

  interface Props {
    mode?: 'single' | 'multiple';
    onadd?: (name: string) => void;
  }

  let { mode = 'single', onadd }: Props = $props();

  const ALL = ['Critical', 'Major', 'Normal', 'Minor'];
  let chosen = $state<string[]>(['Normal']);
  let query = $state('');
  const options = $derived(ALL.filter((name) => name.toLowerCase().includes(query.toLowerCase())));
</script>

<button type="button">Before</button>
<Popover label="Change Priority">
  {#snippet trigger(props)}
    <button type="button" {...props}>Priority: {chosen.join(', ')}</button>
  {/snippet}
  {#snippet children({ close }: { close: () => void })}
    <OptionList
      label="Values of Priority"
      {mode}
      {options}
      key={(name: string) => name}
      checked={(name: string) => chosen.includes(name)}
      onchange={(name: string, on: boolean) => {
        if (mode === 'single') {
          chosen = [name];
          close();
        } else {
          chosen = on ? [...chosen, name] : chosen.filter((other) => other !== name);
        }
      }}
      bind:query
    >
      {#snippet option(name)}{name}{/snippet}
      {#snippet secondary(name)}{#if name === 'Critical'}Drop everything{/if}{/snippet}
      {#snippet action()}
        {#if query && options.length === 0}
          <Button size="sm" onclick={() => onadd?.(query)}>Add “{query}”</Button>
        {/if}
      {/snippet}
    </OptionList>
  {/snippet}
</Popover>
