// Texts of the issue list (QRY-053, QRY-054, QRY-069, QRY-073, QRY-074, QRY-081) and of the
// problems a query can have. Voice and glossary: docs/design.md §12.
import type { QueryProblemCode } from '#lib/api/schema.gen.ts';
import { msg, plural } from '#lib/i18n/message.ts';

export const title = msg({ en: 'Issues', ru: 'Задачи' });
export const searchBar = msg({ en: 'Issue search', ru: 'Поиск задач' });
export const query = msg({ en: 'Search query', ru: 'Поисковый запрос' });
export const queryPlaceholder = msg({
  en: 'For example: login sort: updated',
  ru: 'Например: вход сортировка: изменена',
});
export const find = msg({ en: 'Find', ru: 'Найти' });
export const clear = msg({ en: 'Clear', ru: 'Очистить' });
export const hideResolved = msg({ en: 'Hide resolved', ru: 'Скрыть решённые' });
export const copyLink = msg({ en: 'Copy link', ru: 'Скопировать ссылку' });
export const applyToAll = msg<{ count: number }>({
  en: ({ count }) => `Apply a command to all ${String(count)} issues…`,
  ru: ({ count }) => `Применить команду ко всем ${String(count)} задачам…`,
});

// The context (QRY-053).
export const context = msg({ en: 'Search in', ru: 'Где искать' });
export const contextOf = msg<{ name: string }>({
  en: ({ name }) => `Search in: ${name}`,
  ru: ({ name }) => `Где искать: ${name}`,
});
export const allIssues = msg({ en: 'All issues', ru: 'Все задачи' });
export const findContext = msg({
  en: 'Find a project, tag or search',
  ru: 'Найти проект, тег или поиск',
});
export const kindProject = msg({ en: 'Project', ru: 'Проект' });
export const kindTag = msg({ en: 'Tag', ru: 'Тег' });
export const kindSearch = msg({ en: 'Saved search', ru: 'Сохранённый поиск' });
export const contextsFailed = msg({
  en: 'The list did not load. Close it and open it again.',
  ru: 'Список не загрузился. Закройте его и откройте снова.',
});
export const contextMissing = msg({
  en: 'This project, tag or saved search does not exist or is hidden from you. All issues are shown.',
  ru: 'Такого проекта, тега или сохранённого поиска нет, или он вам не виден. Показаны все задачи.',
});
export const showConditions = msg({ en: 'Show conditions', ru: 'Показать условия' });
export const hideConditions = msg({ en: 'Hide conditions', ru: 'Скрыть условия' });
export const conditionsOf = msg<{ name: string }>({
  en: ({ name }) => `Conditions of “${name}”`,
  ru: ({ name }) => `Условия поиска «${name}»`,
});

// Under the bar.
export const count = msg<{ count: number }>({
  en: ({ count }) => plural('en', count, { one: '# issue', other: '# issues' }),
  ru: ({ count }) =>
    plural('ru', count, { one: '# задача', few: '# задачи', many: '# задач', other: '# задачи' }),
});
export const saveSearch = msg({ en: 'Save search', ru: 'Сохранить поиск' });
export const updateSearch = msg({ en: 'Update search', ru: 'Обновить поиск' });
export const searchName = msg({ en: 'Name', ru: 'Название' });
export const save = msg({ en: 'Save', ru: 'Сохранить' });
export const cancel = msg({ en: 'Cancel', ru: 'Отмена' });
export const nameRequired = msg({ en: 'Name the search.', ru: 'Дайте поиску название.' });
export const searchSaved = msg<{ name: string }>({
  en: ({ name }) => `Search “${name}” saved`,
  ru: ({ name }) => `Поиск «${name}» сохранён`,
});
export const searchUpdated = msg<{ name: string }>({
  en: ({ name }) => `Search “${name}” updated`,
  ru: ({ name }) => `Поиск «${name}» обновлён`,
});
export const problemsTitle = msg({ en: 'The query has mistakes', ru: 'В запросе есть ошибки' });
export const problemAt = msg<{ text: string; problem: string }>({
  en: ({ text, problem }) => `“${text}”: ${problem}`,
  ru: ({ text, problem }) => `«${text}»: ${problem}`,
});

