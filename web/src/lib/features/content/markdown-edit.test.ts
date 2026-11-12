// CNT-029: the editor's formatting actions as changes of a text and its selection. In the cases
// below `‹` and `›` mark the selection before and after the action.
import { describe, expect, it } from 'vitest';

import { applyEdit, fenceAt, formatEdit, type FormatAction } from './markdown-edit.ts';

function run(action: FormatAction, marked: string): string {
  const start = marked.indexOf('‹');
  const end = marked.indexOf('›') - 1;
  const text = marked.replace('‹', '').replace('›', '');
  const edit = formatEdit(action, text, { start, end }, { column: 'Col' });
  const result = applyEdit(text, edit);
  const { start: from, end: to } = edit.selection;
  return `${result.slice(0, from)}‹${result.slice(from, to)}›${result.slice(to)}`;
}

describe('formatEdit', () => {
  it.each<[FormatAction, string, string]>([
    ['bold', 'a ‹word› b', 'a **‹word›** b'],
    ['bold', 'a **‹word›** b', 'a ‹word› b'],
    ['bold', 'a ‹**word**› b', 'a ‹word› b'],
    ['bold', 'a ‹word ›b', 'a **‹word›** b'],
    ['bold', 'a ‹›b', 'a **‹›**b'],
    ['italic', '‹word›', '_‹word›_'],
    ['italic', '_‹word›_', '‹word›'],
    ['strikethrough', '‹gone›', '~~‹gone›~~'],
    ['code', 'run ‹ls -l›', 'run `‹ls -l›`'],
    ['code', 'run ‹a`b›', 'run `` ‹a`b› ``'],
    ['code', 'run `‹ls›`', 'run ‹ls›'],
  ])('CNT-029: %s turns %s into %s', (action, before, after) => {
    expect(run(action, before)).toBe(after);
  });

  it.each<[string, string]>([
    ['see ‹the docs›', 'see [the docs](‹›)'],
    ['‹https://example.com›', '[‹›](https://example.com)'],
    ['‹›', '[](‹›)'],
  ])('CNT-029: link turns %s into %s', (before, after) => {
    expect(run('link', before)).toBe(after);
  });

  it.each<[FormatAction, string, string]>([
    ['heading', 'Ti‹›tle', '### Ti‹›tle'],
    ['heading', '## Ti‹›tle', 'Ti‹›tle'],
    ['quote', 'one\n‹two\nthree›', 'one\n‹> two\n> three›'],
    ['quote', '‹> two\n> three›', '‹two\nthree›'],
    ['bullets', '‹a\nb›', '‹- a\n- b›'],
    ['bullets', '‹- a\n- b›', '‹a\nb›'],
    ['bullets', '‹1. a\n2. b›', '‹- a\n- b›'],
    ['numbers', '‹a\n\nb›', '‹1. a\n\n2. b›'],
    ['tasks', '‹a\n  b›', '‹- [ ] a\n  - [ ] b›'],
    ['tasks', '- [x] ‹a›', '‹a›'],
    ['bullets', '‹›', '- ‹›'],
    ['bullets', 'x\n‹a\n›b', 'x\n‹- a›\nb'],
  ])('CNT-029: %s turns %j into %j', (action, before, after) => {
    expect(run(action, before)).toBe(after);
  });

  it('CNT-029: a code block wraps whole lines with a fence longer than the code’s backticks', () => {
    expect(run('codeBlock', 'x\nfn ‹main›()\ny')).toBe('x\n```\n‹fn main()›\n```\ny');
    expect(run('codeBlock', '‹›')).toBe('```\n‹›\n```');
    expect(run('codeBlock', '‹a ``` b›')).toBe('````\n‹a ``` b›\n````');
    // Inline code over several lines becomes a block.
    expect(run('code', '‹a\nb›')).toBe('```\n‹a\nb›\n```');
  });

  it('CNT-029: a table starts a new block with its first header selected', () => {
    expect(run('table', '‹›')).toBe('| ‹Col› | Col |\n| --- | --- |\n|  |  |');
    expect(run('table', 'Text‹›')).toBe('Text\n\n| ‹Col› | Col |\n| --- | --- |\n|  |  |');
    expect(run('table', 'Text\n‹›\nMore')).toBe(
      'Text\n\n| ‹Col› | Col |\n| --- | --- |\n|  |  |\n\nMore',
    );
  });
});

describe('fenceAt', () => {
  it('CNT-045: a language is typed right after an opening fence only', () => {
    expect(fenceAt('```', 3)).toEqual({ from: 3, query: '' });
    expect(fenceAt('text\n```ru', 10)).toEqual({ from: 8, query: 'ru' });
    expect(fenceAt('~~~~py\n', 6)).toEqual({ from: 4, query: 'py' });
    // A closing fence, text after the caret, a fence not at the start of the line.
    expect(fenceAt('```\ncode\n```', 12)).toBeNull();
    expect(fenceAt('```rust x', 7)).toBeNull();
    expect(fenceAt('a ```', 5)).toBeNull();
    expect(fenceAt('``', 2)).toBeNull();
  });
});
