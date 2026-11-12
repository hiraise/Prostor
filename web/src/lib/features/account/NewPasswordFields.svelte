<script lang="ts">
  // A new password typed twice, with the hint about choosing one.
  import TextField from '#lib/ui/TextField.svelte';

  import * as m from './messages.ts';

  interface Props {
    password?: string;
    repeat?: string;
    errors: Record<string, string>;
    /** Path of the password in the server's field errors. */
    path?: string;
    /** The username, for password managers. */
    username?: string;
  }

  let {
    password = $bindable(''),
    repeat = $bindable(''),
    errors,
    path = 'password',
    username,
  }: Props = $props();
</script>

{#if username}
  <!-- Lets password managers tie the new password to the account. -->
  <input
    class="hidden"
    type="text"
    autocomplete="username"
    value={username}
    readonly
    tabindex="-1"
  />
{/if}
<TextField
  label={m.newPassword()}
  type="password"
  bind:value={password}
  hint={m.passwordHint()}
  error={errors[path]}
  autocomplete="new-password"
  required
/>
<TextField
  label={m.repeatNewPassword()}
  type="password"
  bind:value={repeat}
  error={errors.repeat}
  autocomplete="new-password"
  required
/>

<style>
  .hidden {
    display: none;
  }
</style>
