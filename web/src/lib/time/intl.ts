// Intl formatters are expensive to create and cheap to reuse: one instance per locale and options.

const dateTimeFormats = new Map<string, Intl.DateTimeFormat>();
const relativeTimeFormats = new Map<string, Intl.RelativeTimeFormat>();

export function dateTimeFormat(
  locale: string,
  options: Intl.DateTimeFormatOptions,
): Intl.DateTimeFormat {
  // Options are built by this module's callers in a fixed key order, so the JSON is a stable key.
  const key = `${locale}|${JSON.stringify(options)}`;
  let format = dateTimeFormats.get(key);
  if (!format) {
    format = new Intl.DateTimeFormat(locale, options);
    dateTimeFormats.set(key, format);
  }
  return format;
}

export function relativeTimeFormat(locale: string): Intl.RelativeTimeFormat {
  let format = relativeTimeFormats.get(locale);
  if (!format) {
    format = new Intl.RelativeTimeFormat(locale, { numeric: 'auto', style: 'long' });
    relativeTimeFormats.set(locale, format);
  }
  return format;
}
