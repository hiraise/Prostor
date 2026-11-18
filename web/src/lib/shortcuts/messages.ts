// Texts of the shortcut framework (step 2.2 §3): the label of every binding, for the overlay
// and tooltips, and the names of the contexts and groups the overlay groups by. The conflict
// test walks these against the tables, so a binding without a label fails the build.
import { msg } from '#lib/i18n/message.ts';

import type { ContextId } from './kinds.ts';

export const labels = {
  overlay: msg({ en: 'Show this reference', ru: 'Показать эту справку' }),
  'focus-query': msg({ en: 'Focus the query bar', ru: 'Перейти к строке запроса' }),
  create: msg({ en: 'The Create menu', ru: 'Меню «Создать»' }),
  'copy.id': msg({ en: 'Copy the ID', ru: 'Скопировать идентификатор' }),
  'copy.link': msg({ en: 'Copy the link', ru: 'Скопировать ссылку' }),
  'copy.full': msg({ en: 'Copy the ID with the summary', ru: 'Скопировать идентификатор с темой' }),
  command: msg({ en: 'Command dialog', ru: 'Диалог команд' }),
  'command.repeat': msg({ en: 'Repeat the last command', ru: 'Повторить последнюю команду' }),
  rail: msg({
    en: 'Collapse or expand the navigation rail',
    ru: 'Свернуть или развернуть панель навигации',
  }),
  'go.issues': msg({ en: 'Go to Issues', ru: 'Перейти в «Задачи»' }),
  'go.boards': msg({ en: 'Go to Boards', ru: 'Перейти в «Доски»' }),
  'go.projects': msg({ en: 'Go to Projects', ru: 'Перейти в «Проекты»' }),
  'go.admin': msg({ en: 'Go to Administration', ru: 'Перейти в «Администрирование»' }),

  'list.up': msg({ en: 'The row above', ru: 'Строка выше' }),
  'list.down': msg({ en: 'The row below', ru: 'Строка ниже' }),
  'list.home': msg({ en: 'The first row', ru: 'Первая строка' }),
  'list.end': msg({ en: 'The last row', ru: 'Последняя строка' }),
  'list.page-prev': msg({ en: 'Previous page', ru: 'Предыдущая страница' }),
  'list.page-next': msg({ en: 'Next page', ru: 'Следующая страница' }),
  'list.open': msg({ en: 'Open the issue', ru: 'Открыть задачу' }),
  'list.open.key': msg({ en: 'Open the issue', ru: 'Открыть задачу' }),
  'list.open.new': msg({ en: 'Open in a new tab', ru: 'Открыть в новой вкладке' }),
  'list.edit': msg({ en: 'Edit the issue', ru: 'Редактировать задачу' }),
  'list.select': msg({ en: 'Toggle the row’s selection', ru: 'Отметить строку или снять отметку' }),
  'list.select.up': msg({ en: 'Extend the selection up', ru: 'Расширить отметку вверх' }),
  'list.select.down': msg({ en: 'Extend the selection down', ru: 'Расширить отметку вниз' }),
  'list.select.page': msg({ en: 'Select the page', ru: 'Отметить страницу' }),
  'list.assign-me': msg({ en: 'Assign to me', ru: 'Назначить на себя' }),
  'list.star': msg({ en: 'Star', ru: 'В избранное' }),
  'list.sidebar': msg({ en: 'Show or hide the sidebar', ru: 'Показать или скрыть боковую панель' }),

  'issue.edit': msg({ en: 'Edit the issue', ru: 'Редактировать задачу' }),
  'issue.comment': msg({ en: 'Write a comment', ru: 'Написать комментарий' }),
  'issue.history': msg({
    en: 'Toggle history in the activity',
    ru: 'Показать или скрыть изменения',
  }),
  'issue.comments': msg({
    en: 'Toggle comments in the activity',
    ru: 'Показать или скрыть комментарии',
  }),
  'issue.assign-me': msg({ en: 'Assign to me', ru: 'Назначить на себя' }),
  'issue.star': msg({ en: 'Star', ru: 'В избранное' }),
  'issue.attach': msg({ en: 'Attach files', ru: 'Прикрепить файлы' }),
  'issue.tags': msg({ en: 'Open the tag picker', ru: 'Открыть выбор тегов' }),

  'editor.bold': msg({ en: 'Bold', ru: 'Полужирный' }),
  'editor.italic': msg({ en: 'Italic', ru: 'Курсив' }),
  'editor.strike': msg({ en: 'Strikethrough', ru: 'Зачёркнутый' }),
  'editor.code': msg({ en: 'Inline code', ru: 'Код в строке' }),
  'editor.submit': msg({ en: 'Submit', ru: 'Отправить' }),
  'editor.cancel': msg({ en: 'Cancel', ru: 'Отмена' }),

  'dialog.apply': msg({ en: 'Apply the command', ru: 'Применить команду' }),
  'dialog.apply.primary': msg({ en: 'Apply the command', ru: 'Применить команду' }),
  'dialog.apply.silent': msg({
    en: 'Apply without notifications',
    ru: 'Применить без уведомлений',
  }),
  'dialog.cancel': msg({ en: 'Close the dialog', ru: 'Закрыть диалог' }),

  'query.suggest': msg({ en: 'Query suggestions', ru: 'Подсказки по запросу' }),
  'query.run': msg({ en: 'Run the search', ru: 'Выполнить поиск' }),
  'query.leave': msg({ en: 'Leave the query bar', ru: 'Покинуть строку запроса' }),
} as const;

