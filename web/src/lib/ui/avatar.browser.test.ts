// Avatars with pictures (ACC-017): the picture when it loads, the initials when it does not.
import { describe, expect, it, vi } from 'vitest';
import { render } from 'vitest-browser-svelte';

import { BROKEN_PICTURE, PICTURE } from '../../test/pictures.ts';
import Avatar from './Avatar.svelte';

// A failing picture waits for a 404, which takes a while when every test file runs at once.
const SLOW = { timeout: 5000 };

describe('Avatar', () => {
  it('ACC-017: shows the initials without a picture', async () => {
    const screen = await render(Avatar, { name: 'Anna Admin', color: 2 });
    await vi.waitFor(() => {
      expect(screen.container.textContent.trim()).toBe('AA');
    });
    expect(screen.container.querySelector('img')).toBeNull();
  });

  it('ACC-017: shows the picture, decorative', async () => {
    const screen = await render(Avatar, { name: 'Anna Admin', color: 2, src: PICTURE });
    const img = screen.container.querySelector('img');
    expect(img?.getAttribute('src')).toBe(PICTURE);
    expect(img?.getAttribute('alt')).toBe('');
    expect(screen.container.textContent.trim()).toBe('');
    await vi.waitFor(() => {
      expect(img?.naturalWidth).toBe(2);
    });
    expect(screen.container.querySelector('.avatar')?.getAttribute('aria-hidden')).toBe('true');
  });

  it('ACC-017: falls back to the initials when the picture fails', async () => {
    const screen = await render(Avatar, { name: 'Anna Admin', src: BROKEN_PICTURE });
    await vi.waitFor(() => {
      expect(screen.container.querySelector('img')).toBeNull();
    }, SLOW);
    expect(screen.container.textContent.trim()).toBe('AA');

    // A new picture is tried again.
    await screen.rerender({ name: 'Anna Admin', src: PICTURE });
    await vi.waitFor(() => {
      expect(screen.container.querySelector('img')?.getAttribute('src')).toBe(PICTURE);
    }, SLOW);
  });
});
