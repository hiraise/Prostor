<script lang="ts">
  import { brand } from '#lib/i18n/messages/brand.ts';
  // Creating a project (PRJ-017…PRJ-019): a template, the name, the key (suggested from the name
  // until the person types their own), the number of the first issue and a description. The
  // server checks the name and the key while they are typed (PRJ-018), so problems show before
  // anything is created. One page, one column on phones with the main button at full width.
  import { resolve } from '$app/paths';

  import { ApiError } from '#lib/api/client.ts';
  import type { NewProject, Project, TemplateKey } from '#lib/api/schema.gen.ts';
  import * as common from '#lib/i18n/messages/common.ts';
  import { errorText, fieldErrorText } from '#lib/i18n/messages/errors.ts';
  import Button from '#lib/ui/Button.svelte';
  import ForbiddenState from '#lib/ui/ForbiddenState.svelte';
  import Notice from '#lib/ui/Notice.svelte';
  import PageHeader from '#lib/ui/PageHeader.svelte';
  import TextField from '#lib/ui/TextField.svelte';
  import { describeError, toasts } from '#lib/ui/toast.svelte.ts';

  import * as shared from '../messages.ts';
  import { firstNumberRange, parseFirstNumber, projectFieldTexts } from '../problems.ts';
  import { canCreateProjects, checkProject, createProject } from '../projects.ts';
  import TextArea from '#lib/ui/TextArea.svelte';
  import * as m from './messages.ts';
  import TemplateChoice from './TemplateChoice.svelte';

  interface Props {
    /** The project exists; the route opens its overview. */
    oncreated: (project: Project) => unknown;
    /** Where "forbidden" offers to go. */
    home?: string;
  }

  let { oncreated, home }: Props = $props();

  /** Typing pauses this long before the name and key are checked. */
  const CHECK_MS = 300;
  /** Fields the form shows problems under; others are reported above it. */
  const FIELDS = ['name', 'key', 'first_number', 'description'];

  let template = $state<TemplateKey>('blank');
  let name = $state('');
  let key = $state('');
  /** The person typed a key of their own: stop suggesting one. */
  let ownKey = $state(false);
  let firstNumber = $state('1');
  let description = $state('');
  /** Problems the check found while typing. */
  let live = $state<Record<string, string>>({});
  /** Problems of the last submission. */
  let errors = $state<Record<string, string>>({});
  let problem = $state<string | null>(null);
  let busy = $state(false);
  let form: HTMLFormElement | undefined = $state();

  const shownKey = $derived(key.trim().toUpperCase());
  const example = $derived(shownKey || 'WEB');
  const nameError = $derived(errors.name ?? live.name);
  const keyError = $derived(errors.key ?? (ownKey ? live.key : undefined));

  // Check the name and the key a moment after typing stops; suggest a key for the name until
  // the person types their own.
  $effect(() => {
    const typedName = name.trim();
    const typedKey = ownKey ? key.trim() : '';
    if (!typedName && !typedKey) {
      live = {};
      if (!ownKey) key = '';
      return;
    }
    const controller = new AbortController();
    const timer = setTimeout(() => {
      checkProject(
        { name: typedName || undefined, key: typedKey || undefined },
        controller.signal,
      ).then(
        (result) => {
          live = projectFieldTexts(result.fields);
          if (!ownKey && result.suggested_key) key = result.suggested_key;
        },
        // A failed check changes nothing: submitting checks again.
        () => undefined,
      );
    }, CHECK_MS);
    return () => {
      clearTimeout(timer);
      controller.abort();
    };
  });

  function forget(field: string): void {
    if (!(field in errors)) return;
    errors = Object.fromEntries(Object.entries(errors).filter(([path]) => path !== field));
  }

  function focusFirstError(): void {
    form?.querySelector<HTMLElement>('[aria-invalid="true"]')?.focus();
  }

  async function submit(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    problem = null;
    const found: Record<string, string> = {};
    if (!name.trim()) found.name = fieldErrorText.required();
    const first = parseFirstNumber(firstNumber);
    if (first === null) found.first_number = firstNumberRange();
    errors = found;
    if (Object.keys(found).length > 0) {
      queueMicrotask(focusFirstError);
      return;
    }
    const input: NewProject = {
      template,
      name: name.trim(),
      first_number: first,
      description: description.trim(),
    };
    // Without a key of their own the server picks the free key it suggested.
    if (ownKey && shownKey) input.key = shownKey;
    busy = true;
    try {
      const project = await createProject(input);
      toasts.show({ tone: 'success', message: m.projectCreated({ name: project.name }) });
      await oncreated(project);
    } catch (error) {
      if (error instanceof ApiError && error.code === 'invalid_request') {
        const fields = error.body?.fields ?? [];
        errors = projectFieldTexts(fields);
        if (fields.some((field) => !FIELDS.includes(field.path))) {
          problem = errorText.invalid_request();
        }
        queueMicrotask(focusFirstError);
      } else {
        problem = describeError(error).message;
      }
    } finally {
      busy = false;
    }
  }
