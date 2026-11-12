<script lang="ts">
  // The activity stream with the comment box, as the issue page puts them together (step 1.7b-3).
  import type { Issue, Upload, UserRef } from '#lib/api/schema.gen.ts';
  import ActivityStream from '#lib/features/issues/activity/ActivityStream.svelte';
  import type { ActivityStream as Stream } from '#lib/features/issues/activity/activity.svelte.ts';
  import CommentBox from '#lib/features/issues/comments/CommentBox.svelte';

  interface Props {
    issue: Issue;
    stream: Stream;
    author: UserRef;
    upload: (file: File) => Promise<Upload>;
  }

  let { issue, stream, author, upload }: Props = $props();
</script>

<ActivityStream {issue} {stream} {upload}>
  {#snippet box()}
    <CommentBox {issue} {author} {upload} onposted={() => stream.posted()} />
  {/snippet}
</ActivityStream>