// Rows.
export const rows = msg({ en: 'Found issues', ru: 'Найденные задачи' });
export const showMore = msg({ en: 'Show more', ru: 'Показать ещё' });
export const loadFailed = msg({
  en: 'The issues did not load. Check the connection and try again.',
  ru: 'Задачи не загрузились. Проверьте соединение и повторите.',
});
export const retry = msg({ en: 'Try again', ru: 'Повторить' });
export const listChanged = msg({ en: 'The list has changed', ru: 'Список изменился' });
export const refreshList = msg({ en: 'Refresh', ru: 'Обновить' });
export const nothingFound = msg({ en: 'No issues found', ru: 'Задачи не найдены' });
export const hintFewer = msg({
  en: 'Remove some conditions from the query.',
  ru: 'Уберите часть условий из запроса.',
});
export const hintValues = msg({
  en: 'Check the values: another spelling or a wider date range may match.',
  ru: 'Проверьте значения: подойдёт другое написание или более широкий период.',
});
export const hintResolved = msg({
  en: 'Resolved issues are hidden. Show them to search among all.',
  ru: 'Решённые задачи скрыты. Покажите их, чтобы искать среди всех.',
});
export const showResolved = msg({ en: 'Show resolved', ru: 'Показать решённые' });

/** What a problem of a query means, by its code. */
export const problems: Record<QueryProblemCode, () => string> = {
  unclosed_quote: msg({ en: 'the quote is not closed', ru: 'кавычка не закрыта' }),
  unclosed_group: msg({ en: 'the parenthesis is not closed', ru: 'скобка не закрыта' }),
  unexpected_close: msg({
    en: 'this parenthesis closes nothing',
    ru: 'эта скобка ничего не закрывает',
  }),
  unexpected_symbol: msg({
    en: 'this symbol is not expected here',
    ru: 'этот знак здесь не ожидается',
  }),
  missing_value: msg({ en: 'a value is missing', ru: 'не хватает значения' }),
  missing_operand: msg({
    en: 'a condition is missing on one side',
    ru: 'с одной стороны не хватает условия',
  }),
  empty_group: msg({ en: 'the parentheses are empty', ru: 'в скобках ничего нет' }),
  sort_twice: msg({ en: 'the order is given twice', ru: 'сортировка указана дважды' }),
  sort_in_group: msg({
    en: 'the order cannot be inside parentheses',
    ru: 'сортировка не может быть в скобках',
  }),
  bad_range: msg({ en: 'the range is not valid', ru: 'диапазон указан неверно' }),
  too_deep: msg({ en: 'too many nested parentheses', ru: 'слишком много вложенных скобок' }),
  too_many_terms: msg({ en: 'too many conditions', ru: 'слишком много условий' }),
  query_too_long: msg({ en: 'the query is too long', ru: 'запрос слишком длинный' }),
  unknown_attribute: msg({ en: 'no such field', ru: 'такого поля нет' }),
  unknown_value: msg({ en: 'no such value', ru: 'такого значения нет' }),
  ambiguous_user: msg({
    en: 'several people match; write the username',
    ru: 'подходит несколько человек; укажите имя пользователя',
  }),
  wrong_operator: msg({
    en: 'this comparison does not fit the field',
    ru: 'такое сравнение не подходит полю',
  }),
  bad_date: msg({ en: 'the date is not valid', ru: 'дата указана неверно' }),
  bad_number: msg({ en: 'the number is not valid', ru: 'число указано неверно' }),
  not_sortable: msg({
    en: 'issues cannot be sorted by this',
    ru: 'по этому задачи не сортируются',
  }),
  too_many_sort_keys: msg({ en: 'too many sort keys', ru: 'слишком много ключей сортировки' }),
  deleted_reference: msg({ en: 'this was deleted', ru: 'это удалено' }),
  same_field_twice: msg({
    en: 'the field is in the order twice',
    ru: 'поле указано в сортировке дважды',
  }),
  field_shadowed: msg({
    en: 'a field has this name too; the word is read as the field',
    ru: 'так же называется поле; слово понято как поле',
  }),
  hide_resolved_ignored: msg({
    en: 'the query asks about resolution, so resolved issues are not hidden',
    ru: 'запрос задаёт решённость, поэтому решённые задачи не скрыты',
  }),
  ignored_common_word: msg({
    en: 'a common word; the search leaves it out',
    ru: 'частое слово; поиск его не учитывает',
  }),
};

