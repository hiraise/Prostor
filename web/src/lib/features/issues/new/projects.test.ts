// The project a new issue starts in and the picker's order (ISS-005).
import { describe, expect, it } from 'vitest';

import { defaultProject, orderProjects } from './projects.ts';

const p = (id: number, key: string, name: string, favorite = false) => ({
  id,
  key,
  name,
  favorite,
});
const API = p(1, 'API', 'Api');
const WEB = p(2, 'WEB', 'Web', true);
const OPS = p(3, 'OPS', 'Ops');

describe('ISS-005: the default project', () => {
  it('orders the context project, favorites, then the rest by name', () => {
    expect(orderProjects([OPS, WEB, API]).map((x) => x.key)).toEqual(['WEB', 'API', 'OPS']);
    expect(orderProjects([OPS, WEB, API], 'OPS').map((x) => x.key)).toEqual(['OPS', 'WEB', 'API']);
  });

  it('takes the given project, the only one, the last one, then the first', () => {
    expect(defaultProject([API, WEB, OPS], 'OPS', 1)?.key).toBe('OPS');
    expect(defaultProject([API], 'NOPE', null)?.key).toBe('API');
    expect(defaultProject([API, WEB, OPS], 'NOPE', 3)?.key).toBe('OPS');
    expect(defaultProject([API, WEB, OPS], undefined, 99)?.key).toBe('WEB');
    expect(defaultProject<typeof API>([], undefined, null)).toBeUndefined();
  });
});
