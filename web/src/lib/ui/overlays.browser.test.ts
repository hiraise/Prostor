// Dialogs, menus, popovers and option lists: keyboard use, focus management, phone layout (UI-032, D-019).
import { describe, expect, it, vi } from 'vitest';
import { page, userEvent } from 'vitest/browser';
import { render } from 'vitest-browser-svelte';

import { expectAccessible, isPhone, smallTargets } from '../../test/a11y.ts';
import DialogHarness from '../../test/harness/DialogHarness.svelte';
import MenuHarness from '../../test/harness/MenuHarness.svelte';
import PopoverHarness from '../../test/harness/PopoverHarness.svelte';
import { announcement } from './announce.svelte.ts';
import { toasts } from './toast.svelte.ts';

/** A closed menu is hidden, so role queries must include hidden elements to find it. */
const closedMenu = () => page.getByRole('menu', { includeHidden: true });

/** Waits until the opening animation (a sheet sliding up) has finished. */
async function settled(element: Element): Promise<void> {
  await Promise.all(
    element.getAnimations({ subtree: true }).map((animation) => animation.finished),
  );
}

describe('Dialog', () => {
  it('UI-032: moves focus inside, closes on Escape and gives focus back', async () => {
    const screen = await render(DialogHarness);
    const opener = screen.getByRole('button', { name: 'Rename' });

    await opener.click();
    const dialog = page.getByRole('dialog', { name: 'Rename project' });
    await expect.element(dialog).toBeVisible();
    await expect.element(dialog).toHaveAccessibleDescription('The key stays the same.');
    await expect.element(page.getByLabelText('Name')).toHaveFocus();
    await expectAccessible(dialog.element());

    await userEvent.keyboard('{Escape}');
    await expect.element(dialog).not.toBeInTheDocument();
    await expect.element(opener).toHaveFocus();
    await expect.element(screen.getByTestId('closed')).toHaveTextContent('1');
  });

  it('closes from its buttons and from the backdrop', async () => {
    const screen = await render(DialogHarness);
    await screen.getByRole('button', { name: 'Rename' }).click();
    await page.getByRole('button', { name: 'Cancel' }).click();
    await expect.element(page.getByRole('dialog')).not.toBeInTheDocument();

    await screen.getByRole('button', { name: 'Rename' }).click();
    await settled(page.getByRole('dialog').element());
    const box = page.getByRole('dialog').element().getBoundingClientRect();
    // A point above the dialog is the backdrop on every layout.
    await userEvent.click(document.body, { position: { x: 5, y: Math.max(box.top - 10, 2) } });
    await expect.element(page.getByRole('dialog')).not.toBeInTheDocument();
  });

  it('stays open while it may not be dismissed', async () => {
    const screen = await render(DialogHarness, { dismissible: false });
    await screen.getByRole('button', { name: 'Rename' }).click();
    await userEvent.keyboard('{Escape}');
    await expect.element(page.getByRole('dialog')).toBeVisible();
    await expect.element(page.getByRole('button', { name: 'Close' })).not.toBeInTheDocument();
  });

  it('D-019: is a bottom sheet on a phone and a centered window on a desktop', async () => {
    const screen = await render(DialogHarness);
    await screen.getByRole('button', { name: 'Rename' }).click();
    await settled(page.getByRole('dialog').element());
    const box = page.getByRole('dialog').element().getBoundingClientRect();
    if (isPhone()) {
      expect(Math.round(box.bottom)).toBe(window.innerHeight);
      expect(Math.round(box.width)).toBe(window.innerWidth);
      expect(smallTargets(page.getByRole('dialog').element())).toEqual([]);
    } else {
      expect(box.top).toBeGreaterThan(0);
      expect(Math.round(box.left + box.width / 2)).toBe(Math.round(window.innerWidth / 2));
    }
  });
});

