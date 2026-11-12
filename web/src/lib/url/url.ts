// Addressable views (UI-006): every view's state that matters for sharing — a query, a filter, a
// selected tab — lives in its URL, and entity URLs may carry a readable slug that is never needed
// to resolve them.

/**
 * A readable URL part from a title: letters and digits of any script, lowercased, joined by
 * hyphens, cut at a word boundary. Empty when nothing readable is left.
 */
export function readableSlug(title: string, maxLength = 60): string {
  const words = title
    .normalize('NFKC')
    .toLowerCase()
    .split(/[^\p{L}\p{N}]+/u)
    .filter(Boolean);
  let slug = '';
  for (const word of words) {
    const next = slug ? `${slug}-${word}` : word;
    if (next.length > maxLength) break;
    slug = next;
  }
  return slug || (words[0]?.slice(0, maxLength) ?? '');
}

/**
 * `href` with some search parameters set (a string) or removed (`null`/`undefined`), others kept.
 * Use it for links and `goto` so that shared URLs reopen the same view.
 */
export function withSearchParams(
  href: string | URL,
  changes: Readonly<Record<string, string | null | undefined>>,
): string {
  const url = new URL(href, 'http://local.invalid');
  for (const [key, value] of Object.entries(changes)) {
    if (value === null || value === undefined || value === '') url.searchParams.delete(key);
    else url.searchParams.set(key, value);
  }
  return url.pathname + url.search + url.hash;
}
