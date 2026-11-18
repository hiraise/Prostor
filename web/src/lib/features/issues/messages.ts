// Texts of the issue page and of issues wherever they are named (ISS, FLD, UI-005). Voice and
// glossary: docs/design.md §12.
import { msg, plural } from '#lib/i18n/message.ts';

export const issues = msg({ en: 'Issues', ru: 'Задачи' });
export const breadcrumbs = msg({ en: 'Breadcrumbs', ru: 'Путь' });
export const backToIssues = msg<{ key: string }>({
  en: ({ key }) => `Issues ${key}`,
  ru: ({ key }) => `Задачи ${key}`,
});
export const resolved = msg({ en: 'resolved', ru: 'решена' });
export const draft = msg({ en: 'Draft', ru: 'Черновик' });
export const copyId = msg({ en: 'Copy ID', ru: 'Скопировать ID' });
export const idCopied = msg<{ id: string }>({
  en: ({ id }) => `${id} copied`,
  ru: ({ id }) => `${id} скопирован`,
});
export const noSummary = msg({ en: 'No summary', ru: 'Без названия' });

// The metadata strip (ISS-016).
export const details = msg({ en: 'About the issue', ru: 'Сведения о задаче' });
export const reportedBy = msg({ en: 'Reported by', ru: 'Создал' });
export const updatedBy = msg({ en: 'Updated by', ru: 'Изменил' });
export const visibleToAll = msg({
  en: 'Visible to everyone who sees the project',
  ru: 'Видна всем, кто видит проект',
});
export const visibility = msg({ en: 'Visibility', ru: 'Видимость' });
export const restricted = msg({ en: 'Restricted', ru: 'Ограничена' });
export const restrictedTo = msg<{ names: string }>({
  en: ({ names }) => `Visible to ${names} and the reporter`,
  ru: ({ names }) => `Видят ${names} и автор`,
});
export const restrictedHidden = msg({
  en: 'Visible to a limited audience',
  ru: 'Видна ограниченному кругу',
});
export const votes = msg<{ count: number }>({
  en: ({ count }) => plural('en', count, { one: '# vote', other: '# votes' }),
  ru: ({ count }) =>
    plural('ru', count, {
      one: '# голос',
      few: '# голоса',
      many: '# голосов',
      other: '# голоса',
    }),
});
export const watchers = msg<{ count: number }>({
  en: ({ count }) => plural('en', count, { one: '# watcher', other: '# watchers' }),
  ru: ({ count }) =>
    plural('ru', count, {
      one: '# наблюдатель',
      few: '# наблюдателя',
      many: '# наблюдателей',
      other: '# наблюдателя',
    }),
});

// Content.
export const description = msg({ en: 'Description', ru: 'Описание' });
export const noDescription = msg({ en: 'No description.', ru: 'Описания нет.' });

// The field panel (ISS-020, FLD-057, FLD-062).
export const fields = msg({ en: 'Fields', ru: 'Поля' });
export const project = msg({ en: 'Project', ru: 'Проект' });
export const projectSettings = msg<{ name: string }>({
  en: ({ name }) => `Settings of ${name}`,
  ru: ({ name }) => `Настройки проекта ${name}`,
});
export const privateField = msg({ en: 'Hidden field', ru: 'Скрытое поле' });
export const requiredField = msg({ en: 'Required field', ru: 'Обязательное поле' });
export const collapsePanel = msg({ en: 'Collapse the field panel', ru: 'Свернуть панель полей' });
export const expandPanel = msg({ en: 'Expand the field panel', ru: 'Развернуть панель полей' });
export const panelWidth = msg({ en: 'Width of the field panel', ru: 'Ширина панели полей' });

