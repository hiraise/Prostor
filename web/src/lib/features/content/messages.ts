// Texts of rendered Markdown and the Markdown editor (CNT-029, CNT-034, CNT-040…CNT-045, CNT-054).
// Voice and glossary: docs/design.md §12.
import { msg, plural } from '#lib/i18n/message.ts';

// ---- Rendered text ----

export const resolved = msg({ en: 'resolved', ru: 'решена' });
export const copyCode = msg({ en: 'Copy code', ru: 'Скопировать код' });
export const codeCopied = msg({ en: 'Code copied', ru: 'Код скопирован' });
export const codeIn = msg<{ language: string }>({
  en: ({ language }) => `Code, ${language}`,
  ru: ({ language }) => `Код, ${language}`,
});
export const code = msg({ en: 'Code', ru: 'Код' });
export const table = msg({ en: 'Table', ru: 'Таблица' });
export const fileMissing = msg({ en: 'file not found', ru: 'файл не найден' });
export const namedFileMissing = msg<{ name: string }>({
  en: ({ name }) => `file “${name}” not found`,
  ru: ({ name }) => `файл «${name}» не найден`,
});

// ---- Editor ----

export const modes = msg<{ label: string }>({
  en: ({ label }) => `${label}: text or preview`,
  ru: ({ label }) => `${label}: текст или предпросмотр`,
});
export const write = msg({ en: 'Text', ru: 'Текст' });
export const preview = msg({ en: 'Preview', ru: 'Просмотр' });
export const formatting = msg<{ label: string }>({
  en: ({ label }) => `Formatting of “${label}”`,
  ru: ({ label }) => `Оформление поля «${label}»`,
});
export const showFormatting = msg({
  en: 'Show formatting buttons',
  ru: 'Показать кнопки оформления',
});
export const hideFormatting = msg({
  en: 'Hide formatting buttons',
  ru: 'Скрыть кнопки оформления',
});
export const markdownHint = msg({
  en: 'Markdown is supported.',
  ru: 'Поддерживается Markdown.',
});
export const nothingToPreview = msg({ en: 'Nothing to preview yet.', ru: 'Пока нечего показать.' });
export const previewLoading = msg({ en: 'Preparing the preview…', ru: 'Готовим предпросмотр…' });
export const column = msg({ en: 'Column', ru: 'Столбец' });

export const actions = {
  heading: msg({ en: 'Heading', ru: 'Заголовок' }),
  bold: msg({ en: 'Bold', ru: 'Полужирный' }),
  italic: msg({ en: 'Italic', ru: 'Курсив' }),
  strikethrough: msg({ en: 'Strikethrough', ru: 'Зачёркнутый' }),
  code: msg({ en: 'Inline code', ru: 'Код в строке' }),
  link: msg({ en: 'Link', ru: 'Ссылка' }),
  quote: msg({ en: 'Quote', ru: 'Цитата' }),
  bullets: msg({ en: 'Bulleted list', ru: 'Маркированный список' }),
  numbers: msg({ en: 'Numbered list', ru: 'Нумерованный список' }),
  tasks: msg({ en: 'Checklist', ru: 'Чек-лист' }),
  codeBlock: msg({ en: 'Code block', ru: 'Блок кода' }),
  table: msg({ en: 'Table', ru: 'Таблица' }),
};

// ---- The language of a code block (CNT-045) ----

export const languageList = msg({ en: 'Code language', ru: 'Язык кода' });
export const languageCount = msg<{ count: number }>({
  en: ({ count }) =>
    count === 0
      ? 'No such language; it will be detected'
      : plural('en', count, { one: '# language', other: '# languages' }),
  ru: ({ count }) =>
    count === 0
      ? 'Такого языка нет, он определится сам'
      : plural('ru', count, {
          one: '# язык',
          few: '# языка',
          many: '# языков',
          other: '# языка',
        }),
});

// ---- Mentions (CNT-054) ----

export const mentionList = msg({ en: 'People to mention', ru: 'Кого упомянуть' });
export const mentionCount = msg<{ count: number }>({
  en: ({ count }) =>
    count === 0
      ? 'Nobody found'
      : plural('en', count, { one: '# person found', other: '# people found' }),
  ru: ({ count }) =>
    count === 0
      ? 'Никого не нашлось'
      : plural('ru', count, {
          one: 'Нашёлся # человек',
          few: 'Нашлось # человека',
          many: 'Нашлось # человек',
          other: 'Нашлось # человека',
        }),
});

// ---- Files (CNT-034) ----

export const filesHint = msg({
  en: 'Paste or drop files to attach them.',
  ru: 'Файлы можно вставить или перетащить сюда.',
});
export const uploading = msg<{ count: number }>({
  en: ({ count }) => plural('en', count, { one: 'Uploading # file…', other: 'Uploading # files…' }),
  ru: ({ count }) =>
    plural('ru', count, {
      one: 'Загружается # файл…',
      few: 'Загружаются # файла…',
      many: 'Загружаются # файлов…',
      other: 'Загружаются # файла…',
    }),
});
export const uploaded = msg<{ count: number }>({
  en: ({ count }) => plural('en', count, { one: '# file added', other: '# files added' }),
  ru: ({ count }) =>
    plural('ru', count, {
      one: 'Добавлен # файл',
      few: 'Добавлено # файла',
      many: 'Добавлено # файлов',
      other: 'Добавлено # файла',
    }),
});
export const uploadFailed = msg<{ name: string; reason: string }>({
  en: ({ name, reason }) => `“${name}” was not uploaded. ${reason}`,
  ru: ({ name, reason }) => `Файл «${name}» не загружен. ${reason}`,
});
export const insertAttachment = msg({ en: 'Attachment', ru: 'Вложение' });
export const attachmentList = msg({ en: 'Files of the issue', ru: 'Файлы задачи' });
