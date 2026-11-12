<script lang="ts">
  // The «Интерфейс» section of the account page (UI-010): the person's home page (UI-004) and how
  // the issue page and the issue list look. Each choice is a preference (UI-018): it applies at
  // once and follows the person to their other devices. The home page that the instance defaults
  // to (set in «Настройки», plan step 1.11) is marked in the chooser.
  import { knownBootstrap } from '#lib/api/system.ts';
  import { preferences } from '#lib/session/index.ts';
  import RadioGroup from '#lib/ui/RadioGroup.svelte';

  import * as m from './messages.ts';

  const homeOptions = $derived([
    {
      value: 'issues',
      label: m.homeIssues(),
      hint: knownBootstrap()?.default_home === 'issues' ? m.instanceDefault() : undefined,
    },
    {
      value: 'projects',
      label: m.homeProjects(),
      hint: knownBootstrap()?.default_home === 'projects' ? m.instanceDefault() : undefined,
    },
    {
      value: 'boards',
      label: m.homeBoards(),
      hint: knownBootstrap()?.default_home === 'boards' ? m.instanceDefault() : undefined,
    },
  ]);
</script>

<p class="about">{m.interfaceAbout()}</p>
<div class="choices">
  <RadioGroup
    legend={m.homePage()}
    options={homeOptions}
    bind:value={
      () => preferences.value('home'),
      (home: string) => {
        preferences.set({ home: home === 'projects' || home === 'boards' ? home : 'issues' });
      }
    }
  />
  <RadioGroup
    legend={m.panelSide()}
    options={[
      { value: 'end', label: m.panelEnd() },
      { value: 'start', label: m.panelStart() },
    ]}
    bind:value={
      () => preferences.value('issue_panel_side'),
      (side: string) => {
        preferences.set({ issue_panel_side: side === 'start' ? 'start' : 'end' });
      }
    }
  />
  <RadioGroup
    legend={m.activityOrder()}
    options={[
      { value: 'oldest_first', label: m.oldestFirst() },
      { value: 'newest_first', label: m.newestFirst() },
    ]}
    bind:value={
      () => preferences.value('activity_order'),
      (order: string) => {
        preferences.set({
          activity_order: order === 'newest_first' ? 'newest_first' : 'oldest_first',
        });
      }
    }
  />
  <RadioGroup
    legend={m.longComments()}
    options={[
      { value: 'collapsed', label: m.commentsCollapsed() },
      { value: 'full', label: m.commentsFull() },
    ]}
    bind:value={
      () => preferences.value('long_comments'),
      (shown: string) => {
        preferences.set({ long_comments: shown === 'full' ? 'full' : 'collapsed' });
      }
    }
  />
  <RadioGroup
    legend={m.listSidebar()}
    options={[
      { value: 'open', label: m.sidebarOpen() },
      { value: 'auto', label: m.sidebarAuto() },
      { value: 'closed', label: m.sidebarClosed() },
    ]}
    bind:value={
      () => preferences.value('list_sidebar'),
      (mode: string) => {
        preferences.set({ list_sidebar: mode === 'auto' || mode === 'closed' ? mode : 'open' });
      }
    }
  />
</div>

<style>
  .about {
    color: var(--color-text-muted);
  }

  .choices {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(16rem, 1fr));
    gap: var(--space-6);
  }
</style>