// Field editors (ISS-019, FLD-001…FLD-014, FLD-043, FLD-047, FLD-057).
export const editField = msg<{ name: string; value: string }>({
  en: ({ name, value }) => `${name}: ${value}. Change`,
  ru: ({ name, value }) => `${name}: ${value}. Изменить`,
});
export const fieldEditor = msg<{ name: string }>({
  en: ({ name }) => `Change ${name}`,
  ru: ({ name }) => `Изменить: ${name}`,
});
export const fieldOptions = msg<{ name: string }>({
  en: ({ name }) => `Values of ${name}`,
  ru: ({ name }) => `Значения поля ${name}`,
});
export const save = msg({ en: 'Save', ru: 'Сохранить' });
export const saved = msg<{ name: string }>({
  en: ({ name }) => `Saved: ${name}`,
  ru: ({ name }) => `Сохранено: ${name}`,
});
export const me = msg({ en: 'me', ru: 'я' });
export const archived = msg({ en: 'archived', ru: 'в архиве' });
export const released = msg({ en: 'released', ru: 'выпущена' });
export const releaseOn = msg({ en: 'release', ru: 'выпуск' });
export const addValue = msg<{ name: string }>({
  en: ({ name }) => `Add “${name}”`,
  ru: ({ name }) => `Добавить «${name}»`,
});
export const valueAdded = msg<{ name: string }>({
  en: ({ name }) => `“${name}” added`,
  ru: ({ name }) => `«${name}» добавлено`,
});
export const pickDate = msg({ en: 'Pick on the calendar', ru: 'Выбрать в календаре' });
export const exampleHint = msg<{ example: string }>({
  en: ({ example }) => `For example, ${example}`,
  ru: ({ example }) => `Например, ${example}`,
});
export const durationExample = msg({ en: '1h 30m', ru: '1ч 30м' });
export const cannotBeEmpty = msg({
  en: 'This field cannot be empty.',
  ru: 'Это поле нельзя оставить пустым.',
});
export const typedProblems = {
  number: msg({ en: 'Type a number.', ru: 'Введите число.' }),
  fraction: msg({ en: 'Type a whole number.', ru: 'Введите целое число.' }),
  date: msg({ en: 'Type a date.', ru: 'Введите дату.' }),
  date_time: msg({ en: 'Type a date and time.', ru: 'Введите дату и время.' }),
  duration: msg({
    en: 'Type a period such as 1w 2d 3h 30m.',
    ru: 'Введите срок, например 1н 2д 3ч 30м.',
  }),
  range: msg({ en: 'The number is too large.', ru: 'Слишком большое число.' }),
};

// Edit mode and conflicts (ISS-018, ISS-051).
export const edit = msg({ en: 'Edit', ru: 'Изменить' });
export const saveVisibility = msg({ en: 'Save visibility', ru: 'Сохранить видимость' });
export const openIssuePage = msg({ en: 'Open the page', ru: 'Открыть страницей' });
export const editIssue = msg({ en: 'Edit the issue', ru: 'Правка задачи' });
export const summary = msg({ en: 'Summary', ru: 'Название' });
export const editHint = msg({
  en: 'Ctrl+Enter saves, Esc cancels.',
  ru: 'Ctrl+Enter — сохранить, Esc — отменить.',
});
export const summaryRequired = msg({
  en: 'An issue needs a summary.',
  ru: 'У задачи должно быть название.',
});
export const issueSaved = msg({ en: 'Issue saved', ru: 'Задача сохранена' });
export const discardTitle = msg({ en: 'Discard your edits?', ru: 'Отменить правки?' });
export const discardText = msg({
  en: 'What you changed will be lost.',
  ru: 'Всё, что вы изменили, пропадёт.',
});
export const discard = msg({ en: 'Discard edits', ru: 'Отменить правки' });
export const keepEditing = msg({ en: 'Keep editing', ru: 'Продолжить правку' });
export const conflictTitle = msg({
  en: 'Someone changed this issue',
  ru: 'Задачу уже изменили',
});
export const conflictText = msg({
  en: 'While you were editing, someone else saved the summary or the description. Here is how your text differs from theirs.',
  ru: 'Пока вы правили, кто-то другой сохранил название или описание. Ниже — чем ваш текст отличается от сохранённого.',
});
export const keepMine = msg({ en: 'Save my version', ru: 'Сохранить мой вариант' });
export const editFurther = msg({ en: 'Keep editing', ru: 'Править дальше' });
export const dropMine = msg({ en: 'Discard my edits', ru: 'Отменить мои правки' });
export const theirVersion = msg({
  en: 'Saved meanwhile — compared with your text',
  ru: 'Сохранено за это время — в сравнении с вашим текстом',
});
export const diffAdded = msg({ en: 'added:', ru: 'добавлено:' });
export const diffRemoved = msg({ en: 'removed:', ru: 'удалено:' });
export const noChanges = msg({ en: 'No differences.', ru: 'Различий нет.' });

