// Messages are typed functions with every language inline (D-018): a missing translation or a
// wrong parameter is a type error, and the bundler keeps only the messages a page imports.
import { getLocale, type Locale } from './locale.svelte.ts';

type Texts = Readonly<Record<Locale, string>>;
type Templates<P> = Readonly<Record<Locale, (params: P) => string>>;

/** A message without parameters. */
export function msg(texts: Texts): () => string;
/** A message with parameters; every language receives the same parameters. */
export function msg<P extends object>(templates: Templates<P>): (params: P) => string;
export function msg<P extends object>(texts: Texts | Templates<P>): (params: P) => string {
  return (params: P) => {
    const text = texts[getLocale()];
    return typeof text === 'string' ? text : text(params);
  };
}

/** Plural categories each language uses (CLDR). */
interface PluralCategories {
  en: 'one' | 'other';
  ru: 'one' | 'few' | 'many' | 'other';
}

const pluralRules = new Map<Locale, Intl.PluralRules>();
const numberFormats = new Map<Locale, Intl.NumberFormat>();

function rulesFor(locale: Locale): Intl.PluralRules {
  let rules = pluralRules.get(locale);
  if (!rules) {
    rules = new Intl.PluralRules(locale);
    pluralRules.set(locale, rules);
  }
  return rules;
}

/** Formats a number for the given language (digit grouping, decimal separator). */
export function formatNumber(locale: Locale, value: number): string {
  let format = numberFormats.get(locale);
  if (!format) {
    format = new Intl.NumberFormat(locale);
    numberFormats.set(locale, format);
  }
  return format.format(value);
}

/**
 * Picks the plural form of `count` and puts the formatted number in place of `#`.
 * `plural('ru', 3, { one: '# задача', few: '# задачи', many: '# задач', other: '# задачи' })`.
 */
export function plural<L extends Locale>(
  locale: L,
  count: number,
  forms: Readonly<Record<PluralCategories[L], string>>,
): string {
  const byCategory = forms as Readonly<Record<string, string | undefined>>;
  const form = byCategory[rulesFor(locale).select(count)] ?? byCategory.other ?? '';
  return form.replaceAll('#', formatNumber(locale, count));
}
