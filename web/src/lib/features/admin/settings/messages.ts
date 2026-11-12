// Texts of the admin settings screen (ADM-002…ADM-017, AUTH-004, UI-004; plan step 1.11 §4).
// Voice and glossary: docs/design.md §12.
import { msg } from '#lib/i18n/message.ts';

export const settingsTitle = msg({ en: 'Settings', ru: 'Настройки' });
export const settingsIntro = msg({
  en: 'How this instance as a whole behaves: its address and sessions, the language and the format of dates it writes, and what it is.',
  ru: 'Как ведёт себя весь инстанс: его адрес и сессии, язык и формат дат в его текстах, и что он сам о себе знает.',
});
export const readOnlyNote = msg({
  en: 'Only system administrators can change these settings.',
  ru: 'Менять эти настройки могут только системные администраторы.',
});
export const serverTab = msg({ en: 'Server', ru: 'Сервер' });
export const localizationTab = msg({ en: 'Localization', ru: 'Локализация' });
export const aboutTab = msg({ en: 'About', ru: 'О системе' });
export const tabsLabel = msg({ en: 'Settings sections', ru: 'Разделы настроек' });

// — Server —
export const baseUrlLabel = msg({ en: 'Instance address', ru: 'Адрес инстанса' });
export const baseUrlHint = msg({
  en: 'The full address letters and invitations link to, such as https://track.example.com. Empty: links fall back to the address of the request.',
  ru: 'Полный адрес, на который ссылаются письма и приглашения, например https://track.example.com. Пусто — ссылки собираются из адреса самого запроса.',
});
export const baseUrlInvalid = msg({
  en: 'A full address starting with http:// or https://.',
  ru: 'Полный адрес, начинающийся с http:// или https://.',
});
export const baseUrlHttpWarning = msg({
  en: 'The address starts with http://: everything travels the network unencrypted. https:// is the safe choice.',
  ru: 'Адрес начинается с http:// — всё идёт по сети открытым текстом. Безопасный выбор — https://.',
});
export const baseUrlOriginWarning = msg<{ origin: string }>({
  en: ({ origin }) =>
    `This differs from the address this page came from (${origin}). Letters will point people where you typed, not here.`,
  ru: ({ origin }) =>
    `Отличается от адреса, по которому открыта эта страница (${origin}). Письма будут вести туда, что вы указали, а не сюда.`,
});
export const baseUrlChecklistTitle = msg({
  en: 'The address has changed. What follows:',
  ru: 'Адрес изменился. Что из этого следует:',
});
export const baseUrlChecklistLetters = msg({
  en: 'letters and invitations now link to the new address;',
  ru: 'письма и приглашения теперь ссылаются на новый адрес;',
});
export const baseUrlChecklistOld = msg({
  en: 'the old address keeps working while traffic still reaches it.',
  ru: 'прежний адрес продолжает работать, пока на него идёт трафик.',
});
export const uploadLabel = msg({ en: 'Largest attachment', ru: 'Наибольший вложенный файл' });
export const uploadHint = msg<{ ceiling: string }>({
  en: ({ ceiling }) => `This server allows at most ${ceiling}.`,
  ru: ({ ceiling }) => `Этот сервер допускает не больше ${ceiling}.`,
});
export const uploadTooBig = msg<{ ceiling: string }>({
  en: ({ ceiling }) => `More than the server allows (${ceiling}).`,
  ru: ({ ceiling }) => `Больше, чем допускает сервер (${ceiling}).`,
});
export const uploadNotWhole = msg({
  en: 'A whole number of megabytes.',
  ru: 'Целое число мегабайт.',
});
export const uploadUnitLabel = msg({ en: 'Unit', ru: 'Единица' });
export const megabyte = msg({ en: 'MB', ru: 'МБ' });
export const gigabyte = msg({ en: 'GB', ru: 'ГБ' });
export const apiOriginsLabel = msg({ en: 'API origins', ru: 'Источники API' });
export const apiOriginsHint = msg({
  en: 'Other sites whose pages may call this API from a browser, one origin per line (https://app.example.com). Scripts bring their own token; pages of this server work without a line. Empty — same-origin only.',
  ru: 'Сайты, чьи страницы могут звать этот API из браузера, по одному источнику в строке (https://app.example.com). Скрипт приносит свой токен; страницам самого сервера строка не нужна. Пусто — только этот сервер.',
});
export const apiOriginsInvalid = msg({
  en: 'Each line is an origin: https://app.example.com, at most a port, no path.',
  ru: 'Каждая строка — источник: https://app.example.com, максимум порт, без пути.',
});
export const sessionIdleLabel = msg({ en: 'Session ends after', ru: 'Сессия завершается после' });
export const sessionIdleHint = msg({
  en: 'Minutes without a request, from 10 to 720. Then the person signs in again.',
  ru: 'Минут без единого запроса, от 10 до 720. После этого нужно войти заново.',
});
export const rememberMeLabel = msg({ en: '“Remember me” lasts', ru: '«Запомнить меня» действует' });
export const rememberMeHint = msg({
  en: 'Days from sign-in, from 1 to 365.',
  ru: 'Дней с момента входа, от 1 до 365.',
});
export const outOfBounds = msg<{ min: string; max: string }>({
  en: ({ min, max }) => `From ${min} to ${max}.`,
  ru: ({ min, max }) => `От ${min} до ${max}.`,
});
export const wholeNumber = msg({ en: 'A whole number.', ru: 'Целое число.' });
export const homeLabel = msg({ en: 'Home page', ru: 'Главная страница' });
export const homeHint = msg({
  en: 'What opens after signing in for people who have not chosen their own.',
  ru: 'Что открывается после входа тем, кто не выбрал свою.',
});
export const homeIssues = msg({ en: 'Issues', ru: 'Задачи' });
export const homeProjects = msg({ en: 'Projects', ru: 'Проекты' });
export const homeBoards = msg({ en: 'Boards', ru: 'Доски' });
export const saveSettings = msg({ en: 'Save', ru: 'Сохранить' });
export const savedToast = msg({ en: 'Settings saved', ru: 'Настройки сохранены' });