// The audience picker (ISS-041…ISS-043).
export const audienceAll = msg({
  en: 'Everyone who sees the project',
  ru: 'Все, кто видит проект',
});
export const audienceChosen = msg({ en: 'Only the chosen', ru: 'Только выбранные' });
export const audienceOptions = msg({ en: 'Groups and people', ru: 'Группы и люди' });
export const findAudience = msg({
  en: 'Find a group or a person',
  ru: 'Найти группу или человека',
});
export const reporterIncluded = msg({
  en: 'reporter, always sees it',
  ru: 'автор, видит всегда',
});
export const audienceEmpty = msg({
  en: 'Choose at least one group or person.',
  ru: 'Выберите хотя бы одну группу или человека.',
});

// Tags (ISS-034, ISS-035).
export const tags = msg({ en: 'Tags', ru: 'Теги' });
export const addTag = msg({ en: 'Add tag', ru: 'Добавить тег' });
export const findTag = msg({ en: 'Find a tag', ru: 'Найти тег' });
export const createTag = msg<{ name: string }>({
  en: ({ name }) => `Create tag “${name}”`,
  ru: ({ name }) => `Создать тег «${name}»`,
});
export const removeTag = msg<{ name: string }>({
  en: ({ name }) => `Remove tag ${name}`,
  ru: ({ name }) => `Убрать тег ${name}`,
});
export const tagIssues = msg<{ name: string }>({
  en: ({ name }) => `Issues tagged ${name}`,
  ru: ({ name }) => `Задачи с тегом ${name}`,
});
export const noTags = msg({ en: 'No tags found.', ru: 'Теги не найдены.' });

// Star and vote (ISS-030, ISS-032).
export const star = msg({ en: 'Watch', ru: 'Наблюдать' });
export const vote = msg({ en: 'Vote', ru: 'Голосовать' });
export const ownIssueVote = msg({
  en: 'You cannot vote for your own issue.',
  ru: 'За свою задачу голосовать нельзя.',
});
export const cannotVote = msg({
  en: 'You may not vote in this project.',
  ru: 'В этом проекте вам нельзя голосовать.',
});

// Moving (ISS-020, ISS-025).
export const moveTitle = msg({ en: 'Move to another project', ru: 'Перенос в другой проект' });
export const changeProject = msg<{ name: string }>({
  en: ({ name }) => `Project: ${name}. Move to another`,
  ru: ({ name }) => `Проект: ${name}. Перенести в другой`,
});
export const targetProject = msg({ en: 'Projects', ru: 'Проекты' });
export const findProject = msg({ en: 'Find a project', ru: 'Найти проект' });
export const changeProjectHint = msg({ en: 'Choose another project', ru: 'Выбрать другой проект' });
export const noProjects = msg({
  en: 'No other project takes new issues from you.',
  ru: 'Других проектов, где вы можете создавать задачи, нет.',
});
export const moveChanges = msg<{ name: string }>({
  en: ({ name }) => `In ${name} these fields change:`,
  ru: ({ name }) => `В проекте ${name} изменятся поля:`,
});
export const moveNoChanges = msg({
  en: 'Every field keeps its value.',
  ru: 'Все поля сохранят свои значения.',
});
export const outcomeErased = msg({ en: 'will be cleared', ru: 'будет очищено' });
export const outcomeReplaced = msg({ en: 'will be replaced', ru: 'будет заменено' });
export const losesAccess = msg({
  en: 'You will not see the issue in that project.',
  ru: 'В том проекте вы не будете видеть эту задачу.',
});
export const move = msg({ en: 'Move', ru: 'Перенести' });
export const back = msg({ en: 'Back', ru: 'Назад' });
export const moved = msg<{ id: string }>({
  en: ({ id }) => `Moved, now ${id}`,
  ru: ({ id }) => `Перенесена, теперь ${id}`,
});
export const movedAway = msg({
  en: 'Moved. You no longer see the issue.',
  ru: 'Перенесена. Эта задача вам больше не видна.',
});
export const emptyValue = msg({ en: 'empty', ru: 'пусто' });

