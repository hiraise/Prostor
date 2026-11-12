<script lang="ts">
  // Inviting people by email (ACC-004): a list of addresses; each gets an account at once and a link
  // to choose a password. Without outgoing email nothing can be sent, and the dialog says so.
  import { ApiError } from '#lib/api/client.ts';
  import type { AccountSummary, FieldError } from '#lib/api/schema.gen.ts';
  import * as common from '#lib/i18n/messages/common.ts';
  import { fieldErrorText } from '#lib/i18n/messages/errors.ts';
  import Button from '#lib/ui/Button.svelte';
  import Dialog from '#lib/ui/Dialog.svelte';
  import Notice from '#lib/ui/Notice.svelte';
  import { toasts } from '#lib/ui/toast.svelte.ts';

  import * as m from './messages.ts';
  import { refusalText } from './problems.ts';
  import TextArea from '#lib/ui/TextArea.svelte';
  import { addressesIn, inviteByEmail } from './users.ts';

  interface Props {
    open?: boolean;
    /** Outgoing email works on this server. */
    emailEnabled: boolean;
    oninvited: (accounts: AccountSummary[]) => unknown;
  }

  let { open = $bindable(false), emailEnabled, oninvited }: Props = $props();

  const formId = $props.id();
  let text = $state('');
  let busy = $state(false);
  let error = $state<string | null>(null);
  let problem = $state<string | null>(null);

  $effect(() => {
    if (!open) return;
    text = '';
    error = null;
    problem = null;
  });

  /** One sentence per kind of problem with the addresses. */
  function addressProblems(fields: readonly FieldError[]): string {
    const about = (code: string): string[] =>
      addressesIn(fields.filter((field) => field.code === code).map((field) => field.message));
    const texts: string[] = [];
    if (fields.some((field) => field.code === 'required')) texts.push(m.emailsRequired());
    if (fields.some((field) => field.code === 'too_long')) texts.push(m.emailsTooMany());
    const invalid = about('invalid');
    if (invalid.length > 0) texts.push(m.emailsInvalid({ list: invalid.join(', ') }));
    const taken = about('taken');
    if (taken.length > 0) texts.push(m.emailsTaken({ list: taken.join(', ') }));
    return texts.join(' ') || fieldErrorText.invalid();
  }

  async function submit(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    problem = null;
    error = text.trim() ? null : m.emailsRequired();
    if (error) return;
    busy = true;
    try {
      const invited = await inviteByEmail(text);
      open = false;
      toasts.show({ tone: 'success', message: m.invitationsSent({ count: invited.length }) });
      await oninvited(invited);
    } catch (failure) {
      if (failure instanceof ApiError && failure.code === 'invalid_request') {
        error = addressProblems(
          (failure.body?.fields ?? []).filter((field) => field.path === 'emails'),
        );
      } else {
        problem = refusalText(failure, m.emailOff);
      }
    } finally {
      busy = false;
    }
  }
</script>

<Dialog bind:open title={m.invitePeople()} dismissible={!busy}>
  <form id={formId} class="form" novalidate onsubmit={submit}>
    <p class="intro">{m.inviteIntro()}</p>
    {#if !emailEnabled}<Notice tone="warning">{m.emailOff()}</Notice>{/if}
    {#if problem}<Notice tone="danger">{problem}</Notice>{/if}
    <TextArea
      label={m.emails()}
      bind:value={text}
      hint={m.emailsHint()}
      {error}
      rows={4}
      autocapitalize="none"
      spellcheck="false"
      disabled={!emailEnabled}
      required
    />
  </form>

  {#snippet footer()}
    <Button
      disabled={busy}
      onclick={() => {
        open = false;
      }}
    >
      {common.cancel()}
    </Button>
    <Button type="submit" form={formId} variant="primary" loading={busy} disabled={!emailEnabled}>
      {m.sendInvitations()}
    </Button>
  {/snippet}
</Dialog>

<style>
  .form {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }

  .intro {
    color: var(--color-text-muted);
  }
</style>