export type BindingId = keyof typeof labels;

/** The groups the overlay gathers bindings by, within a context (§5). */
export const groups = {
  navigation: msg({ en: 'Navigation', ru: 'Навигация' }),
  actions: msg({ en: 'Actions', ru: 'Действия' }),
  reference: msg({ en: 'Reference', ru: 'Справка' }),
  rows: msg({ en: 'Rows', ru: 'Строки' }),
  opening: msg({ en: 'Opening', ru: 'Открытие' }),
  selection: msg({ en: 'Selection', ru: 'Отметки' }),
  fields: msg({ en: 'Fields', ru: 'Поля' }),
  text: msg({ en: 'Text', ru: 'Текст' }),
  applying: msg({ en: 'Applying and closing', ru: 'Применение и закрытие' }),
} as const;

export type GroupId = keyof typeof groups;

/** The group a global binding shows under in the overlay. The conflict test walks these against
 * the tables, so a binding without a group fails the build. */
export const globalGroups: Readonly<Partial<Record<BindingId, GroupId>>> = {
  overlay: 'reference',
  'focus-query': 'navigation',
  'go.issues': 'navigation',
  'go.boards': 'navigation',
  'go.projects': 'navigation',
  'go.admin': 'navigation',
  rail: 'navigation',
  create: 'actions',
  'copy.id': 'actions',
  'copy.link': 'actions',
  'copy.full': 'actions',
  command: 'actions',
  'command.repeat': 'actions',
};

/** The groups of a context's bindings, in display order. */
export const contextGroups: Readonly<Record<ContextId, readonly GroupId[]>> = {
  list: ['rows', 'opening', 'selection', 'actions'],
  issue: ['opening', 'fields', 'actions'],
  editor: ['text', 'applying'],
  dialog: ['applying'],
  query: ['actions'],
};

/** The binding group map for each context table. The conflict test walks these against the
 * tables, so a binding without a group fails the build. */
export const bindingGroups: Readonly<
  Record<ContextId, Readonly<Partial<Record<BindingId, GroupId>>>>
> = {
  list: {
    'list.up': 'rows',
    'list.down': 'rows',
    'list.home': 'rows',
    'list.end': 'rows',
    'list.page-prev': 'rows',
    'list.page-next': 'rows',
    'list.open': 'opening',
    'list.open.key': 'opening',
    'list.open.new': 'opening',
    'list.edit': 'opening',
    'list.select': 'selection',
    'list.select.up': 'selection',
    'list.select.down': 'selection',
    'list.select.page': 'selection',
    'list.assign-me': 'actions',
    'list.star': 'actions',
    'list.sidebar': 'actions',
  },
  issue: {
    'issue.edit': 'opening',
    'issue.comment': 'fields',
    'issue.history': 'fields',
    'issue.comments': 'fields',
    'issue.assign-me': 'actions',
    'issue.star': 'actions',
    'issue.attach': 'fields',
    'issue.tags': 'fields',
  },
  editor: {
    'editor.bold': 'text',
    'editor.italic': 'text',
    'editor.strike': 'text',
    'editor.code': 'text',
    'editor.submit': 'applying',
    'editor.cancel': 'applying',
  },
  dialog: {
    'dialog.apply': 'applying',
    'dialog.apply.primary': 'applying',
    'dialog.apply.silent': 'applying',
    'dialog.cancel': 'applying',
  },
  query: {
    'query.suggest': 'actions',
    'query.run': 'actions',
    'query.leave': 'actions',
  },
};

/** The name of a context's section in the overlay. */
export const contextTitles: Readonly<Record<ContextId, () => string>> = {
  list: msg({ en: 'The issue list', ru: 'Список задач' }),
  issue: msg({ en: 'The issue page', ru: 'Страница задачи' }),
  editor: msg({ en: 'The text editor', ru: 'Редактор текста' }),
  dialog: msg({ en: 'The command dialog', ru: 'Диалог команд' }),
  query: msg({ en: 'The query bar', ru: 'Строка запроса' }),
};

export const globalTitle = msg({ en: 'Everywhere', ru: 'Везде' });
