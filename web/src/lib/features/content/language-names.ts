// The names of the languages code blocks highlight (CNT-045), offered by the editor after an
// opening fence. Kept apart from `languages.ts`, which loads highlight.js only when a code block
// appears; `languages.test.ts` checks that both name the same languages.
export const LANGUAGE_NAMES: readonly string[] = [
  'bash',
  'c',
  'cpp',
  'csharp',
  'css',
  'diff',
  'dockerfile',
  'go',
  'graphql',
  'ini',
  'java',
  'javascript',
  'json',
  'kotlin',
  'lua',
  'makefile',
  'markdown',
  'nginx',
  'objectivec',
  'perl',
  'php',
  'plaintext',
  'powershell',
  'python',
  'r',
  'ruby',
  'rust',
  'scala',
  'scss',
  'shell',
  'sql',
  'swift',
  'typescript',
  'vbnet',
  'xml',
  'yaml',
];

/** How many languages the list shows at once; typing narrows it. */
export const LANGUAGE_LIMIT = 8;

/** Languages starting with `query`, then those containing it. */
export function matchLanguages(query: string): string[] {
  const typed = query.toLowerCase();
  const starts = LANGUAGE_NAMES.filter((name) => name.startsWith(typed));
  const contains = LANGUAGE_NAMES.filter((name) => !name.startsWith(typed) && name.includes(typed));
  return [...starts, ...contains].slice(0, LANGUAGE_LIMIT);
}
