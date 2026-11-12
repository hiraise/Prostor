// The viewer's time zone, the instance's date format, and the context every date in the interface
// is shown with. The zone is the browser's until the personal zone setting (UI-011) arrives; it
// will call `setTimeZone`. The format follows the instance's own choice (ADM-016), set here as the
// configuration arrives; personal formats are P2.
import type { DateFormat } from '#lib/api/schema.gen.ts';
import { getLocale } from '#lib/i18n/locale.svelte.ts';

import type { TimeContext } from './instant.ts';
import { browserTimeZone, isValidTimeZone } from './zone.ts';

let zone = $state(browserTimeZone());
let format = $state<DateFormat>('locale');

/** The viewer's time zone. Reading it inside a component or `$derived` tracks changes. */
export function getTimeZone(): string {
  return zone;
}

/** Switches the zone every date is shown in; false (and no change) for an unknown zone name. */
export function setTimeZone(timeZone: string): boolean {
  if (!isValidTimeZone(timeZone)) return false;
  zone = timeZone;
  return true;
}

/** The instance's date format; `'locale'` when dates follow the interface language. */
export function getInstanceFormat(): DateFormat {
  return format;
}

/** Sets the format every date is shown in; the root layout calls it as the configuration loads. */
export function setInstanceFormat(dateFormat: DateFormat): void {
  format = dateFormat;
}

/** The current language, zone and date format; reading it inside a component tracks all three. */
export function getTimeContext(): TimeContext {
  return format === 'locale'
    ? { locale: getLocale(), timeZone: zone }
    : { locale: getLocale(), timeZone: zone, dateFormat: format };
}
