<script lang="ts">
  // Creating a personal API token (AUTH-029, AUTH-030): a name and the access to each area, then
  // the secret, shown once with a copy button. Problems are reported inside the dialog.
  import Copy from '@lucide/svelte/icons/copy';

  import { ApiError } from '#lib/api/client.ts';
  import type { CreatedApiToken } from '#lib/api/schema.gen.ts';
  import * as common from '#lib/i18n/messages/common.ts';
  import { fieldErrorText, fieldErrorTexts } from '#lib/i18n/messages/errors.ts';
  import Button from '#lib/ui/Button.svelte';
  import { copyText } from '#lib/ui/copy.ts';
  import Dialog from '#lib/ui/Dialog.svelte';
  import Notice from '#lib/ui/Notice.svelte';
  import RadioGroup, { type RadioOption } from '#lib/ui/RadioGroup.svelte';
  import TextField from '#lib/ui/TextField.svelte';
  import { describeError } from '#lib/ui/toast.svelte.ts';

  import * as m from './messages.ts';
  import { createToken, scopesFor, type Access } from './tokens.ts';

  interface Props {
    open?: boolean;
    oncreated: (created: CreatedApiToken) => unknown;
  }

  let { open = $bindable(false), oncreated }: Props = $props();

  const formId = $props.id();
  let name = $state('');
  let tracker = $state<Access>('read');
  let admin = $state<Access>('none');
  let busy = $state(false);
  let problem = $state<string | null>(null);
  let errors = $state<Record<string, string>>({});
  let created = $state<CreatedApiToken | null>(null);

  const levels = $derived<RadioOption[]>([
    { value: 'none', label: m.accessNone() },
    { value: 'read', label: m.accessRead() },
    { value: 'write', label: m.accessWrite() },
  ]);

  // Every opening starts from a clean form.
  $effect(() => {
    if (!open) return;
    name = '';
    tracker = 'read';
    admin = 'none';
    problem = null;
    errors = {};
    created = null;
  });

  async function submit(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    problem = null;
    errors = {};
    if (!name.trim()) errors.name = fieldErrorText.required();
    const scopes = scopesFor(tracker, admin);
    if (scopes.length === 0) problem = m.chooseAccess();
    if (problem || Object.keys(errors).length > 0) return;
    busy = true;
    try {
      created = await createToken(name.trim(), scopes);
      await oncreated(created);
    } catch (error) {
      if (error instanceof ApiError && error.code === 'invalid_request') {
        errors = fieldErrorTexts(error.body?.fields ?? []);
        if (errors.scopes) problem = m.chooseAccess();
      } else {
        problem = describeError(error).message;
      }
    } finally {
      busy = false;
    }
  }
</script>

<Dialog bind:open title={created ? m.tokenCreatedTitle() : m.newTokenTitle()}>
  {#if created}
    <div class="created">
      <Notice tone="warning">{m.copyTokenNow()}</Notice>
      <TextField
        label={m.tokenValue()}
        value={created.secret}
        readonly
        class="secret"
        onfocus={(event: FocusEvent & { currentTarget: HTMLInputElement }) => {
          event.currentTarget.select();
        }}
      />
      <p class="usage">{m.tokenUsage()}</p>
    </div>
  {:else}
    <form id={formId} class="form" novalidate onsubmit={submit}>
      {#if problem}<Notice tone="danger">{problem}</Notice>{/if}
      <TextField
        label={m.tokenName()}
        bind:value={name}
        hint={m.tokenNameHint()}
        error={errors.name}
        maxlength={100}
        autocomplete="off"
        required
      />
      <div class="areas">
        <RadioGroup
          legend={m.trackerArea()}
          hint={m.trackerAreaHint()}
          options={levels}
          bind:value={tracker}
        />
        <RadioGroup
          legend={m.adminArea()}
          hint={m.adminAreaHint()}
          options={levels}
          bind:value={admin}
        />
      </div>
    </form>
  {/if}

  {#snippet footer()}
    {#if created}
      <Button
        icon={Copy}
        onclick={() => {
          if (created) void copyText(created.secret, m.tokenCopied());
        }}
      >
        {m.copyToken()}
      </Button>
      <Button
        variant="primary"
        onclick={() => {
          open = false;
        }}
      >
        {m.done()}
      </Button>
    {:else}
      <Button
        onclick={() => {
          open = false;
        }}
      >
        {common.cancel()}
      </Button>
      <Button type="submit" form={formId} variant="primary" loading={busy}>
        {m.createToken()}
      </Button>
    {/if}
  {/snippet}
</Dialog>

<style>
  .form,
  .created {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }

  .areas {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(12rem, 1fr));
    gap: var(--space-4);
  }

  .usage {
    color: var(--color-text-muted);
    font-size: var(--text-sm);
  }

  .created :global(.secret input) {
    font-family: var(--font-mono);
    font-size: var(--text-sm);
  }
</style>
