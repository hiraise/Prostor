// Buttons, fields, notices and page states (UI-032, UI-036).
import Plus from '@lucide/svelte/icons/plus';
import { createRawSnippet, flushSync } from 'svelte';
import { describe, expect, it, vi } from 'vitest';
import { page, userEvent } from 'vitest/browser';
import { render } from 'vitest-browser-svelte';

import { ApiError } from '#lib/api/client.ts';

import { expectAccessible, isPhone, smallTargets } from '../../test/a11y.ts';
import Button from './Button.svelte';
import Checkbox from './Checkbox.svelte';
import EmptyState from './EmptyState.svelte';
import ErrorState from './ErrorState.svelte';
import ForbiddenState from './ForbiddenState.svelte';
import IconButton from './IconButton.svelte';
import NotFoundState from './NotFoundState.svelte';
import Notice from './Notice.svelte';
import TextField from './TextField.svelte';
import { visibleTooltip } from './tooltip.ts';

const text = (value: string) => createRawSnippet(() => ({ render: () => `<span>${value}</span>` }));

describe('Button', () => {
  it('runs its action and has the size of a control', async () => {
    const onclick = vi.fn();
    const screen = await render(Button, {
      variant: 'primary',
      icon: Plus,
      onclick,
      children: text('Create'),
    });

    await screen.getByRole('button', { name: 'Create' }).click();

    expect(onclick).toHaveBeenCalledOnce();
    expect(await smallTargets(screen.container, isPhone() ? 44 : 32)).toEqual([]);
    await expectAccessible(screen.container);
  });

  it('ignores clicks while loading but keeps focus', async () => {
    const onclick = vi.fn();
    const screen = await render(Button, { loading: true, onclick, children: text('Save') });
    const button = screen.getByRole('button', { name: 'Save' });

    // Forced: a busy button reports itself as disabled (aria-disabled), which Playwright honors.
    await button.click({ force: true });

    expect(onclick).not.toHaveBeenCalled();
    await expect.element(button).toHaveAttribute('aria-busy', 'true');
    // Still focusable, unlike a disabled button, so keyboard users do not lose their place.
    button.element().focus();
    await expect.element(button).toHaveFocus();
  });

  it('renders a link that looks like a button', async () => {
    const screen = await render(Button, { href: '/issues', children: text('Open issues') });
    await expect
      .element(screen.getByRole('link', { name: 'Open issues' }))
      .toHaveAttribute('href', '/issues');
  });
});

describe('IconButton', () => {
  it('UI-032: is named by its label and shows it as a tooltip on keyboard focus', async () => {
    const screen = await render(IconButton, { icon: Plus, label: 'Create issue' });
    const button = screen.getByRole('button', { name: 'Create issue' });

    await userEvent.tab();
    await expect.element(button).toHaveFocus();
    await expect.poll(() => visibleTooltip()?.text).toBe('Create issue');

    // Escape hides the tooltip without moving focus (WCAG 1.4.13).
    await userEvent.keyboard('{Escape}');
    expect(visibleTooltip()).toBeNull();
    await expect.element(button).toHaveFocus();
    expect(await smallTargets(screen.container, isPhone() ? 44 : 32)).toEqual([]);
  });

  it('shows the tooltip on hover with a mouse, not on touch', async () => {
    // Park the mouse cursor elsewhere, so only the touch event below can reach the button.
    const parking = document.createElement('div');
    parking.style.cssText = 'position: fixed; right: 0; bottom: 0; width: 20px; height: 20px;';
    document.body.append(parking);
    await userEvent.hover(parking);
    const screen = await render(IconButton, { icon: Plus, label: 'Create issue' });
    const button = screen.getByRole('button').element();

    button.dispatchEvent(new PointerEvent('pointerenter', { pointerType: 'touch' }));
    await new Promise((resolve) => setTimeout(resolve, 700));
    expect(visibleTooltip()).toBeNull();

    if (!isPhone()) {
      await screen.getByRole('button').hover();
      await expect.poll(() => visibleTooltip()?.text, { timeout: 2000 }).toBe('Create issue');
      await userEvent.unhover(screen.getByRole('button'));
    }
    parking.remove();
  });
});

describe('TextField', () => {
  it('ties its label, hint and error to the input', async () => {
    const screen = await render(TextField, {
      label: 'Project key',
      hint: 'Letters and digits',
      error: 'The value is not valid.',
      required: true,
    });
    const input = screen.getByLabelText(/^Project key/);

    await expect.element(input).toHaveAttribute('aria-invalid', 'true');
    await expect
      .element(input)
      .toHaveAccessibleDescription('Letters and digits The value is not valid.');
    await input.fill('CORE');
    await expect.element(input).toHaveValue('CORE');
    await expectAccessible(screen.container);
  });
});

describe('Checkbox', () => {
  it('toggles from its label', async () => {
    const screen = await render(Checkbox, { label: 'Remember me' });
    await screen.getByText('Remember me').click();
    await expect.element(screen.getByRole('checkbox', { name: 'Remember me' })).toBeChecked();
    await expectAccessible(screen.container);
  });
});

describe('Notice', () => {
  it('can be dismissed', async () => {
    const ondismiss = vi.fn();
    const screen = await render(Notice, {
      tone: 'warning',
      ondismiss,
      children: text('Read-only mode'),
    });
    await screen.getByRole('button', { name: 'Dismiss' }).click();
    expect(ondismiss).toHaveBeenCalledOnce();
    await expectAccessible(screen.container);
  });
});

describe('page states', () => {
  it('UI-036: an empty state names what will appear and offers the main action', async () => {
    const screen = await render(EmptyState, {
      title: 'No boards yet',
      description: 'Boards you create or follow appear here.',
      actions: createRawSnippet(() => ({
        render: () => '<button type="button">Create a board</button>',
      })),
    });
    await expect
      .element(screen.getByRole('heading', { level: 2, name: 'No boards yet' }))
      .toBeVisible();
    await expect.element(screen.getByRole('button', { name: 'Create a board' })).toBeVisible();
  });

  it('UI-036: not found does not say whether the thing exists', async () => {
    const screen = await render(NotFoundState, { home: '/issues' });
    await expect
      .element(screen.getByRole('heading', { level: 1 }))
      .toHaveTextContent('Nothing here');
    await expect
      .element(screen.getByText(/does not exist, or you do not have access/))
      .toBeVisible();
    await expect
      .element(screen.getByRole('link', { name: 'Go to the home page' }))
      .toHaveAttribute('href', '/issues');
    await expectAccessible(screen.container);
  });

  it('UI-036: forbidden explains whom to ask', async () => {
    const screen = await render(ForbiddenState);
    await expect
      .element(screen.getByRole('heading', { level: 1 }))
      .toHaveTextContent('Only administrators can open this');
  });

  it('UI-036: a failed load shows the reason, the request ID and Try again', async () => {
    const onretry = vi.fn();
    const error = new ApiError(503, { code: 'unavailable', message: 'starting' }, 'req-42');
    const screen = await render(ErrorState, { error, onretry });

    await expect
      .element(screen.getByText('The server is temporarily unavailable.', { exact: false }))
      .toBeVisible();
    await expect.element(screen.getByText('Request ID: req-42')).toBeVisible();
    await screen.getByRole('button', { name: 'Try again' }).click();
    flushSync();
    expect(onretry).toHaveBeenCalledOnce();
    await expect.element(page.getByTestId('announcer-assertive')).not.toBeInTheDocument();
  });
});