describe('Menu', () => {
  it('UI-032: works from the keyboard', async () => {
    const onaction = vi.fn();
    const screen = await render(MenuHarness, { onaction });
    const trigger = screen.getByRole('button', { name: 'Project actions' });

    await userEvent.tab();
    await userEvent.tab();
    await expect.element(trigger).toHaveFocus();
    await expect.element(trigger).toHaveAttribute('aria-haspopup', 'menu');

    await userEvent.keyboard('{ArrowDown}');
    const menu = page.getByRole('menu', { name: 'Project actions' });
    await expect.element(menu).toBeVisible();
    await expect.element(trigger).toHaveAttribute('aria-expanded', 'true');
    await expect.element(page.getByRole('menuitem', { name: 'Copy link' })).toHaveFocus();

    // Disabled items are skipped; arrows wrap.
    await userEvent.keyboard('{ArrowDown}');
    await expect.element(page.getByRole('menuitem', { name: 'Delete' })).toHaveFocus();
    await userEvent.keyboard('{End}');
    await expect.element(page.getByRole('menuitemradio', { name: 'large' })).toHaveFocus();
    await userEvent.keyboard('{ArrowDown}');
    await expect.element(page.getByRole('menuitem', { name: 'Copy link' })).toHaveFocus();

    // Typing jumps to an item.
    await userEvent.keyboard('s');
    await expect.element(page.getByRole('menuitemradio', { name: 'small' })).toHaveFocus();

    await userEvent.keyboard('{Escape}');
    await expect.element(closedMenu()).not.toBeVisible();
    await expect.element(trigger).toHaveFocus();
    await expect.element(trigger).toHaveAttribute('aria-expanded', 'false');
  });

  it('selects an action or a choice and closes', async () => {
    const onaction = vi.fn();
    const screen = await render(MenuHarness, { onaction });
    const trigger = screen.getByRole('button', { name: 'Project actions' });

    await trigger.click();
    await page.getByRole('menuitem', { name: 'Copy link' }).click();
    expect(onaction).toHaveBeenCalledWith('copy');
    await expect.element(closedMenu()).not.toBeVisible();

    await trigger.click();
    await expect
      .element(page.getByRole('menuitemradio', { name: 'medium' }))
      .toHaveAttribute('aria-checked', 'true');
    await page.getByRole('menuitemradio', { name: 'large' }).click();
    await expect.element(screen.getByTestId('size')).toHaveTextContent('large');

    await trigger.click();
    await page.getByRole('menuitem', { name: 'Archive' }).click({ force: true });
    expect(onaction).not.toHaveBeenCalledWith('archive');
  });

  it('a late entry joins at the end without sliding the open menu', async () => {
    // The issue page's clone entries land after the menu has opened (loaded per project); the
    // shown entries must not slide under the pointer — the e2e delete click hit «Clone as draft».
    const onaction = vi.fn();
    const screen = await render(MenuHarness, { onaction, late: false });
    const trigger = screen.getByRole('button', { name: 'Project actions' });

    await trigger.click();
    const del = page.getByRole('menuitem', { name: 'Delete' });
    await expect.element(del).toBeVisible();
    const before = del.element().getBoundingClientRect();

    await screen.rerender({ onaction, late: true });
    await expect.element(page.getByRole('menuitem', { name: 'Clone' })).toBeVisible();
    const after = del.element().getBoundingClientRect();
    expect(after.y).toBe(before.y);

    await del.click();
    expect(onaction).toHaveBeenCalledWith('delete');

    // The next opening shows the natural order.
    await trigger.click();
    const clone = page.getByRole('menuitem', { name: 'Clone' });
    await expect.element(clone).toBeVisible();
    expect(clone.element().getBoundingClientRect().y).toBeLessThan(
      page.getByRole('menuitem', { name: 'Delete' }).element().getBoundingClientRect().y,
    );
  });

  it('closes on a click outside and stays inside the screen', async () => {
    const screen = await render(MenuHarness, { onaction: vi.fn() });
    await screen.getByRole('button', { name: 'Project actions' }).click();
    const menu = page.getByRole('menu');
    await expect.element(menu).toBeVisible();
    const box = menu.element().getBoundingClientRect();
    expect(box.right).toBeLessThanOrEqual(window.innerWidth);
    expect(box.bottom).toBeLessThanOrEqual(window.innerHeight);
    if (isPhone()) expect(smallTargets(menu.element())).toEqual([]);
    await expectAccessible(menu.element());

    await screen.getByRole('button', { name: 'Before' }).click();
    await expect.element(closedMenu()).not.toBeVisible();
  });
});

describe('Toasts and overlays', () => {
  it('close when a menu or a popover opens, so they never cover it (backlog 1.4c)', async () => {
    const menu = await render(MenuHarness, { onaction: vi.fn() });
    const onClose = vi.fn();
    toasts.show({ message: 'Saved', onClose });
    await menu.getByRole('button', { name: 'Project actions' }).click();
    expect(onClose).toHaveBeenCalledWith('dismissed');
    expect(toasts.items).toHaveLength(0);
    await userEvent.keyboard('{Escape}');
    await menu.unmount();

    const popover = await render(PopoverHarness);
    toasts.show({ message: 'Saved again' });
    await popover.getByRole('button', { name: 'Priority: Normal' }).click();
    expect(toasts.items).toHaveLength(0);
  });
});