// Delete (ISS-028) and the actions menu (ISS-017, UI-006).
export const actions = msg({ en: 'Issue actions', ru: 'Действия с задачей' });
export const more = msg({ en: 'More', ru: 'Ещё' });
export const copyIdAndSummary = msg({
  en: 'Copy ID and summary',
  ru: 'Скопировать ID и название',
});
export const deleteIssue = msg({ en: 'Delete', ru: 'Удалить' });
export const deleteTitle = msg<{ id: string }>({
  en: ({ id }) => `Delete ${id}?`,
  ru: ({ id }) => `Удалить ${id}?`,
});
export const deleteText = msg<{ summary: string }>({
  en: ({ summary }) =>
    `“${summary}” is deleted with its comments, files and links. This cannot be undone.`,
  ru: ({ summary }) =>
    `«${summary}» удаляется вместе с комментариями, файлами и связями. Это нельзя отменить.`,
});
export const deleteConfirm = msg({ en: 'Delete issue', ru: 'Удалить задачу' });
export const deleted = msg<{ id: string }>({
  en: ({ id }) => `${id} deleted`,
  ru: ({ id }) => `${id} удалена`,
});

// Cloning (AUT-066).
export const clone = msg({ en: 'Clone', ru: 'Клонировать' });
export const cloneAsDraft = msg({ en: 'Clone as draft', ru: 'Клонировать как черновик' });
/** The row's menu in the list; `id` is the issue's readable ID. */
export const rowActions = msg<{ id: string }>({
  en: ({ id }) => `Actions for ${id}`,
  ru: ({ id }) => `Действия с ${id}`,
});

// The create menu, the new-issue form and drafts (ISS-005…ISS-011, UI-002).
export const create = msg({ en: 'New', ru: 'Создать' });
export const createIssueItem = msg({ en: 'Issue', ru: 'Задачу' });
export const newIssue = msg({ en: 'New issue', ru: 'Новая задача' });
export const newIssueForm = msg({ en: 'New issue form', ru: 'Форма новой задачи' });
export const openFullPage = msg({ en: 'Open as a page', ru: 'Открыть на всю страницу' });
export const chooseProject = msg({ en: 'Project of the issue', ru: 'Проект задачи' });
export const noCreatableProjects = msg({
  en: 'There is no project where you may create issues.',
  ru: 'Нет проектов, в которых вы можете создавать задачи.',
});
export const movedDropped = msg<{ names: string }>({
  en: ({ names }) => `The new project has no place for: ${names}. These values were cleared.`,
  ru: ({ names }) => `В новом проекте нет места для: ${names}. Эти значения очищены.`,
});
export const draftSaving = msg({ en: 'Saving the draft…', ru: 'Сохраняем черновик…' });
export const draftSaved = msg({ en: 'Draft saved', ru: 'Черновик сохранён' });
export const draftNotSaved = msg({
  en: 'The draft is not saved yet',
  ru: 'Черновик пока не сохранён',
});
export const open = msg({ en: 'Open', ru: 'Открыть' });
export const createIssue = msg({ en: 'Create', ru: 'Создать' });
export const createVariants = msg({ en: 'Other ways to create', ru: 'Другие варианты создания' });
export const createAndNew = msg({
  en: 'Create and start another',
  ru: 'Создать и открыть новую',
});
export const createAndNewSameText = msg({
  en: 'Create and start another with the same text',
  ru: 'Создать и открыть новую с тем же текстом',
});
export const createHint = msg({
  en: 'Ctrl+Enter or ⌘+Enter creates the issue.',
  ru: 'Ctrl+Enter или ⌘+Enter создаёт задачу.',
});
export const discardDraft = msg({ en: 'Cancel and delete', ru: 'Отменить и удалить' });
export const issueCreated = msg<{ id: string }>({
  en: ({ id }) => `Issue ${id} created`,
  ru: ({ id }) => `Задача ${id} создана`,
});
export const requiredMissing = msg({
  en: 'Fill in the required fields first.',
  ru: 'Сначала заполните обязательные поля.',
});
export const fillRequired = msg({ en: 'Fill this field in', ru: 'Заполните это поле' });
export const starForMe = msg({ en: 'Watch it', ru: 'Следить за задачей' });
export const drafts = msg({ en: 'Drafts', ru: 'Черновики' });
export const draftsMenu = msg({ en: 'Your drafts', ru: 'Ваши черновики' });
export const draftItem = msg<{ summary: string; time: string }>({
  en: ({ summary, time }) => `${summary} · ${time}`,
  ru: ({ summary, time }) => `${summary} · ${time}`,
});
export const noDrafts = msg({ en: 'No other drafts', ru: 'Других черновиков нет' });
export const deleteAllDrafts = msg({ en: 'Delete all drafts', ru: 'Удалить все черновики' });
export const deleteAllTitle = msg({ en: 'Delete all drafts?', ru: 'Удалить все черновики?' });
export const deleteAllText = msg({
  en: 'Every unpublished issue of yours is deleted with its files, this one included. This cannot be undone.',
  ru: 'Все ваши неопубликованные задачи удалятся вместе с файлами, включая эту. Отменить это нельзя.',
});
export const draftsDeleted = msg({ en: 'Drafts deleted', ru: 'Черновики удалены' });

