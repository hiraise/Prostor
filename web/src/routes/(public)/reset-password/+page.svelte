<script lang="ts">
  // A link from a password-reset email (AUTH-008): choose a new password and continue signed in.
  import { goto } from '$app/navigation';

  import type { Session } from '#lib/api/schema.gen.ts';
  import { accountMessages as m, ResetPasswordForm } from '#lib/features/account/index.ts';
  import { adoptSession } from '#lib/session/index.ts';
  import { homeHref } from '#lib/shell/sections.ts';
  import PublicFrame from '#lib/shell/PublicFrame.svelte';
  import { toasts } from '#lib/ui/toast.svelte.ts';

  async function done(session: Session): Promise<void> {
    adoptSession(session);
    toasts.show({ tone: 'success', message: m.passwordReset() });
    // eslint-disable-next-line svelte/no-navigation-without-resolve -- homeHref() resolves it
    await goto(homeHref(), { replaceState: true });
  }
</script>

<svelte:head><title>{m.resetTitle()} · TaskMaster</title></svelte:head>

<PublicFrame title={m.resetTitle()}>
  <ResetPasswordForm ondone={done} />
</PublicFrame>
