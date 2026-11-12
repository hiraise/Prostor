<script lang="ts">
  // A link from an invitation email (ACC-004): join with one's own password and continue signed in.
  import { goto } from '$app/navigation';

  import type { Session } from '#lib/api/schema.gen.ts';
  import { accountMessages as m, InvitationForm } from '#lib/features/account/index.ts';
  import { adoptSession } from '#lib/session/index.ts';
  import { homeHref } from '#lib/shell/sections.ts';
  import PublicFrame from '#lib/shell/PublicFrame.svelte';
  import { toasts } from '#lib/ui/toast.svelte.ts';

  async function done(session: Session): Promise<void> {
    adoptSession(session);
    toasts.show({ tone: 'success', message: m.joined() });
    // eslint-disable-next-line svelte/no-navigation-without-resolve -- homeHref() resolves it
    await goto(homeHref(), { replaceState: true });
  }
</script>

<svelte:head><title>{m.invitationTitle()} · trackflow</title></svelte:head>

<PublicFrame title={m.invitationTitle()}>
  <InvitationForm ondone={done} />
</PublicFrame>