// The activity stream (HIST-001…HIST-011).
export const activity = msg({ en: 'Activity', ru: 'Активность' });
export const activityShown = msg({ en: 'Shown in the activity', ru: 'Что показывать' });
export const activityAll = msg({ en: 'All', ru: 'Все' });
export const activityComments = msg({ en: 'Comments', ru: 'Комментарии' });
export const activityChanges = msg({ en: 'Changes', ru: 'Изменения' });
export const orderOldest = msg({ en: 'Oldest first', ru: 'Сначала старые' });
export const orderNewest = msg({ en: 'Newest first', ru: 'Сначала новые' });
export const showOlder = msg({ en: 'Show earlier', ru: 'Показать более ранние' });
export const showNewer = msg({ en: 'Show newer', ru: 'Показать более новые' });
export const activityEmpty = msg({
  en: 'No comments or changes yet.',
  ru: 'Комментариев и изменений пока нет.',
});
export const commentBy = msg<{ name: string }>({
  en: ({ name }) => `Comment by ${name}`,
  ru: ({ name }) => `Комментарий: ${name}`,
});
export const changeBy = msg<{ name: string }>({
  en: ({ name }) => `Change by ${name}`,
  ru: ({ name }) => `Изменение: ${name}`,
});
export const commentDeleted = msg({ en: 'Comment deleted', ru: 'Комментарий удалён' });
export const showWhole = msg({ en: 'Show in full', ru: 'Показать полностью' });
export const showLess = msg({ en: 'Collapse', ru: 'Свернуть' });
export const changesWithComment = msg({
  en: 'Changes with the comment',
  ru: 'Изменения вместе с комментарием',
});
export const silentChange = msg({ en: 'without notifications', ru: 'без уведомлений' });
/** The origin badge of a record a rule made (AUT-023, §3.12): the rule did it, the person caused it. */
export const ruleBadge = msg({ en: 'rule', ru: 'правило' });
export const issueCreatedChange = msg({ en: 'Created the issue', ru: 'Создал задачу' });
/** AUT-066: the creation record of a clone names the original it was made from. */
export const issueClonedFrom = msg({
  en: 'Created the issue by cloning',
  ru: 'Создал задачу клонированием',
});
export const summaryChanged = msg({ en: 'Summary changed', ru: 'Название изменено' });
export const descriptionChanged = msg({ en: 'Description changed', ru: 'Описание изменено' });
export const textChanged = msg<{ field: string }>({
  en: ({ field }) => `${field} changed`,
  ru: ({ field }) => `Поле «${field}» изменено`,
});
export const showDiff = msg({ en: 'Show differences', ru: 'Показать различия' });
export const hideDiff = msg({ en: 'Hide differences', ru: 'Скрыть различия' });
export const resolvedChange = msg({ en: 'Resolved the issue', ru: 'Решил задачу' });
export const reopenedChange = msg({ en: 'Reopened the issue', ru: 'Открыл задачу снова' });
export const deletedChange = msg({ en: 'Deleted the issue', ru: 'Удалил задачу' });
export const filesLabel = msg({ en: 'Files', ru: 'Файлы' });
export const linksLabel = msg({ en: 'Links', ru: 'Связи' });
export const boardsLabel = msg({ en: 'Boards', ru: 'Доски' });
export const boardSprint = msg<{ board: string; sprint: string }>({
  en: ({ board, sprint }) => `${board} · ${sprint}`,
  ru: ({ board, sprint }) => `${board} · ${sprint}`,
});
export const valueGone = msg({ en: 'deleted', ru: 'удалено' });
export const before = msg({ en: 'Before', ru: 'Было' });
export const after = msg({ en: 'After', ru: 'Стало' });

