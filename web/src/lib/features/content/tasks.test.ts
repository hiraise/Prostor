// CNT-041: checklist boxes change exactly their own line of the Markdown.
import { describe, expect, it } from 'vitest';

import { sourceLine, toggleTask } from './tasks.ts';

describe('toggleTask', () => {
  const source =
    '# Plan\n\n- [ ] open\n- [x] done\n  - [ ] nested\n1. [X] numbered\n> - [ ] quoted';

  it('CNT-041: ticks and clears the box on the given line', () => {
    expect(toggleTask(source, 3, true)).toBe(source.replace('- [ ] open', '- [x] open'));
    expect(toggleTask(source, 4, false)).toBe(source.replace('- [x] done', '- [ ] done'));
    expect(toggleTask(source, 5, true)).toBe(source.replace('  - [ ] nested', '  - [x] nested'));
    expect(toggleTask(source, 6, false)).toBe(source.replace('1. [X]', '1. [ ]'));
    expect(toggleTask(source, 7, true)).toBe(source.replace('> - [ ] quoted', '> - [x] quoted'));
  });

  it('CNT-041: keeps the rest of the text, Windows line ends included', () => {
    const windows = '- [ ] a\r\n- [ ] b [ ] c\r\n';
    expect(toggleTask(windows, 2, true)).toBe('- [ ] a\r\n- [x] b [ ] c\r\n');
    expect(toggleTask('* [ ]  two spaces', 1, true)).toBe('* [x]  two spaces');
    expect(toggleTask('+ [x] same', 1, true)).toBe('+ [x] same');
  });

  it('CNT-041: a line without a box gives null', () => {
    expect(toggleTask(source, 1, true)).toBeNull();
    expect(toggleTask(source, 2, true)).toBeNull();
    expect(toggleTask(source, 99, true)).toBeNull();
    expect(toggleTask(source, 0, true)).toBeNull();
    expect(toggleTask('text [ ] here', 1, true)).toBeNull();
    expect(toggleTask('-[ ] glued', 1, true)).toBeNull();
  });
});

describe('sourceLine', () => {
  it('CNT-041: reads the first line of a source position', () => {
    expect(sourceLine('12:3-14:0')).toBe(12);
    expect(sourceLine('1:1-1:10')).toBe(1);
    expect(sourceLine(null)).toBeNull();
    expect(sourceLine('x')).toBeNull();
  });
});