// — Localization —
export const languageLabel = msg({ en: 'System language', ru: 'Язык системы' });
export const languageHint = msg({
  en: 'The language of letters and system texts. The interface language stays each person’s own choice.',
  ru: 'Язык писем и служебных текстов. Язык интерфейса остаётся выбором каждого человека.',
});
export const zoneLabel = msg({ en: 'Time zone', ru: 'Часовой пояс' });
export const zoneHint = msg({
  en: 'For people who have not chosen their own; server-side calendars count in it too.',
  ru: 'Для тех, кто не выбрал свой; в нём же считаются календари на стороне сервера.',
});
export const zoneSearch = msg({ en: 'Find a zone', ru: 'Найти пояс' });
export const formatLabel = msg({ en: 'Date and time format', ru: 'Формат даты и времени' });
export const formatHint = msg({
  en: 'For people who have not chosen their own. The sample is the last minute of 2000.',
  ru: 'Для тех, кто не выбрал свой. Образец — последняя минута 2000 года.',
});
export const formatLocale = msg({ en: 'By interface language', ru: 'По языку интерфейса' });
export const formatDmyDot = msg({ en: 'Day first, numbers', ru: 'Сначала день, числами' });
export const formatDmyName = msg({ en: 'Day first, month name', ru: 'Сначала день, имя месяца' });
export const formatIso = msg({ en: 'Year first (ISO)', ru: 'Сначала год (ISO)' });
export const formatDmyNameSample = msg({
  en: '31 Dec 2000, 23:59',
  ru: '31 дек. 2000, 23:59',
});

// — About —
export const aboutVersion = msg({ en: 'Version', ru: 'Версия' });
