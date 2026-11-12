<script lang="ts">
  // A person: avatar and name, optionally the username. People the viewer may not see appear
  // under a pseudonym (ACC-016), deleted accounts under a placeholder (ACC-006). With `href` the
  // name links to the profile.
  import Avatar from '#lib/ui/Avatar.svelte';

  import { personName, personUsername, type PersonLike } from './people.ts';

  interface Props {
    person: PersonLike;
    size?: 'sm' | 'md' | 'lg';
    withUsername?: boolean;
    href?: string;
  }

  let { person, size = 'sm', withUsername = false, href }: Props = $props();

  const name = $derived(personName(person));
  const username = $derived(withUsername ? personUsername(person) : null);
</script>

<span class={['person', size]}>
  <Avatar
    {name}
    {size}
    color={person.kind === 'deleted' ? null : person.avatar_color}
    src={person.avatar_url ?? null}
  />
  <span class="text">
    {#if href}
      <!-- Callers pass a resolved path. -->
      <!-- eslint-disable-next-line svelte/no-navigation-without-resolve -->
      <a class="name" {href}>{name}</a>
    {:else}
      <span class="name">{name}</span>
    {/if}
    {#if username}<span class="username">{username}</span>{/if}
  </span>
</span>

<style>
  .person {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    min-width: 0;
  }

  .lg {
    gap: var(--space-3);
  }

  .text {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }

  .name {
    overflow-wrap: anywhere;
  }

  /* As tall as a control, so it is easy to hit with a mouse and, on touch screens, a finger. */
  a.name {
    display: inline-flex;
    align-items: center;
    min-height: var(--control-md);
    color: inherit;
    font-weight: var(--weight-medium);
    text-decoration: none;
  }

  a.name:hover {
    color: var(--color-link);
    text-decoration: underline;
  }

  .username {
    color: var(--color-text-muted);
    font-family: var(--font-mono);
    font-size: var(--text-xs);
    overflow-wrap: anywhere;
  }
</style>
