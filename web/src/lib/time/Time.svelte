<!--
  A date or time in the viewer's language and zone (UI-030). Relative phrases follow the shared
  clock; the tooltip always gives the full absolute value.
-->
<script lang="ts">
  import { clock } from './clock.svelte.ts';
  import { getTimeContext } from './context.svelte.ts';
  import {
    describeTime,
    type DateStyle,
    type TimeKind,
    type TimeValue,
    type YearMode,
  } from './format.ts';

  interface Props {
    /** An instant (RFC 3339 string, Date, epoch ms) or a calendar date (`YYYY-MM-DD` or object). */
    value: TimeValue;
    kind?: TimeKind;
    style?: DateStyle;
    year?: YearMode;
  }

  let { value, kind = 'datetime', style = 'short', year = 'auto' }: Props = $props();

  // Only relative phrases follow the clock; for the year of other values the time of rendering is
  // enough.
  const view = $derived(
    describeTime(value, getTimeContext(), {
      kind,
      style,
      year,
      now: kind === 'relative' ? clock.now : Date.now(),
    }),
  );
</script>

{#if view}
  <time datetime={view.datetime} title={view.title}>{view.text}</time>
{/if}
