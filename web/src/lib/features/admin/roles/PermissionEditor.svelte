<script lang="ts">
  // A role's permissions (ACC-034, ACC-044, ACC-046, ACC-050, ACC-054): the catalog grouped by
  // area or by action, filtered by name. A toggle shows at once with what it implies or what
  // depends on it, saves immediately, and the role the server returns replaces the stored one.
  // Without the right to change the role every checkbox is disabled.
  import { SvelteMap } from 'svelte/reactivity';

  import type { PermissionInfo, PermissionKey, Role } from '#lib/api/schema.gen.ts';
  import { permissions } from '#lib/features/access/index.ts';
  import * as common from '#lib/i18n/messages/common.ts';
  import { refreshAccess } from '#lib/session/index.ts';
  import { announce } from '#lib/ui/announce.svelte.ts';
  import RadioGroup, { type RadioOption } from '#lib/ui/RadioGroup.svelte';
  import TextField from '#lib/ui/TextField.svelte';
  import { describeError, showError, toasts } from '#lib/ui/toast.svelte.ts';

  import * as m from './messages.ts';
  import PermissionRow from './PermissionRow.svelte';
  import {
    groupPermissions,
    missingPermissions,
    permissionName,
    setRolePermission,
    toggled,
    type Grouping,
  } from './roles.ts';

  interface Props {
    role: Role;
    catalog: readonly PermissionInfo[];
    editable: boolean;
  }

  let { role, catalog, editable }: Props = $props();

  const id = $props.id();
  let filter = $state('');
  let grouping = $state<Grouping>('entity');
  /** Toggles being saved, in the order they were made: permission → switched on. */
  const pending = new SvelteMap<PermissionKey, boolean>();

  const groupings = $derived<RadioOption[]>([
    { value: 'entity', label: m.byEntity() },
    { value: 'operation', label: m.byOperation() },
  ]);
  const byKey = $derived(new Map(catalog.map((info) => [info.key, info])));
  // What the role holds as it will be once the toggles in flight are saved.
  const effective = $derived.by(() => {
    let set = new Set(role.permissions);
    for (const [key, on] of pending) {
      const info = byKey.get(key);
      if (info) set = toggled(set, info, on);
    }
    return set;
  });
  const needle = $derived(filter.trim().toLocaleLowerCase());
  const shown = $derived(
    needle
      ? catalog.filter(
          (info) =>
            permissions[info.key].name().toLocaleLowerCase().includes(needle) ||
            info.key.includes(needle),
        )
      : catalog,
  );
  const groups = $derived(groupPermissions(shown, grouping));
  const hasGlobalOnly = $derived(shown.some((info) => info.global_only));

  function names(keys: Iterable<PermissionKey>): string {
    return [...keys].map((key) => permissions[key].name()).join(', ');
  }

  async function toggle(info: PermissionInfo, on: boolean): Promise<void> {
    const before = new Set(role.permissions);
    pending.set(info.key, on);
    try {
      const saved = await setRolePermission(role.id, info.key, on);
      const after = new Set(saved.permissions);
      const added = [...after].filter((key) => !before.has(key));
      const removed = [...before].filter((key) => !after.has(key));
      if (added.length > 0) announce(m.switchedOn({ names: names(added) }));
      if (removed.length > 0) announce(m.switchedOff({ names: names(removed) }));
      // The signed-in person may hold this role.
      if (saved.assignments > 0) void refreshAccess().catch(() => undefined);
    } catch (error) {
      const missing = missingPermissions(error);
      if (missing.length > 0) {
        const { requestId } = describeError(error);
        toasts.show({
          tone: 'error',
          message: m.missingPermissions({ names: missing.map(permissionName).join(', ') }),
          detail: requestId ? common.requestId({ id: requestId }) : undefined,
        });
      } else {
        showError(error, () => toggle(info, on));
      }
    } finally {
      pending.delete(info.key);
    }
  }
</script>

<section class="editor" aria-labelledby="{id}-title">
  <div class="head">
    <h2 id="{id}-title">{m.permissionsTitle()}</h2>
    <p class="count">{m.switchedOnCount({ on: effective.size, total: catalog.length })}</p>
  </div>

  <div class="controls">
    <TextField
      class="find"
      type="search"
      label={m.findPermission()}
      bind:value={filter}
      autocomplete="off"
    />
    <RadioGroup legend={m.grouping()} options={groupings} bind:value={grouping} />
  </div>

  {#if hasGlobalOnly}<p class="legend">{m.globalOnlyExplained()}</p>{/if}

  {#if groups.length === 0}
    <p class="none">{m.noPermissionsMatch({ text: filter.trim() })}</p>
  {:else}
    <div class="groups">
      {#each groups as group (group.key)}
        <section class="group" aria-labelledby="{id}-{group.key}">
          <h3 id="{id}-{group.key}">{group.title}</h3>
          <ul>
            {#each group.items as info (info.key)}
              <li>
                <PermissionRow
                  permission={info}
                  checked={effective.has(info.key)}
                  busy={pending.has(info.key)}
                  disabled={!editable}
                  ontoggle={(on: boolean) => toggle(info, on)}
                />
              </li>
            {/each}
          </ul>
        </section>
      {/each}
    </div>
  {/if}
</section>

<style>
  .editor {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }

  .head {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: var(--space-1) var(--space-3);
  }

  h2 {
    font-size: var(--text-lg);
  }

  .count {
    color: var(--color-text-muted);
    font-size: var(--text-sm);
    font-variant-numeric: tabular-nums;
  }

  .controls {
    display: flex;
    flex-wrap: wrap;
    align-items: flex-start;
    gap: var(--space-3) var(--space-8);
  }

  .controls :global(.find) {
    flex: 1 1 16rem;
    max-width: 24rem;
  }

  .legend,
  .none {
    max-width: 44rem;
    color: var(--color-text-muted);
    font-size: var(--text-sm);
  }

  .groups {
    display: flex;
    flex-direction: column;
    gap: var(--space-5);
  }

  .group {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }

  h3 {
    color: var(--color-text-muted);
    font-size: var(--text-sm);
    font-weight: var(--weight-semibold);
  }

  ul {
    margin: 0;
    padding: 0;
    overflow: hidden;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-lg);
    background: var(--color-surface);
    list-style: none;
  }

  li + li {
    border-top: 1px solid var(--color-border);
  }

  @media (width < 48rem) {
    .controls :global(.find) {
      max-width: none;
    }
  }
</style>
