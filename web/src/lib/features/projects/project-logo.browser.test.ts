// Project logos (PRJ-007): the uploaded picture when it loads, the key letters when it does not.
import { describe, expect, it, vi } from 'vitest';
import { render } from 'vitest-browser-svelte';

import { BROKEN_PICTURE, PICTURE } from '../../../test/pictures.ts';
import ProjectLogo from './ProjectLogo.svelte';

// A failing picture waits for a 404, which takes a while when every test file runs at once.
const SLOW = { timeout: 5000 };

describe('ProjectLogo', () => {
  it('PRJ-007: shows the key letters without a picture', async () => {
    const screen = await render(ProjectLogo, { projectKey: 'WEB_APP', color: 3 });
    await vi.waitFor(() => {
      expect(screen.container.textContent.trim()).toBe('WE');
    });
    expect(screen.container.querySelector('img')).toBeNull();
  });

  it('PRJ-007: shows the uploaded picture, decorative', async () => {
    const screen = await render(ProjectLogo, { projectKey: 'WEB', color: 3, src: PICTURE });
    const img = screen.container.querySelector('img');
    expect(img?.getAttribute('alt')).toBe('');
    expect(screen.container.textContent.trim()).toBe('');
    await vi.waitFor(() => {
      expect(img?.naturalWidth).toBe(2);
    });
    expect(screen.container.querySelector('.logo')?.getAttribute('aria-hidden')).toBe('true');
  });

  it('PRJ-007: falls back to the letters when the picture fails', async () => {
    const screen = await render(ProjectLogo, { projectKey: 'WEB', color: 3, src: BROKEN_PICTURE });
    await vi.waitFor(() => {
      expect(screen.container.textContent.trim()).toBe('WE');
    }, SLOW);
    expect(screen.container.querySelector('img')).toBeNull();
  });
});