// Comments (CNT-001…CNT-007, CNT-018).
export const comment = msg({ en: 'Comment', ru: 'Комментарий' });
export const newComment = msg({ en: 'New comment', ru: 'Новый комментарий' });
export const writeComment = msg({ en: 'Write a comment…', ru: 'Написать комментарий…' });
export const discardCommentTitle = msg({ en: 'Discard the draft?', ru: 'Сбросить черновик?' });
export const discardCommentText = msg({
  en: 'The text and files of the comment will be lost.',
  ru: 'Текст и файлы комментария будут потеряны.',
});
export const discardComment = msg({ en: 'Discard', ru: 'Сбросить' });
export const keepComment = msg({ en: 'Keep writing', ru: 'Продолжить' });
export const send = msg({ en: 'Send', ru: 'Отправить' });
export const commentHint = msg({
  en: 'Ctrl+Enter sends.',
  ru: 'Ctrl+Enter — отправить.',
});
export const commentEmpty = msg({
  en: 'Write something or add a file.',
  ru: 'Напишите текст или добавьте файл.',
});
export const commentPosted = msg({ en: 'Comment posted', ru: 'Комментарий отправлен' });
export const commentSaved = msg({ en: 'Comment saved', ru: 'Комментарий сохранён' });
export const commentActions = msg({ en: 'Comment actions', ru: 'Действия с комментарием' });
export const editComment = msg({ en: 'Edit the comment', ru: 'Правка комментария' });
export const editCommentHint = msg({
  en: 'Ctrl+Enter saves, Esc cancels. F2 on a comment edits it.',
  ru: 'Ctrl+Enter — сохранить, Esc — отменить. F2 на комментарии — изменить его.',
});
export const deleteComment = msg({ en: 'Delete', ru: 'Удалить' });
export const commentRemoved = msg({ en: 'Comment deleted', ru: 'Комментарий удалён' });
export const restoreComment = msg({ en: 'Restore', ru: 'Восстановить' });
export const commentRestored = msg({ en: 'Comment restored', ru: 'Комментарий восстановлен' });
export const commentEdited = msg({ en: 'edited', ru: 'изменён' });
export const commentEditedBy = msg<{ name: string }>({
  en: ({ name }) => `Edited by ${name}`,
  ru: ({ name }) => `Изменил: ${name}`,
});
export const commentConflict = msg({
  en: 'While you were editing, someone else saved this comment. Its text now:',
  ru: 'Пока вы правили, кто-то другой сохранил этот комментарий. Сейчас в нём так:',
});
export const cancel = msg({ en: 'Cancel', ru: 'Отменить' });
export const commentVisibility = msg({ en: 'Visibility', ru: 'Видимость' });
export const audienceIssue = msg({
  en: 'Everyone who sees the issue',
  ru: 'Все, кто видит задачу',
});
export const commentFiles = msg({ en: 'Files of the comment', ru: 'Файлы комментария' });
export const filesAdded = msg<{ count: number }>({
  en: ({ count }) => plural('en', count, { one: 'File added', other: 'Files added' }),
  ru: ({ count }) =>
    plural('ru', count, {
      one: 'Файл добавлен',
      few: 'Файлы добавлены',
      many: 'Файлы добавлены',
      other: 'Файлы добавлены',
    }),
});
export const dropOnComment = msg({
  en: 'Drop to add to the comment',
  ru: 'Отпустите, чтобы добавить к комментарию',
});

