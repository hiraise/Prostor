// Texts shared by many screens: generic actions and states. Voice and tone: docs/design.md §11.
import { msg, plural } from '../message.ts';

export const retry = msg({ en: 'Try again', ru: 'Повторить' });
export const close = msg({ en: 'Close', ru: 'Закрыть' });
export const cancel = msg({ en: 'Cancel', ru: 'Отмена' });
export const dismiss = msg({ en: 'Dismiss', ru: 'Скрыть' });
export const undo = msg({ en: 'Undo', ru: 'Отменить' });
export const undone = msg({ en: 'Undone', ru: 'Отменено' });
export const loading = msg({ en: 'Loading…', ru: 'Загрузка…' });
export const showDetails = msg({ en: 'Details', ru: 'Подробнее' });
export const copyLink = msg({ en: 'Copy link', ru: 'Скопировать ссылку' });
export const linkCopied = msg({ en: 'Link copied', ru: 'Ссылка скопирована' });
export const copied = msg({ en: 'Copied', ru: 'Скопировано' });
export const copyFailed = msg({
  en: 'Could not copy. Select the text and copy it by hand.',
  ru: 'Не получилось скопировать. Выделите текст и скопируйте вручную.',
});
export const requiredMark = msg({ en: 'required', ru: 'обязательно' });
export const statusMessages = msg({ en: 'Status messages', ru: 'Сообщения о действиях' });
export const requestId = msg<{ id: string }>({
  en: ({ id }) => `Request ID: ${id}`,
  ru: ({ id }) => `Код запроса: ${id}`,
});
export const search = msg({ en: 'Search', ru: 'Поиск' });
export const nothingFound = msg({ en: 'Nothing found', ru: 'Ничего не найдено' });
export const found = msg<{ count: number }>({
  en: ({ count }) => plural('en', count, { one: '# found', other: '# found' }),
  ru: ({ count }) =>
    plural('ru', count, {
      one: 'Найден #',
      few: 'Найдено #',
      many: 'Найдено #',
      other: 'Найдено #',
    }),
});
