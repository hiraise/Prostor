// CNT-034, CNT-044: Markdown for uploaded files and pasted addresses, and the names behind file
// markers, decoded by the same rule as the server's renderer (crates/markup).
import { describe, expect, it } from 'vitest';

import {
  encodeFileName,
  fileMarkdown,
  linkOverSelection,
  markerName,
  percentDecode,
} from './files.ts';

describe('files', () => {
  it.each<[string, string]>([
    ['shot.png', 'shot.png'],
    ['build log.txt', 'build%20log.txt'],
    ['a(1)[2].png', 'a%281%29%5B2%5D.png'],
    ['100%.txt', '100%25.txt'],
    ['c:\\x.txt', 'c%3A%5Cx.txt'],
    ['.env', '%2Eenv'],
    ['отчёт.pdf', 'отчёт.pdf'],
    ['tab\there?#.md', 'tab%09here%3F%23.md'],
  ])('CNT-034: the name %j goes into Markdown as %j and decodes back', (name, encoded) => {
    expect(encodeFileName(name)).toBe(encoded);
    expect(percentDecode(encodeFileName(name))).toBe(name);
  });

  it('CNT-034: pictures and videos show, other files are links', () => {
    expect(fileMarkdown({ name: 'my shot.png', kind: 'image' })).toBe('![](my%20shot.png)');
    expect(fileMarkdown({ name: 'clip.mp4', kind: 'video' })).toBe('![](clip.mp4)');
    expect(fileMarkdown({ name: 'spec.pdf', kind: 'pdf' })).toBe('[spec.pdf](spec.pdf)');
    expect(fileMarkdown({ name: 'my_log [1].txt', kind: 'other' })).toBe(
      '[my\\_log \\[1\\].txt](my_log%20%5B1%5D.txt)',
    );
  });

  it('CNT-034: an address pasted over a selection links it', () => {
    expect(linkOverSelection('the docs', 'https://example.com/a ')).toBe(
      '[the docs](https://example.com/a)',
    );
    expect(linkOverSelection('wiki', 'https://x.example/A_(b)')).toBe(
      '[wiki](https://x.example/A_%28b%29)',
    );
    expect(linkOverSelection('', 'https://example.com')).toBeNull();
    expect(linkOverSelection('two\nlines', 'https://example.com')).toBeNull();
    expect(linkOverSelection('text', 'not an address')).toBeNull();
    expect(linkOverSelection('text', 'javascript:alert(1)')).toBeNull();
    expect(linkOverSelection('https://old.example', 'https://new.example')).toBeNull();
  });

  it('CNT-044: marker names decode like the server does', () => {
    expect(markerName('tf-file:build%20log.txt')).toBe('build log.txt');
    expect(markerName('tf-file:my%20file%zz.txt')).toBe('my file%zz.txt');
    expect(markerName('tf-file:%D0%BE%D1%82%D1%87%D1%91%D1%82.pdf')).toBe('отчёт.pdf');
    expect(markerName('tf-file:bad%FF.txt')).toBe('bad\uFFFD.txt');
    expect(markerName('https://example.com')).toBeNull();
  });
});