describe('Popover', () => {
  it('UI-032: moves focus inside, closes on Escape and gives focus back', async () => {
    const screen = await render(PopoverHarness);
    const trigger = screen.getByRole('button', { name: 'Priority: Normal' });

    await trigger.click();
    const popover = page.getByRole('dialog', { name: 'Change Priority' });
    await expect.element(popover).toBeVisible();
    await expect.element(trigger).toHaveAttribute('aria-expanded', 'true');
    await expect.element(page.getByRole('searchbox', { name: 'Search' })).toHaveFocus();
    await expectAccessible(popover.element());

    await userEvent.keyboard('{Escape}');
    await expect.element(page.getByRole('dialog')).not.toBeInTheDocument();
    await expect.element(trigger).toHaveFocus();
    await expect.element(trigger).toHaveAttribute('aria-expanded', 'false');
  });

  it('D-019: is a bottom sheet on a phone and next to its button on a desktop', async () => {
    const screen = await render(PopoverHarness);
    const trigger = screen.getByRole('button', { name: 'Priority: Normal' });
    await trigger.click();
    const popover = page.getByRole('dialog', { name: 'Change Priority' });
    await expect.element(popover).toBeVisible();
    await settled(popover.element());
    const box = popover.element().getBoundingClientRect();
    if (isPhone()) {
      expect(Math.round(box.bottom)).toBe(window.innerHeight);
      expect(smallTargets(popover.element())).toEqual([]);
    } else {
      const anchor = trigger.element().getBoundingClientRect();
      expect(box.top).toBeGreaterThanOrEqual(anchor.bottom);
      expect(Math.round(box.left)).toBe(Math.round(anchor.left));
      // A click outside closes it.
      await screen.getByRole('button', { name: 'Before' }).click();
      await expect.element(page.getByRole('dialog')).not.toBeInTheDocument();
    }
  });
});

describe('OptionList', () => {
  it('UI-032: arrows move between options without choosing; Enter chooses', async () => {
    const screen = await render(PopoverHarness);
    await screen.getByRole('button', { name: 'Priority: Normal' }).click();
    await expect.element(page.getByRole('searchbox', { name: 'Search' })).toHaveFocus();

    await userEvent.keyboard('{ArrowDown}');
    // The second line is part of the option's name.
    await expect
      .element(page.getByRole('radio', { name: 'Critical Drop everything' }))
      .toHaveFocus();
    await userEvent.keyboard('{ArrowDown}');
    await expect.element(page.getByRole('radio', { name: 'Major' })).toHaveFocus();
    await expect.element(page.getByRole('radio', { name: 'Normal' })).toBeChecked();
    await userEvent.keyboard('{End}');
    await expect.element(page.getByRole('radio', { name: 'Minor' })).toHaveFocus();
    await userEvent.keyboard('{Home}{ArrowUp}');
    await expect.element(page.getByRole('searchbox', { name: 'Search' })).toHaveFocus();

    await userEvent.keyboard('{ArrowDown}{ArrowDown}{Enter}');
    await expect.element(screen.getByRole('button', { name: 'Priority: Major' })).toHaveFocus();
  });

  it('announces the number of matches and offers an action for an unknown name', async () => {
    const onadd = vi.fn();
    const screen = await render(PopoverHarness, { mode: 'multiple', onadd });
    await screen.getByRole('button', { name: 'Priority: Normal' }).click();

    await userEvent.keyboard('m');
    await expect.element(page.getByRole('checkbox')).toHaveLength(3);
    await expect.poll(() => announcement('polite')).toMatch(/^3 found/);
    await page.getByRole('checkbox', { name: 'Major' }).click();
    // Several values: the list stays open.
    await expect.element(page.getByRole('checkbox', { name: 'Major' })).toBeChecked();

    await userEvent.fill(page.getByRole('searchbox', { name: 'Search' }), 'mzz');
    await expect.poll(() => announcement('polite')).toMatch(/^Nothing found/);
    await page.getByRole('button', { name: 'Add “mzz”' }).click();
    expect(onadd).toHaveBeenCalledWith('mzz');
  });
});