// Attachments (CNT-017, CNT-020, CNT-022, CNT-024).
export const attachments = msg({ en: 'Files', ru: 'Файлы' });
export const attachmentCount = msg<{ count: number }>({
  en: ({ count }) => plural('en', count, { one: '# file', other: '# files' }),
  ru: ({ count }) =>
    plural('ru', count, { one: '# файл', few: '# файла', many: '# файлов', other: '# файла' }),
});
export const showAttachments = msg({ en: 'Show files', ru: 'Показать файлы' });
export const hideAttachments = msg({ en: 'Hide files', ru: 'Скрыть файлы' });
export const attachFiles = msg({ en: 'Attach files', ru: 'Прикрепить файлы' });
export const moreFiles = msg({ en: 'Show more files', ru: 'Показать ещё файлы' });
export const dropToAttach = msg({ en: 'Drop to attach', ru: 'Отпустите, чтобы прикрепить' });
export const attachingFiles = msg({ en: 'Files on their way', ru: 'Загружаемые файлы' });
export const attaching = msg<{ name: string }>({
  en: ({ name }) => `Uploading ${name}`,
  ru: ({ name }) => `Загрузка: ${name}`,
});
export const attachFailed = msg<{ name: string; reason: string }>({
  en: ({ name, reason }) => `${name} was not attached: ${reason}`,
  ru: ({ name, reason }) => `${name} не прикреплён: ${reason}`,
});
export const dismiss = msg({ en: 'Dismiss', ru: 'Убрать' });
export const filesAttached = msg<{ count: number }>({
  en: ({ count }) => plural('en', count, { one: 'File attached', other: '# files attached' }),
  ru: ({ count }) =>
    plural('ru', count, {
      one: 'Файл прикреплён',
      few: 'Прикреплено # файла',
      many: 'Прикреплено # файлов',
      other: 'Прикреплено # файла',
    }),
});
export const fileActions = msg<{ name: string }>({
  en: ({ name }) => `Actions with ${name}`,
  ru: ({ name }) => `Действия с файлом ${name}`,
});
export const openFile = msg<{ name: string }>({
  en: ({ name }) => `Open ${name}`,
  ru: ({ name }) => `Открыть ${name}`,
});
export const download = msg({ en: 'Download', ru: 'Скачать' });
export const fileDetails = msg({ en: 'Details', ru: 'Сведения' });
export const rename = msg({ en: 'Rename', ru: 'Переименовать' });
export const fileVisibility = msg({ en: 'Visibility', ru: 'Видимость' });
export const deleteFile = msg({ en: 'Delete', ru: 'Удалить' });
export const fileName = msg({ en: 'Name', ru: 'Имя' });
export const fileSizeLabel = msg({ en: 'Size', ru: 'Размер' });
export const fileType = msg({ en: 'Type', ru: 'Тип' });
export const fileDimensions = msg({ en: 'Dimensions', ru: 'Размеры' });
export const uploadedBy = msg({ en: 'Uploaded by', ru: 'Загрузил' });
export const uploadedAt = msg({ en: 'Uploaded', ru: 'Загружен' });
export const fromComment = msg({ en: 'From a comment', ru: 'Из комментария' });
export const renameTitle = msg({ en: 'Rename the file', ru: 'Переименование файла' });
export const fileNameEmpty = msg({ en: 'Enter a name.', ru: 'Введите имя.' });
export const fileRenamed = msg({ en: 'File renamed', ru: 'Файл переименован' });
export const fileVisibilityTitle = msg({ en: 'Who sees the file', ru: 'Кто видит файл' });
export const fileVisibilitySaved = msg({ en: 'Visibility saved', ru: 'Видимость сохранена' });
export const deleteFileTitle = msg<{ name: string }>({
  en: ({ name }) => `Delete ${name}?`,
  ru: ({ name }) => `Удалить ${name}?`,
});
export const deleteFileText = msg({
  en: 'The file is deleted for good; texts that name it will show it as missing.',
  ru: 'Файл удалится насовсем; в текстах, где он упомянут, будет пометка, что его нет.',
});
export const fileDeleted = msg({ en: 'File deleted', ru: 'Файл удалён' });
export const previousFile = msg({ en: 'Previous', ru: 'Предыдущий' });
export const nextFile = msg({ en: 'Next', ru: 'Следующий' });
export const fileOf = msg<{ index: number; count: number }>({
  en: ({ index, count }) => `${String(index)} of ${String(count)}`,
  ru: ({ index, count }) => `${String(index)} из ${String(count)}`,
});