// The table, the phone cards and the columns (QRY-075, QRY-076, QRY-078, LNK-014).
export const listSettings = msg({ en: 'List settings', ru: 'Настройки списка' });
export const columnsMenu = msg({ en: 'Columns…', ru: 'Столбцы…' });
export const columnNames = {
  id: msg({ en: 'ID', ru: 'ID' }),
  summary: msg({ en: 'Summary', ru: 'Заголовок' }),
  project: msg({ en: 'Project', ru: 'Проект' }),
  reporter: msg({ en: 'Reporter', ru: 'Автор' }),
  created: msg({ en: 'Created', ru: 'Создана' }),
  updated: msg({ en: 'Updated', ru: 'Изменена' }),
  resolved: msg({ en: 'Resolved', ru: 'Решена' }),
  votes: msg({ en: 'Votes', ru: 'Голоса' }),
  tags: msg({ en: 'Tags', ru: 'Теги' }),
};
export const starColumn = msg({ en: 'Watching', ru: 'Наблюдение' });
/** The header over the row's clone menu (AUT-066). */
export const actionsColumn = msg({ en: 'Actions', ru: 'Действия' });
export const starIssue = msg<{ id: string }>({
  en: ({ id }) => `Watch ${id}`,
  ru: ({ id }) => `Наблюдать за ${id}`,
});
export const sortBy = msg<{ name: string }>({
  en: ({ name }) => `Sort by ${name}`,
  ru: ({ name }) => `Сортировать: ${name}`,
});
export const progress = msg<{ done: number; total: number }>({
  en: ({ done, total }) => `${String(done)} of ${String(total)} subtasks resolved`,
  ru: ({ done, total }) => `Решено подзадач: ${String(done)} из ${String(total)}`,
});
export const fieldOf = msg<{ name: string; id: string }>({
  en: ({ name, id }) => `${name} of ${id}`,
  ru: ({ name, id }) => `${name} у ${id}`,
});
export const more = msg({ en: 'More', ru: 'Ещё' });
export const moreOf = msg<{ id: string }>({
  en: ({ id }) => `More for ${id}`,
  ru: ({ id }) => `Ещё для ${id}`,
});
export const fieldSaved = msg<{ name: string }>({
  en: ({ name }) => `${name} saved`,
  ru: ({ name }) => `${name}: сохранено`,
});
export const columnsTitle = msg({ en: 'Columns', ru: 'Столбцы' });
export const shownColumns = msg({ en: 'Shown columns', ru: 'Показанные столбцы' });
export const moveUp = msg<{ name: string }>({
  en: ({ name }) => `Move ${name} up`,
  ru: ({ name }) => `Поднять ${name}`,
});
export const moveDown = msg<{ name: string }>({
  en: ({ name }) => `Move ${name} down`,
  ru: ({ name }) => `Опустить ${name}`,
});
export const removeColumn = msg<{ name: string }>({
  en: ({ name }) => `Remove ${name}`,
  ru: ({ name }) => `Убрать ${name}`,
});
export const fieldsOf = msg({ en: 'Fields of', ru: 'Поля проекта' });
export const allProjects = msg({ en: 'All projects', ru: 'Все проекты' });
export const addColumn = msg({ en: 'Column to add', ru: 'Добавить столбец' });
export const add = msg({ en: 'Add', ru: 'Добавить' });
export const nothingToAdd = msg({ en: 'Every column is shown', ru: 'Все столбцы уже показаны' });
export const resetColumns = msg({ en: 'Reset to default', ru: 'Как по умолчанию' });
export const fieldsFailed = msg({
  en: 'The fields did not load.',
  ru: 'Поля не загрузились.',
});

