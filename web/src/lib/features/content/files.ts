// Files in rich text (CNT-034, CNT-044): the Markdown the editor inserts for an uploaded file and
// for a URL pasted over a selection, and the names behind the renderer's `tf-file:` markers.
// Names are percent-encoded in Markdown so spaces and brackets survive; the server decodes them
// with the same rule as `percentDecode` (crates/markup), so both sides agree on a file's name.
import type { FileKind } from '#lib/api/schema.gen.ts';

/** Characters a link destination cannot hold as written, or that would change its meaning. */
const UNSAFE = /[\s%()[\]<>\\|"':#?]|\p{Cc}/gu;

/** A file name as a Markdown link destination: `my shot.png` → `my%20shot.png`. */
export function encodeFileName(name: string): string {
  const encoded = name.replace(UNSAFE, (char) =>
    [...new TextEncoder().encode(char)]
      .map((byte) => `%${byte.toString(16).toUpperCase().padStart(2, '0')}`)
      .join(''),
  );
  // A leading dot or slash reads as a path, not a file of the issue.
  return encoded.replace(/^[./]/, (char) => (char === '.' ? '%2E' : '%2F'));
}

/** Text shown as a link's text, with Markdown's own marks escaped. */
function escapeText(text: string): string {
  return text.replace(/[\\`*_[\]<>~|]/g, '\\$&');
}

/** What the editor inserts for an uploaded file: pictures and videos show, others are links. */
export function fileMarkdown(file: { name: string; kind: FileKind }): string {
  const target = encodeFileName(file.name);
  return file.kind === 'image' || file.kind === 'video'
    ? `![](${target})`
    : `[${escapeText(file.name)}](${target})`;
}

const WEB_ADDRESS = /^https?:\/\/\S+$/i;

/** A web address pasted over `selected` makes the selection a link to it; `null` otherwise. */
export function linkOverSelection(selected: string, pasted: string): string | null {
  const url = pasted.trim();
  if (selected.trim() === '' || selected.includes('\n') || !WEB_ADDRESS.test(url)) return null;
  // The selection is an address itself: the paste simply replaces it.
  if (WEB_ADDRESS.test(selected.trim())) return null;
  // Brackets would end the destination early.
  const target = url.replace(
    /[()<>]/g,
    (char) => `%${char.charCodeAt(0).toString(16).toUpperCase()}`,
  );
  return `[${selected}](${target})`;
}

function hex(code: number): number | null {
  if (code >= 48 && code <= 57) return code - 48;
  if (code >= 65 && code <= 70) return code - 55;
  if (code >= 97 && code <= 102) return code - 87;
  return null;
}

/** Decodes `%XX` sequences; invalid ones stay as written, invalid UTF-8 becomes U+FFFD. */
export function percentDecode(text: string): string {
  const bytes = new TextEncoder().encode(text);
  const out: number[] = [];
  for (let i = 0; i < bytes.length; i += 1) {
    const byte = bytes[i] ?? 0;
    if (byte === 37) {
      const high = hex(bytes[i + 1] ?? 0);
      const low = hex(bytes[i + 2] ?? 0);
      if (high !== null && low !== null) {
        out.push(high * 16 + low);
        i += 2;
        continue;
      }
    }
    out.push(byte);
  }
  return new TextDecoder().decode(new Uint8Array(out));
}

/** The prefix of file markers in the server's HTML. */
export const FILE_MARKER = 'tf-file:';

/** The file name a `tf-file:` marker names, or `null` for other addresses. */
export function markerName(href: string): string | null {
  return href.startsWith(FILE_MARKER) ? percentDecode(href.slice(FILE_MARKER.length)) : null;
}
