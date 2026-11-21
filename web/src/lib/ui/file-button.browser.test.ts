// Choosing files (CNT-019): the button opens the picker and reports what was chosen.
import { createRawSnippet } from 'svelte';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { userEvent } from 'vitest/browser';
import { render } from 'vitest-browser-svelte';

import { expectAccessible, isPhone, smallTargets } from '../../test/a11y.ts';
import FileButton from './FileButton.svelte';

const text = (value: string) => createRawSnippet(() => ({ render: () => `<span>${value}</span>` }));

afterEach(() => {
  vi.restoreAllMocks();
});

function fileInput(container: Element): HTMLInputElement {
  const input = container.querySelector('input[type="file"]');
  if (!(input instanceof HTMLInputElement)) throw new Error('No file input');
  return input;
}

describe('FileButton', () => {
  it('opens the file picker and reports the chosen files', async () => {
    // The real picker would wait for a person; the test chooses files itself.
    const opened = vi
      .spyOn(HTMLInputElement.prototype, 'click')
      .mockImplementation(() => undefined);
    const onfiles = vi.fn();
    const screen = await render(FileButton, {
      accept: 'image/*',
      multiple: true,
      onfiles,
      children: text('Add files'),
    });

    await screen.getByRole('button', { name: 'Add files' }).click();
    expect(opened).toHaveBeenCalledOnce();

    const input = fileInput(screen.container);
    expect(input.accept).toBe('image/*');
    expect(input.multiple).toBe(true);
    const a = new File(['a'], 'a.png', { type: 'image/png' });
    const b = new File(['b'], 'b.png', { type: 'image/png' });
    await userEvent.upload(input, [a, b]);

    expect(onfiles).toHaveBeenCalledOnce();
    expect(onfiles.mock.calls[0]?.[0]).toEqual([a, b]);
    // Emptied, so choosing the same file again is reported again.
    expect(input.value).toBe('');
    expect(await smallTargets(screen.container, isPhone() ? 44 : 32)).toEqual([]);
    await expectAccessible(screen.container);
  });

  it('does not open the picker while disabled', async () => {
    const opened = vi
      .spyOn(HTMLInputElement.prototype, 'click')
      .mockImplementation(() => undefined);
    const screen = await render(FileButton, {
      disabled: true,
      onfiles: vi.fn(),
      children: text('Add files'),
    });
    await expect.element(screen.getByRole('button', { name: 'Add files' })).toBeDisabled();
    expect(opened).not.toHaveBeenCalled();
  });
});