// Links and subtasks (LNK-005…LNK-015, ISS-009).
export const links = msg({ en: 'Links', ru: 'Связи' });
export const allLinks = msg({ en: 'All', ru: 'Все' });
export const linkTabs = msg({ en: 'Link names', ru: 'Виды связей' });
export const expandLinks = msg({ en: 'Expand', ru: 'Развернуть' });
export const collapseLinks = msg({ en: 'Collapse', ru: 'Свернуть' });
export const moreLinks = msg({ en: 'Show more', ru: 'Ещё' });
export const openAsList = msg({ en: 'Open as a list', ru: 'Открыть списком' });
export const watchIssue = msg<{ id: string }>({
  en: ({ id }) => `Watch ${id}`,
  ru: ({ id }) => `Наблюдать за ${id}`,
});
export const removeLink = msg<{ id: string }>({
  en: ({ id }) => `Remove the link to ${id}`,
  ru: ({ id }) => `Убрать связь с ${id}`,
});
export const linkRemoved = msg({ en: 'Link removed', ru: 'Связь убрана' });
export const linkAction = msg({ en: 'Link', ru: 'Связать' });
export const linkIssuesTitle = msg({ en: 'Link issues', ru: 'Связать задачи' });
export const linkName = msg({ en: 'Link', ru: 'Связь' });
export const findIssue = msg({ en: 'Find an issue', ru: 'Найти задачу' });
export const recentIssues = msg({ en: 'Recent issues', ru: 'Недавние задачи' });
export const foundIssues = msg({ en: 'Issues found', ru: 'Найденные задачи' });
export const linkChosen = msg<{ count: number }>({
  en: ({ count }) => (count > 1 ? `Link ${String(count)}` : 'Link'),
  ru: ({ count }) => (count > 1 ? `Связать ${String(count)}` : 'Связать'),
});
export const alreadyLinked = msg<{ ids: string }>({
  en: ({ ids }) => `Already linked this way: ${ids}`,
  ru: ({ ids }) => `Уже связаны так: ${ids}`,
});
export const issuesLinked = msg<{ count: number }>({
  en: ({ count }) => plural('en', count, { one: 'Issue linked', other: '# issues linked' }),
  ru: ({ count }) =>
    plural('ru', count, {
      one: 'Задача связана',
      few: 'Связано # задачи',
      many: 'Связано # задач',
      other: 'Связано # задачи',
    }),
});
export const createAndLink = msg<{ summary: string }>({
  en: ({ summary }) => `Create issue “${summary}” and link`,
  ru: ({ summary }) => `Создать задачу «${summary}» и связать`,
});
export const createdAndLinked = msg<{ id: string }>({
  en: ({ id }) => `${id} created and linked`,
  ru: ({ id }) => `Задача ${id} создана и связана`,
});
export const openFullForm = msg({ en: 'Open the full form', ru: 'Открыть полную форму' });
export const subtaskOf = msg({ en: 'Subtask of', ru: 'Подзадача для' });
export const subtaskProgress = msg<{ resolved: number; total: number }>({
  en: ({ resolved, total }) => `${String(resolved)} of ${String(total)} resolved`,
  ru: ({ resolved, total }) => `решено ${String(resolved)} из ${String(total)}`,
});
export const newSubtask = msg({ en: 'New subtask', ru: 'Новая подзадача' });
export const newLinkedIssue = msg({ en: 'New linked issue…', ru: 'Новая связанная задача…' });
export const newLinkedTitle = msg({ en: 'New linked issue', ru: 'Новая связанная задача' });
export const commandsMenu = msg({ en: 'Commands…', ru: 'Команды…' });
export const repeatMenu = msg({ en: 'Repeat last command', ru: 'Повторить последнюю команду' });
export const markDuplicate = msg({ en: 'Mark as duplicate…', ru: 'Отметить как дубликат…' });
export const markDuplicateTitle = msg({ en: 'Mark as duplicate', ru: 'Отметить как дубликат' });
export const addLink = msg({ en: 'Add a link', ru: 'Добавить связь' });
export const noLinks = msg({ en: 'No links yet.', ru: 'Связей пока нет.' });
export const linkTypesFailed = msg({
  en: 'Link names did not load.',
  ru: 'Не удалось загрузить виды связей.',
});

// Live updates of the open issue (ISS-048).
export const newEntries = msg<{ count: number }>({
  en: ({ count }) => `New entries: ${String(count)}`,
  ru: ({ count }) => `Новые записи: ${String(count)}`,
});
export const showNew = msg({ en: 'Show', ru: 'Показать' });
export const changedWhileEditing = msg({
  en: 'The issue was changed while you were editing it.',
  ru: 'Задачу изменили, пока вы её правили.',
});
export const showDifferences = msg({ en: 'Show differences', ru: 'Показать различия' });
export const mayBeOutdated = msg({
  en: 'The connection was lost for a while: the page may be out of date.',
  ru: 'Связь с сервером прерывалась: страница могла устареть.',
});
export const refreshPage = msg({ en: 'Refresh', ru: 'Обновить' });

// The keyboard of the issue view (step 2.2 §7, ISS-047).
export const assignedToMe = msg({ en: 'Assigned to me', ru: 'Назначено на меня' });
