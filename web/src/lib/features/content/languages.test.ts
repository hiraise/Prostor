import { describe, expect, it } from 'vitest';

import { LANGUAGE_NAMES, matchLanguages } from './language-names.ts';
import hljs from './languages.ts';

describe('CNT-045: languages of code blocks', () => {
  it('the editor offers exactly the highlighted languages', () => {
    expect(LANGUAGE_NAMES.filter((name) => !hljs.getLanguage(name))).toEqual([]);
    expect([...LANGUAGE_NAMES].sort()).toEqual(
      hljs
        .listLanguages()
        .filter((name) => LANGUAGE_NAMES.includes(name))
        .sort(),
    );
    expect(hljs.listLanguages().length).toBe(LANGUAGE_NAMES.length);
  });

  it('typing narrows the list: starts first, then contains', () => {
    expect(matchLanguages('ru')).toEqual(['ruby', 'rust']);
    expect(matchLanguages('script')).toEqual(['javascript', 'typescript']);
    expect(matchLanguages('')).toHaveLength(8);
    expect(matchLanguages('zz')).toEqual([]);
  });
});