</script>

<svelte:head><title>{m.newProject()} · {brand()}</title></svelte:head>

{#if !canCreateProjects()}
  <ForbiddenState {home} />
{:else}
  <PageHeader title={m.newProject()} />

  <form class="form" novalidate onsubmit={submit} bind:this={form}>
    {#if problem}<Notice tone="danger">{problem}</Notice>{/if}

    <TemplateChoice bind:value={template} />

    <TextField
      label={shared.nameLabel()}
      bind:value={name}
      error={nameError}
      maxlength={100}
      autocomplete="off"
      required
      oninput={() => {
        forget('name');
      }}
    />
    <div class="pair">
      <TextField
        class="key"
        label={shared.keyLabel()}
        bind:value={key}
        hint={`${m.keyHint({ example: `${example}-1` })}${!ownKey && shownKey ? ` ${m.keySuggested()}` : ''}`}
        error={keyError}
        maxlength={10}
        autocomplete="off"
        autocapitalize="characters"
        spellcheck="false"
        oninput={(event: Event & { currentTarget: HTMLInputElement }) => {
          ownKey = event.currentTarget.value.trim() !== '';
          forget('key');
        }}
      />
      <TextField
        label={m.firstNumber()}
        bind:value={firstNumber}
        hint={m.firstNumberHint({
          example: `${example}-${String(parseFirstNumber(firstNumber) ?? 1)}`,
        })}
        error={errors.first_number}
        inputmode="numeric"
        maxlength={10}
        autocomplete="off"
        oninput={() => {
          forget('first_number');
        }}
      />
    </div>
    <TextArea
      label={shared.descriptionLabel()}
      bind:value={description}
      hint={shared.descriptionHint()}
      error={errors.description}
      maxlength={10000}
      rows={5}
      oninput={() => {
        forget('description');
      }}
    />

    <div class="actions">
      <Button type="submit" variant="primary" loading={busy}>{m.createProject()}</Button>
      <Button href={resolve('/(app)/projects')} variant="ghost">{common.cancel()}</Button>
    </div>
  </form>
{/if}

<style>
  .form {
    display: flex;
    flex-direction: column;
    gap: var(--space-5);
    max-width: 48rem;
    padding: var(--space-2) var(--space-6) var(--space-12);
  }

  .pair {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(14rem, 1fr));
    gap: var(--space-5) var(--space-4);
    align-items: start;
  }

  .form :global(.key input) {
    font-family: var(--font-mono);
    text-transform: uppercase;
  }

  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
    padding-top: var(--space-1);
  }

  @media (width < 48rem) {
    .form {
      padding: var(--space-2) var(--space-4) calc(var(--space-8) + var(--safe-bottom));
    }

    .actions {
      flex-direction: column;
    }

    .actions :global(.button) {
      width: 100%;
    }
  }
</style>
