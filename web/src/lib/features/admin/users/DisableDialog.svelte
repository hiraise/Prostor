<script lang="ts">
  // Disabling one or many accounts (ACC-005): the status word shown next to the name and an
  // optional reason. Sign-in, sessions and tokens stop at once; everything they did stays.
  import { ApiError } from '#lib/api/client.ts';
  import type { AccountSummary, DisabledLabel } from '#lib/api/schema.gen.ts';
  import { disabledLabels, personName } from '#lib/features/access/index.ts';
  import * as common from '#lib/i18n/messages/common.ts';
  import { fieldErrorTexts } from '#lib/i18n/messages/errors.ts';
  import Button from '#lib/ui/Button.svelte';
  import Dialog from '#lib/ui/Dialog.svelte';
  import Notice from '#lib/ui/Notice.svelte';
  import Select, { type SelectOption } from '#lib/ui/Select.svelte';
  import TextField from '#lib/ui/TextField.svelte';
  import { toasts } from '#lib/ui/toast.svelte.ts';

  import * as m from './messages.ts';
  import { refusalText } from './problems.ts';
  import { disableAccounts } from './users.ts';

  interface Props {
    open?: boolean;
    accounts: readonly AccountSummary[];
    ondone: (ids: number[]) => unknown;
  }

  let { open = $bindable(false), accounts, ondone }: Props = $props();

  const LABELS: DisabledLabel[] = ['disabled', 'suspended', 'left', 'blocked'];

  const formId = $props.id();
  let label = $state<string>('disabled');
  let reason = $state('');
  let busy = $state(false);
  let errors = $state<Record<string, string>>({});
  let problem = $state<string | null>(null);

  const options = $derived<SelectOption[]>(
    LABELS.map((value) => ({ value, label: disabledLabels[value]() })),
  );
  const title = $derived.by(() => {
    const [first] = accounts;
    return accounts.length === 1 && first
      ? m.disableOneTitle({ name: personName(first) })
      : m.disableManyTitle({ count: accounts.length });
  });

  $effect(() => {
    if (!open) return;
    label = 'disabled';
    reason = '';
    errors = {};
    problem = null;
  });

  function isLabel(value: string): value is DisabledLabel {
    return (LABELS as string[]).includes(value);
  }

  async function submit(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    problem = null;
    errors = {};
    const ids = accounts.map((account) => account.id);
    busy = true;
    try {
      await disableAccounts(ids, isLabel(label) ? label : 'disabled', reason.trim());
      open = false;
      toasts.show({ tone: 'success', message: m.disabledDone({ count: ids.length }) });
      await ondone(ids);
    } catch (error) {
      if (error instanceof ApiError && error.code === 'invalid_request') {
        errors = fieldErrorTexts(error.body?.fields ?? []);
        if (!errors.reason) problem = refusalText(error);
      } else {
        problem = refusalText(error, m.disableConflict);
      }
    } finally {
      busy = false;
    }
  }
</script>

<Dialog bind:open {title} dismissible={!busy}>
  <form id={formId} class="form" novalidate onsubmit={submit}>
    <p>{m.disableText()}</p>
    {#if problem}<Notice tone="danger">{problem}</Notice>{/if}
    <Select label={m.statusLabel()} hint={m.statusLabelHint()} {options} bind:value={label} />
    <TextField
      label={m.reason()}
      hint={m.reasonHint()}
      bind:value={reason}
      error={errors.reason}
      maxlength={500}
      autocomplete="off"
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
    <Button type="submit" form={formId} variant="danger" loading={busy}>
      {m.disableConfirm({ count: accounts.length })}
    </Button>
  {/snippet}
</Dialog>

<style>
  .form {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }
</style>
