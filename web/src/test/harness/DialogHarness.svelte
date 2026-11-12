<script lang="ts">
  // Opens a Dialog from a button, as pages do.
  import Button from '#lib/ui/Button.svelte';
  import Dialog from '#lib/ui/Dialog.svelte';
  import TextField from '#lib/ui/TextField.svelte';

  let { dismissible = true }: { dismissible?: boolean } = $props();
  let open = $state(false);
  let closed = $state(0);
</script>

<Button
  onclick={() => {
    open = true;
  }}>Rename</Button
>
<p data-testid="closed">{closed}</p>
<Dialog
  bind:open
  title="Rename project"
  description="The key stays the same."
  {dismissible}
  onclose={() => {
    closed += 1;
  }}
>
  <TextField label="Name" value="Core" />
  {#snippet footer()}
    <Button
      onclick={() => {
        open = false;
      }}>Cancel</Button
    >
    <Button variant="primary">Save</Button>
  {/snippet}
</Dialog>