// The keyboard (step 2.2 §6, QRY-085): the move's announcement and the assign-to-me answer.
export const focusedRow = msg<{ id: string; summary: string }>({
  en: ({ id, summary }) => `${id} ${summary}`,
  ru: ({ id, summary }) => `${id} ${summary}`,
});

// The selection and its toolbar (step 2.2 §8, QRY-083, QRY-084).
export const selectRow = msg<{ id: string }>({
  en: ({ id }) => `Select ${id}`,
  ru: ({ id }) => `Выбрать ${id}`,
});
export const selectPage = msg({
  en: 'Select all issues on the page',
  ru: 'Выбрать все задачи на странице',
});
export const selectedCount = msg<{ count: number }>({
  en: ({ count }) => plural('en', count, { one: '# issue selected', other: '# issues selected' }),
  ru: ({ count }) =>
    plural('ru', count, {
      one: 'выбрана # задача',
      few: 'выбрано # задачи',
      many: 'выбрано # задач',
      other: 'выбрано # задачи',
    }),
});
export const allMatchingChosen = msg<{ count: number }>({
  en: ({ count }) =>
    plural('en', count, { one: 'all # issue of the query', other: 'all # issues of the query' }),
  ru: ({ count }) =>
    plural('ru', count, {
      one: 'все # задача по запросу',
      few: 'все # задачи по запросу',
      many: 'все # задач по запросу',
      other: 'все # задачи по запросу',
    }),
});
export const selectAllMatching = msg<{ count: number }>({
  en: ({ count }) =>
    plural('en', count, {
      one: 'Select all # issue of the query',
      other: 'Select all # issues of the query',
    }),
  ru: ({ count }) =>
    plural('ru', count, {
      one: 'Выбрать все # задачи по запросу',
      few: 'Выбрать все # задачи по запросу',
      many: 'Выбрать все # задач по запросу',
      other: 'Выбрать все # задачи по запросу',
    }),
});
export const clearSelection = msg({ en: 'Clear selection', ru: 'Снять выделение' });
export const toolbar = msg({ en: 'Selected issues', ru: 'Выбранные задачи' });
export const deleteSelection = msg({ en: 'Delete', ru: 'Удалить' });

// The toolbar's pickers (step 2.2 §8.3, QRY-084): each choice becomes a command line the
// dialog previews before it applies (D-064).
export const addLinkSelection = msg({ en: 'Add link', ru: 'Добавить связь' });
export const tagSelection = msg({ en: 'Tag', ru: 'Тег' });
export const assigneeSelection = msg({ en: 'Assignee', ru: 'Исполнитель' });

// The toolbar's export (step 2.2 §9, QRY-084): two buttons, one per format, and what the toolbar
// says when the file holds less than everything asked — the cap, or issues the viewer may not see.
export const exportCsv = msg({ en: 'Export CSV', ru: 'Экспорт CSV' });
export const exportXlsx = msg({ en: 'Export XLSX', ru: 'Экспорт XLSX' });
export const exportTruncated = msg<{ exported: number; total: number }>({
  en: ({ exported, total }) =>
    `The file holds ${String(exported)} of ${String(total)} issues — the export limit`,
  ru: ({ exported, total }) =>
    `В файле ${String(exported)} из ${String(total)} задач — лимит экспорта`,
});
export const tagPickerTitle = msg({
  en: 'Add a tag to the chosen issues',
  ru: 'Добавить тег выбранным задачам',
});
export const assigneePickerTitle = msg({ en: 'Choose an assignee', ru: 'Выбрать исполнителя' });
export const linkPickerTitle = msg({
  en: 'Link the chosen issues',
  ru: 'Связать выбранные задачи',
});
export const people = msg({ en: 'People', ru: 'Люди' });
export const findPerson = msg({ en: 'Find a person', ru: 'Найти человека' });
/** The tag command's word, the one the person would type (§8.3). */
export const tagVerb = msg({ en: 'tag', ru: 'тег' });
/** The phone's list settings entry that turns selection mode on (§8.1). */
export const chooseRows = msg({ en: 'Select', ru: 'Выбрать' });
export const stopChoosing = msg({ en: 'Stop selecting', ru: 'Завершить выбор' });
