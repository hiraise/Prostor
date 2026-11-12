// Syntax highlighting of code blocks (CNT-045), in the browser: highlight.js and its languages
// load on the first code block (`languages.ts`, a chunk of its own). A block names its language
// in its fence (`language-…` class); a block without one gets the language that fits best, if any
// fits well; `text` (and its synonyms) means no highlighting.

type Highlighter = (typeof import('./languages.ts'))['default'];

/** Languages that mean "no highlighting". */
const PLAIN = new Set(['text', 'txt', 'plain', 'plaintext', 'nohighlight']);
/** Longer code is shown as it is: highlighting it would hold up the page. */
const MAX_LENGTH = 100_000;
/** How sure auto-detection must be (highlight.js relevance) before untagged code is colored. */
const MIN_RELEVANCE = 5;

let loading: Promise<Highlighter> | null = null;

function load(): Promise<Highlighter> {
  loading ??= import('./languages.ts').then(
    (module) => module.default,
    (error: unknown) => {
      // A failed download is tried again with the next code block.
      loading = null;
      throw error;
    },
  );
  return loading;
}

/** The language a code element names (`class="language-rust"`), or null. */
export function codeLanguage(code: Element): string | null {
  for (const name of code.classList) {
    if (name.startsWith('language-')) return name.slice('language-'.length) || null;
  }
  return null;
}

/** Whether a code block in `language` gets highlighting at all. */
export function isHighlighted(language: string | null): boolean {
  return language === null || !PLAIN.has(language.toLowerCase());
}

/**
 * Colors `code` in place. Resolves once done or skipped; the element keeps its plain text when
 * the language is unknown, detection is unsure, or the code changed meanwhile.
 */
export async function highlightCode(code: HTMLElement, language: string | null): Promise<void> {
  if (!isHighlighted(language)) return;
  const text = code.textContent;
  if (text.length === 0 || text.length > MAX_LENGTH) return;
  const hljs = await load();
  if (!code.isConnected || code.textContent !== text) return;
  let html: string;
  if (language !== null) {
    if (!hljs.getLanguage(language)) return;
    html = hljs.highlight(text, { language, ignoreIllegals: true }).value;
  } else {
    const found = hljs.highlightAuto(text);
    if (found.relevance < MIN_RELEVANCE) return;
    html = found.value;
  }
  // highlight.js escapes the code; its markup is only spans with `hljs-…` classes.
  code.innerHTML = html;
  code.classList.add('hljs');
}
