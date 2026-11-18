// Texts of boards (BRD). Voice and glossary: docs/design.md §12 — доска, колонка, карточка,
// спринт, «Своя» preset.
import { msg, plural } from '#lib/i18n/message.ts';

export const boards = msg({ en: 'Boards', ru: 'Доски' });
export const boardList = msg({ en: 'Boards', ru: 'Доски' });
export const createBoard = msg({ en: 'Create board', ru: 'Создать доску' });
export const newBoard = msg({ en: 'New board', ru: 'Новая доска' });
export const createBoardItem = msg({ en: 'Board', ru: 'Доску' });
export const findBoards = msg({ en: 'Find boards', ru: 'Найти доски' });
export const findBoardsHint = msg({ en: 'Part of the name.', ru: 'Часть названия.' });
export const showMore = msg({ en: 'Show more', ru: 'Показать ещё' });

export const noBoards = msg({ en: 'No boards yet', ru: 'Досок пока нет' });
export const noBoardsCreate = msg({
  en: 'A board shows issues as cards in columns: by state, by sprint, the way your team works.',
  ru: 'На доске задачи — карточки в колонках: по состоянию, по спринтам, как работает команда.',
});
export const noBoardsShared = msg({
  en: 'Nobody has shared a board with you yet.',
  ru: 'С вами пока не поделились ни одной доской.',
});
export const noProjectBoards = msg({
  en: 'No boards include this project yet',
  ru: 'В досках этот проект пока не участвует',
});
export const noBoardsFound = msg({ en: 'No boards found', ru: 'Доски не найдены' });
export const noBoardsFoundText = msg({
  en: 'Try another part of the name.',
  ru: 'Попробуйте другую часть названия.',
});

export const ownedBy = msg<{ name: string }>({
  en: ({ name }) => `Owner: ${name}`,
  ru: ({ name }) => `Владелец: ${name}`,
});
export const moreProjects = msg<{ count: string }>({
  en: ({ count }) => `+${count}`,
  ru: ({ count }) => `+${count}`,
});
export const starBoard = msg<{ name: string }>({
  en: ({ name }) => `Add “${name}” to favorites`,
  ru: ({ name }) => `Добавить «${name}» в избранное`,
});
export const unstarBoard = msg<{ name: string }>({
  en: ({ name }) => `Remove “${name}” from favorites`,
  ru: ({ name }) => `Убрать «${name}» из избранного`,
});
export const starFailed = msg({
  en: 'Could not change favorites. Try again.',
  ru: 'Не удалось изменить избранное. Попробуйте ещё раз.',
});

export const watchCard = msg({ en: 'Watch', ru: 'Наблюдать' });
export const unwatchCard = msg({ en: 'Stop watching', ru: 'Не наблюдать' });
export const watchFailed = msg({
  en: 'Could not change watching',
  ru: 'Не удалось изменить наблюдение',
});

// The switcher.
export const switchBoard = msg<{ name: string }>({
  en: ({ name }) => `Board “${name}”: switch board`,
  ru: ({ name }) => `Доска «${name}»: сменить доску`,
});
export const allBoards = msg({ en: 'All boards', ru: 'Все доски' });
export const filterBoards = msg({ en: 'Filter boards', ru: 'Отобрать доски' });
export const currentBoard = msg({ en: 'current', ru: 'текущая' });

// Creating.
export const presetLegend = msg({ en: 'Kind of board', ru: 'Вид доски' });
export const presets = {
  scrum: msg({ en: 'Scrum', ru: 'Скрам' }),
  kanban: msg({ en: 'Kanban', ru: 'Канбан' }),
  custom: msg({ en: 'Custom', ru: 'Своя' }),
};
export const presetHints = {
  scrum: msg({
    en: 'Work in sprints: plan from the backlog, finish, carry the rest over.',
    ru: 'Работа спринтами: план из бэклога, завершение, перенос остатка.',
  }),
  kanban: msg({
    en: 'A continuous flow without sprints; column limits keep work in progress small.',
    ru: 'Непрерывный поток без спринтов; лимиты колонок сдерживают незавершённую работу.',
  }),
  custom: msg({
    en: 'An empty board: choose the columns and the rules yourself.',
    ru: 'Пустая доска: колонки и правила выберете сами.',
  }),
};
export const name = msg({ en: 'Name', ru: 'Название' });
export const nameRequired = msg({ en: 'Enter a name.', ru: 'Введите название.' });
export const nameTaken = msg({
  en: 'You already see a board with this name.',
  ru: 'Доска с таким названием у вас уже есть.',
});
export const projects = msg({ en: 'Projects', ru: 'Проекты' });
export const projectsRequired = msg({
  en: 'Choose at least one project.',
  ru: 'Выберите хотя бы один проект.',
});
export const allMyProjects = msg({ en: 'All my projects', ru: 'Все мои проекты' });
export const whoViews = msg({ en: 'Who sees the board', ru: 'Кто видит доску' });
export const whoEdits = msg({ en: 'Who changes its settings', ru: 'Кто меняет настройки' });
export const viewReaders = msg({
  en: 'Everyone who reads issues in all its projects',
  ru: 'Все, кто читает задачи во всех её проектах',
});
export const editUpdaters = msg({
  en: 'Everyone who updates issues in its projects',
  ru: 'Все, кто меняет задачи в её проектах',
});
export const nobodyChosen = msg({
  en: 'Choose people or groups, or switch to «Only me».',
  ru: 'Выберите людей или группы либо переключитесь на «Только я».',
});
export const create = msg({ en: 'Create', ru: 'Создать' });
export const cancel = msg({ en: 'Cancel', ru: 'Отмена' });
export const boardCreated = msg<{ name: string }>({
  en: ({ name }) => `Board “${name}” created`,
  ru: ({ name }) => `Доска «${name}» создана`,
});

// The board screen (1.8a-5).
export const boardActions = msg<{ name: string }>({
  en: ({ name }) => `Actions of “${name}”`,
  ru: ({ name }) => `Действия с доской «${name}»`,
});
export const toolbar = msg({ en: 'Board tools', ru: 'Инструменты доски' });
export const columnsLabel = msg({ en: 'Columns', ru: 'Колонки' });
export const loadFailed = msg({
  en: 'Could not load the board.',
  ru: 'Не удалось загрузить доску.',
});
export const retry = msg({ en: 'Try again', ru: 'Повторить' });
export const noColumnField = msg({
  en: 'The board has no columns yet',
  ru: 'У доски пока нет колонок',
});
export const noColumnFieldText = msg({
  en: 'Choose the field whose values become the columns in the settings.',
  ru: 'Выберите в настройках поле, значения которого станут колонками.',
});
export const noColumnsReader = msg({
  en: 'Whoever changes its settings will choose them.',
  ru: 'Их выберет тот, кто меняет её настройки.',
});
export const openSettings = msg({ en: 'Open settings', ru: 'Открыть настройки' });

export const cardSize = msg({ en: 'Card size', ru: 'Размер карточек' });
export const cardSizes = [
  msg({ en: 'Small', ru: 'Маленькие' }),
  msg({ en: 'Medium', ru: 'Средние' }),
  msg({ en: 'Large', ru: 'Большие' }),
  msg({ en: 'Extra large', ru: 'Очень большие' }),
];
export const zoom = msg({ en: 'Show', ru: 'Показ' });
export const zooms = {
  column: msg({ en: 'Column', ru: 'Колонка' }),
  all: msg({ en: 'All', ru: 'Все' }),
};
export const columnStrip = msg({ en: 'Board columns', ru: 'Колонки доски' });
export const settings = msg({ en: 'Board settings', ru: 'Настройки доски' });
export const settingsLocked = msg({
  en: 'Only those who change its settings can open them',
  ru: 'Настройки открывает только тот, кто может их менять',
});
export const profileFailed = msg({
  en: 'Could not save how the board looks to you. Try again.',
  ru: 'Не удалось сохранить вид доски. Попробуйте ещё раз.',
});

// Progress (BRD-085).
export const progress = msg({ en: 'Progress by column', ru: 'Ход работы по колонкам' });
export const progressPart = msg<{ name: string; count: number }>({
  en: ({ name, count }) => `${name}: ${plural('en', count, { one: '# card', other: '# cards' })}`,
  ru: ({ name, count }) =>
    `${name}: ${plural('ru', count, { one: '# карточка', few: '# карточки', many: '# карточек', other: '# карточки' })}`,
});

// Columns (BRD-046, BRD-047).
export const cardCount = msg<{ count: number }>({
  en: ({ count }) => plural('en', count, { one: '# card', other: '# cards' }),
  ru: ({ count }) =>
    plural('ru', count, {
      one: '# карточка',
      few: '# карточки',
      many: '# карточек',
      other: '# карточки',
    }),
});
export const wipMax = msg<{ count: number }>({
  en: ({ count }) => `at most ${String(count)}`,
  ru: ({ count }) => `не больше ${String(count)}`,
});
export const wipMin = msg<{ count: number }>({
  en: ({ count }) => `at least ${String(count)}`,
  ru: ({ count }) => `не меньше ${String(count)}`,
});
export const wipOver = msg<{ count: number }>({
  en: ({ count }) => `Over the limit of ${String(count)}`,
  ru: ({ count }) => `Больше лимита ${String(count)}`,
});
export const wipUnder = msg<{ count: number }>({
  en: ({ count }) => `Under the minimum of ${String(count)}`,
  ru: ({ count }) => `Меньше минимума ${String(count)}`,
});
export const collapseColumn = msg<{ name: string }>({
  en: ({ name }) => `Collapse “${name}”`,
  ru: ({ name }) => `Свернуть «${name}»`,
});
export const expandColumn = msg<{ name: string }>({
  en: ({ name }) => `Expand “${name}”`,
  ru: ({ name }) => `Развернуть «${name}»`,
});
export const openAsList = msg<{ name: string }>({
  en: ({ name }) => `Open “${name}” as a list`,
  ru: ({ name }) => `Открыть «${name}» списком`,
});
export const cellCards = msg<{ name: string }>({
  en: ({ name }) => `Cards of “${name}”`,
  ru: ({ name }) => `Карточки колонки «${name}»`,
});
export const emptyCell = msg({ en: 'No cards', ru: 'Карточек нет' });
export const moreCards = msg<{ count: number }>({
  en: ({ count }) => `${String(count)} more`,
  ru: ({ count }) => `Ещё ${String(count)}`,
});
export const moreFailed = msg({
  en: 'Could not show the other cards. Try again.',
  ru: 'Не удалось показать остальные карточки. Попробуйте ещё раз.',
});

// Cards (BRD-058).
export const assignee = msg<{ name: string }>({
  en: ({ name }) => `Assignee: ${name}`,
  ru: ({ name }) => `Исполнитель: ${name}`,
});
export const tagsLabel = msg({ en: 'Tags', ru: 'Теги' });
export const files = msg<{ count: number }>({
  en: ({ count }) => plural('en', count, { one: '# file', other: '# files' }),
  ru: ({ count }) =>
    plural('ru', count, { one: '# файл', few: '# файла', many: '# файлов', other: '# файла' }),
});
export const filesLabel = msg({ en: 'Files', ru: 'Файлы' });
export const restricted = msg({
  en: 'Visible to some people only',
  ru: 'Видна не всем',
});
export const estimate = msg<{ value: string }>({
  en: ({ value }) => `Estimation: ${value}`,
  ru: ({ value }) => `Оценка: ${value}`,
});
export const editField = msg<{ name: string; id: string }>({
  en: ({ name, id }) => `${name} of ${id}`,
  ru: ({ name, id }) => `${name} задачи ${id}`,
});
export const fieldEditor = msg<{ name: string }>({
  en: ({ name }) => `Change ${name}`,
  ru: ({ name }) => `Изменить: ${name}`,
});
export const emptyField = msg<{ name: string }>({
  en: ({ name }) => `${name}: not set`,
  ru: ({ name }) => `${name}: не задано`,
});
export const fieldSaved = msg<{ name: string }>({
  en: ({ name }) => `${name} saved`,
  ru: ({ name }) => `${name}: сохранено`,
});
export const fieldFailed = msg<{ name: string }>({
  en: ({ name }) => `Could not change ${name}.`,
  ru: ({ name }) => `Не удалось изменить: ${name}.`,
});
export const removeTag = msg<{ name: string }>({
  en: ({ name }) => `Remove tag ${name}`,
  ru: ({ name }) => `Снять тег ${name}`,
});
export const tagRemoved = msg<{ name: string }>({
  en: ({ name }) => `Tag ${name} removed`,
  ru: ({ name }) => `Тег ${name} снят`,
});
export const tagFailed = msg<{ name: string }>({
  en: ({ name }) => `Could not remove tag ${name}.`,
  ru: ({ name }) => `Не удалось снять тег ${name}.`,
});
export const colorOf = msg<{ name: string }>({
  en: ({ name }) => `Color: ${name}`,
  ru: ({ name }) => `Цвет: ${name}`,
});
export const estimateSum = msg<{ value: string }>({
  en: ({ value }) => `Estimation in total: ${value}`,
  ru: ({ value }) => `Оценка всего: ${value}`,
});
export const yes = msg({ en: 'yes', ru: 'да' });

// The settings panel (BRD-086).
export const tabs = msg({ en: 'Sections of the settings', ru: 'Разделы настроек' });
export const generalTab = msg({ en: 'General', ru: 'Основное' });
export const columnsTab = msg({ en: 'Columns', ru: 'Колонки' });
export const save = msg({ en: 'Save', ru: 'Сохранить' });
export const saved = msg({ en: 'Saved', ru: 'Сохранено' });
export const settingsChanged = msg({
  en: 'The settings have changed meanwhile. Try again.',
  ru: 'Настройки изменились, повторите.',
});
export const owner = msg({ en: 'Owner', ru: 'Владелец' });
export const transfer = msg({ en: 'Give to someone else', ru: 'Передать другому' });
export const transferTitle = msg({ en: 'New owner of the board', ru: 'Новый владелец доски' });
export const transferPick = msg({ en: 'Person', ru: 'Человек' });
export const transferDo = msg({ en: 'Give', ru: 'Передать' });
export const transferRequired = msg({ en: 'Choose a person.', ru: 'Выберите человека.' });
export const transferred = msg<{ name: string }>({
  en: ({ name }) => `The board now belongs to ${name}`,
  ru: ({ name }) => `Доска передана: ${name}`,
});
export const sharing = msg({ en: 'Access', ru: 'Доступ' });
export const deleteSection = msg({ en: 'Deleting', ru: 'Удаление' });
export const deleteBoard = msg({ en: 'Delete board', ru: 'Удалить доску' });
export const deleteTitle = msg<{ name: string }>({
  en: ({ name }) => `Delete “${name}”?`,
  ru: ({ name }) => `Удалить доску «${name}»?`,
});
export const deleteImpact = msg<{ sprints: number; cards: number }>({
  en: ({ sprints, cards }) =>
    `${plural('en', sprints, { one: '# sprint', other: '# sprints' })} and ${plural('en', cards, { one: '# card', other: '# cards' })} go with it. The issues stay.`,
  ru: ({ sprints, cards }) =>
    `Вместе с ней удалятся ${plural('ru', sprints, { one: '# спринт', few: '# спринта', many: '# спринтов', other: '# спринта' })} и ${plural('ru', cards, { one: '# карточка', few: '# карточки', many: '# карточек', other: '# карточки' })}. Задачи останутся.`,
});
export const deleteForever = msg({
  en: 'For everyone, and it cannot be undone.',
  ru: 'Для всех и без возможности отменить.',
});
export const deleted = msg<{ name: string }>({
  en: ({ name }) => `Board “${name}” deleted`,
  ru: ({ name }) => `Доска «${name}» удалена`,
});

// Columns settings (BRD-042…BRD-046).
export const columnField = msg({ en: 'Field of the columns', ru: 'Поле колонок' });
export const columnFieldHint = msg({
  en: 'One value per issue: a state, a version, a list. Each value can become a column.',
  ru: 'Одно значение у задачи: состояние, версия, список. Каждое значение может стать колонкой.',
});
export const noField = msg({ en: 'Not chosen', ru: 'Не выбрано' });
export const changeFieldTitle = msg({ en: 'Change the field?', ru: 'Сменить поле колонок?' });
export const changeFieldText = msg<{ count: number }>({
  en: ({ count }) =>
    `${plural('en', count, { one: 'The # column goes', other: 'All # columns go' })}; the cards stay in their sprints.`,
  ru: ({ count }) =>
    `${plural('ru', count, { one: 'Удалится # колонка', few: 'Удалятся # колонки', many: 'Удалятся # колонок', other: 'Удалятся # колонки' })}; карточки останутся в своих спринтах.`,
});
export const changeField = msg({ en: 'Change', ru: 'Сменить' });
export const columnList = msg({ en: 'Columns in order', ru: 'Колонки по порядку' });
export const noColumns = msg({ en: 'No columns yet.', ru: 'Колонок пока нет.' });
export const columnActions = msg<{ name: string }>({
  en: ({ name }) => `Actions of “${name}”`,
  ru: ({ name }) => `Действия с колонкой «${name}»`,
});
export const moveUp = msg({ en: 'Move up', ru: 'Выше' });
export const moveDown = msg({ en: 'Move down', ru: 'Ниже' });
export const rename = msg({ en: 'Rename', ru: 'Переименовать' });
export const removeColumn = msg({ en: 'Remove column', ru: 'Убрать колонку' });
export const columnRemoved = msg<{ name: string }>({
  en: ({ name }) => `Column “${name}” removed`,
  ru: ({ name }) => `Колонка «${name}» убрана`,
});
export const minLabel = msg<{ name: string }>({
  en: ({ name }) => `Minimum of “${name}”`,
  ru: ({ name }) => `Минимум «${name}»`,
});
export const maxLabel = msg<{ name: string }>({
  en: ({ name }) => `Limit of “${name}”`,
  ru: ({ name }) => `Лимит «${name}»`,
});
export const minShort = msg({ en: 'min', ru: 'мин.' });
export const maxShort = msg({ en: 'max', ru: 'макс.' });
export const limitsInvalid = msg({
  en: 'Limits are whole numbers from 0; the minimum is not above the limit.',
  ru: 'Лимиты — целые числа от 0; минимум не больше лимита.',
});
export const addColumn = msg({ en: 'Add column', ru: 'Добавить колонку' });
export const existingValue = msg({ en: 'Value of the field', ru: 'Значение поля' });
export const allValuesUsed = msg({
  en: 'Every value is a column already.',
  ru: 'Все значения уже стали колонками.',
});
export const newValue = msg({ en: 'Or a new value', ru: 'Или новое значение' });
export const add = msg({ en: 'Add', ru: 'Добавить' });
export const renameTitle = msg<{ name: string }>({
  en: ({ name }) => `Rename “${name}”`,
  ru: ({ name }) => `Переименовать «${name}»`,
});
export const newName = msg({ en: 'New name', ru: 'Новое название' });
export const setUsedIn = msg<{ projects: string; hidden: number }>({
  en: ({ projects, hidden }) =>
    `The value changes in every project using its set: ${projects}${hidden > 0 ? ` and ${String(hidden)} more` : ''}.`,
  ru: ({ projects, hidden }) =>
    `Значение изменится во всех проектах с этим набором: ${projects}${hidden > 0 ? ` и ещё ${String(hidden)}` : ''}.`,
});
export const newValueTitle = msg<{ name: string }>({
  en: ({ name }) => `Add the value “${name}”?`,
  ru: ({ name }) => `Добавить значение «${name}»?`,
});
export const newValueUsedIn = msg<{ projects: string; hidden: number }>({
  en: ({ projects, hidden }) =>
    `It appears in every project using the set: ${projects}${hidden > 0 ? ` and ${String(hidden)} more` : ''}.`,
  ru: ({ projects, hidden }) =>
    `Оно появится во всех проектах с этим набором: ${projects}${hidden > 0 ? ` и ещё ${String(hidden)}` : ''}.`,
});
export const fieldsFailed = msg({
  en: 'Could not load the fields of the board projects.',
  ru: 'Не удалось загрузить поля проектов доски.',
});

// Moving cards (BRD-068, BRD-069, BRD-074; UI-032, UI-038).
export const cardActions = msg<{ id: string }>({
  en: ({ id }) => `Actions of ${id}`,
  ru: ({ id }) => `Действия с ${id}`,
});
export const moveToColumn = msg({ en: 'Move to column', ru: 'Переместить в колонку' });
export const moveInColumn = msg({ en: 'Move within the column', ru: 'Переместить в колонке' });
export const toStart = msg({ en: 'To the top', ru: 'В начало' });
export const toEnd = msg({ en: 'To the bottom', ru: 'В конец' });
export const afterCard = msg({ en: 'After…', ru: 'После…' });
export const afterTitle = msg<{ id: string }>({
  en: ({ id }) => `Move ${id} after`,
  ru: ({ id }) => `Переместить ${id} после`,
});
export const afterLabel = msg({ en: 'Card', ru: 'Карточка' });
export const moveAction = msg({ en: 'Move', ru: 'Переместить' });
export const removeFromSprint = msg({ en: 'Remove from sprint', ru: 'Убрать со спринта' });
export const removeFromBoard = msg({ en: 'Remove from board', ru: 'Убрать с доски' });
export const openIssue = msg({ en: 'Open', ru: 'Открыть' });
export const cardMoved = msg({ en: 'Moved', ru: 'Перемещено' });
export const cardMovedTo = msg<{ id: string; column: string }>({
  en: ({ id, column }) => `${id} moved to “${column}”`,
  ru: ({ id, column }) => `${id} перемещена в «${column}»`,
});
export const moveFailed = msg({
  en: 'Could not move the card',
  ru: 'Не удалось переместить карточку',
});
export const cardMoveFailed = msg<{ id: string }>({
  en: ({ id }) => `${id} is back in its place`,
  ru: ({ id }) => `${id} вернулась на место`,
});
export const cardRemovedSprint = msg({ en: 'Removed from the sprint', ru: 'Убрано со спринта' });
export const cardRemovedBoard = msg({ en: 'Removed from the board', ru: 'Убрано с доски' });
export const removeFailed = msg({
  en: 'Could not remove the card',
  ru: 'Не удалось убрать карточку',
});
export const restoreFailed = msg({
  en: 'Could not put the card back',
  ru: 'Не удалось вернуть карточку',
});

// Sprints (BRD-083, BRD-084, BRD-098…BRD-104).
export const sprintMenu = msg({ en: 'Sprint', ru: 'Спринт' });
export const sprintMenuOf = msg<{ name: string }>({
  en: ({ name }) => `Sprint: ${name}`,
  ru: ({ name }) => `Спринт: ${name}`,
});
export const noSprint = msg({ en: 'No current sprint', ru: 'Нет текущего спринта' });
export const findSprint = msg({ en: 'Find a sprint', ru: 'Найти спринт' });
export const noSprintsFound = msg({ en: 'No sprints found', ru: 'Спринты не найдены' });
export const archivedSprints = msg({ en: 'Archived', ru: 'В архиве' });
export const currentSprint = msg({ en: 'current', ru: 'текущий' });
export const editSprintOf = msg<{ name: string }>({
  en: ({ name }) => `Edit “${name}”`,
  ru: ({ name }) => `Изменить «${name}»`,
});
export const newSprint = msg({ en: 'New sprint', ru: 'Новый спринт' });
export const editSprint = msg({ en: 'Edit sprint', ru: 'Изменить спринт' });
export const sprintsFailed = msg({
  en: 'Could not load the archived sprints.',
  ru: 'Не удалось загрузить спринты из архива.',
});
export const sprintName = msg({ en: 'Name', ru: 'Название' });
export const sprintNameNeeded = msg({ en: 'Give the sprint a name.', ru: 'Назовите спринт.' });
export const sprintGoal = msg({ en: 'Goal', ru: 'Цель' });
export const sprintGoalHint = msg({
  en: 'Shown under the board for the whole sprint.',
  ru: 'Видна под доской весь спринт.',
});
export const sprintStarts = msg({ en: 'Starts', ru: 'Начало' });
export const sprintEnds = msg({ en: 'Ends', ru: 'Конец' });
export const sprintDatesHint = msg({
  en: 'Leave empty for a sprint without dates.',
  ru: 'Оставьте пустыми, если у спринта нет дат.',
});
export const sprintEndsBeforeStart = msg({
  en: 'The end cannot come before the start.',
  ru: 'Конец не может быть раньше начала.',
});
export const carryFrom = msg<{ name: string }>({
  en: ({ name }) => `Carry the unresolved issues of “${name}” over`,
  ru: ({ name }) => `Перенести незавершённые задачи из «${name}»`,
});
export const autoTarget = msg({
  en: 'New issues join this sprint',
  ru: 'Новые задачи попадают в этот спринт',
});
export const createSprint = msg({ en: 'Create', ru: 'Создать' });
export const saveSprint = msg({ en: 'Save', ru: 'Сохранить' });
export const sprintCreated = msg<{ name: string }>({
  en: ({ name }) => `Sprint “${name}” created`,
  ru: ({ name }) => `Спринт «${name}» создан`,
});
export const sprintSaveFailed = msg({
  en: 'Could not save the sprint.',
  ru: 'Не удалось сохранить спринт.',
});
export const sprintChanged = msg({
  en: 'Someone changed the sprint meanwhile. Check it and save again.',
  ru: 'Спринт успели изменить. Проверьте и сохраните ещё раз.',
});
export const archiveSprint = msg({ en: 'Archive', ru: 'В архив' });
export const restoreSprint = msg({ en: 'Restore', ru: 'Вернуть из архива' });
export const deleteSprint = msg({ en: 'Delete sprint', ru: 'Удалить спринт' });
export const deleteSprintTitle = msg<{ name: string }>({
  en: ({ name }) => `Delete “${name}”?`,
  ru: ({ name }) => `Удалить «${name}»?`,
});
export const deleteSprintText = msg({
  en: 'Its cards leave the board and queries no longer find it. The issues stay. This cannot be undone.',
  ru: 'Карточки уйдут с доски, запросы перестанут его находить. Задачи останутся. Отменить нельзя.',
});
export const sprintDeleted = msg({ en: 'Sprint deleted', ru: 'Спринт удалён' });
export const sprintArchived = msg({ en: 'Sprint archived', ru: 'Спринт в архиве' });
export const sprintRestored = msg({ en: 'Sprint restored', ru: 'Спринт возвращён' });
export const sprintActionFailed = msg({
  en: 'Could not change the sprint',
  ru: 'Не удалось изменить спринт',
});

// The schedule and the countdown (BRD-083), in calendar days.
export const schedule = msg({ en: 'Schedule', ru: 'Сроки' });
export const noDates = msg({ en: 'No dates', ru: 'Без дат' });
export const fromDate = msg<{ date: string }>({
  en: ({ date }) => `From ${date}`,
  ru: ({ date }) => `С ${date}`,
});
export const untilDate = msg<{ date: string }>({
  en: ({ date }) => `Until ${date}`,
  ru: ({ date }) => `До ${date}`,
});
export const daysLeft = msg<{ days: number }>({
  en: ({ days }) => plural('en', days, { one: '# day left', other: '# days left' }),
  ru: ({ days }) =>
    plural('ru', days, {
      one: 'Остался # день',
      few: 'Осталось # дня',
      many: 'Осталось # дней',
      other: 'Осталось # дня',
    }),
});
export const startsIn = msg<{ days: number }>({
  en: ({ days }) => plural('en', days, { one: 'Starts tomorrow', other: 'Starts in # days' }),
  ru: ({ days }) =>
    plural('ru', days, {
      one: 'Начнётся через # день',
      few: 'Начнётся через # дня',
      many: 'Начнётся через # дней',
      other: 'Начнётся через # дня',
    }),
});
export const lastDay = msg({ en: 'Last day', ru: 'Последний день' });
export const sprintFinished = msg({ en: 'Finished', ru: 'Завершён' });
export const startSprint = msg({ en: 'Start', ru: 'Начать' });
export const finishSprint = msg({ en: 'Finish', ru: 'Завершить' });
export const sprintStarted = msg({ en: 'Sprint started', ru: 'Спринт начат' });
export const finishTitle = msg<{ name: string }>({
  en: ({ name }) => `Finish “${name}”?`,
  ru: ({ name }) => `Завершить «${name}»?`,
});
export const finishText = msg({
  en: 'The sprint ends today. Unresolved issues can stay in it or move to another sprint.',
  ru: 'Спринт закончится сегодня. Незавершённые задачи можно оставить в нём или перенести.',
});
export const unresolvedChoice = msg({ en: 'Unresolved issues', ru: 'Незавершённые задачи' });
export const keepUnresolved = msg({ en: 'Leave in this sprint', ru: 'Оставить в этом спринте' });
export const carryUnresolved = msg({
  en: 'Move to another sprint',
  ru: 'Перенести в другой спринт',
});
export const carryTarget = msg({ en: 'Sprint', ru: 'Спринт' });
export const noCarryTarget = msg({
  en: 'There is no other open sprint yet: create one to move the issues to.',
  ru: 'Другого открытого спринта пока нет: создайте его, чтобы перенести задачи.',
});
export const sprintFinishedToast = msg({ en: 'Sprint finished', ru: 'Спринт завершён' });
export const carriedTo = msg<{ name: string }>({
  en: ({ name }) => `Sprint finished; unresolved issues moved to “${name}”`,
  ru: ({ name }) => `Спринт завершён, незавершённые задачи перенесены в «${name}»`,
});

// The footer (BRD-084).
export const boardFooter = msg({ en: 'About the board', ru: 'О доске' });
export const ownerLabel = msg({ en: 'Owner', ru: 'Владелец' });
export const goalLabel = msg({ en: 'Sprint goal', ru: 'Цель спринта' });
export const archivedSprintNote = msg({
  en: 'This sprint is in the archive.',
  ru: 'Этот спринт в архиве.',
});

// The header filter (BRD-070, BRD-081).
export const headerFilter = msg({ en: 'Card filter', ru: 'Фильтр карточек' });
export const sortedByQuery = msg({ en: 'Ordered by the query', ru: 'Порядок по запросу' });
export const sortedNoReorder = msg({
  en: 'The filter sorts the cards: they cannot be reordered within a column.',
  ru: 'Фильтр сортирует карточки: переставить их внутри колонки нельзя.',
});
export const filterFailed = msg({
  en: 'Could not apply the filter',
  ru: 'Не удалось применить фильтр',
});

// The backlog (BRD-089…BRD-096).
export const backlog = msg({ en: 'Backlog', ru: 'Бэклог' });
export const showBacklog = msg({ en: 'Show the backlog', ru: 'Показать бэклог' });
export const hideBacklog = msg({ en: 'Hide the backlog', ru: 'Скрыть бэклог' });
export const backlogRows = msg({ en: 'Backlog issues', ru: 'Задачи бэклога' });
export const backlogNone = msg({
  en: 'The board has no backlog yet.',
  ru: 'У доски пока нет бэклога.',
});
export const backlogNoneEditor = msg({
  en: 'Choose a saved search as the backlog in the settings.',
  ru: 'Выберите сохранённый поиск для бэклога в настройках.',
});
export const backlogHidden = msg({
  en: 'The backlog is a saved search that is not shared with you.',
  ru: 'Бэклог — сохранённый поиск, которым с вами не поделились.',
});
export const backlogEmpty = msg({ en: 'Nothing in the backlog', ru: 'В бэклоге пусто' });
export const backlogFailed = msg({
  en: 'Could not load the backlog',
  ru: 'Не удалось загрузить бэклог',
});
export const backlogChanged = msg({
  en: 'The backlog has changed.',
  ru: 'Бэклог изменился.',
});
export const refresh = msg({ en: 'Refresh', ru: 'Обновить' });
export const toSprint = msg({ en: 'To the sprint', ru: 'В спринт' });
export const toBoard = msg({ en: 'To the board', ru: 'На доску' });
export const toSprintOf = msg<{ id: string }>({
  en: ({ id }) => `Add ${id} to the sprint`,
  ru: ({ id }) => `Добавить ${id} в спринт`,
});
export const toBoardOf = msg<{ id: string }>({
  en: ({ id }) => `Add ${id} to the board`,
  ru: ({ id }) => `Добавить ${id} на доску`,
});
export const addedToSprint = msg({ en: 'Added to the sprint', ru: 'Добавлено в спринт' });
export const addedToBoard = msg({ en: 'Added to the board', ru: 'Добавлено на доску' });
export const addFailed = msg({ en: 'Could not add the issue', ru: 'Не удалось добавить задачу' });
export const newIssue = msg({ en: 'New issue', ru: 'Новая задача' });
export const resetOrder = msg({ en: 'Reset the order', ru: 'Сбросить порядок' });
export const orderReset = msg({
  en: 'The backlog follows its sort again',
  ru: 'Бэклог снова в порядке сортировки',
});
export const orderFailed = msg({
  en: 'Could not change the order',
  ru: 'Не удалось изменить порядок',
});
export const openBacklogList = msg({ en: 'Open as a list', ru: 'Открыть списком' });
export const backlogFilter = msg({ en: 'Backlog filter', ru: 'Фильтр бэклога' });
export const backlogActions = msg({ en: 'Backlog actions', ru: 'Действия с бэклогом' });
export const noSprintToAdd = msg({
  en: 'There is no sprint to add to: create one first.',
  ru: 'Добавлять некуда: сначала создайте спринт.',
});

// The query-mode preview (BRD-028, owner decision 2).
export const queryAddTitle = msg<{ id: string }>({
  en: ({ id }) => `Add ${id} to the board?`,
  ru: ({ id }) => `Добавить ${id} на доску?`,
});
export const queryAddText = msg({
  en: 'The board shows the issues of its query. To show this one, these fields change:',
  ru: 'На доске — задачи её запроса. Чтобы задача появилась на ней, изменятся поля:',
});
export const queryAddNothing = msg({
  en: 'The issue already matches the board query.',
  ru: 'Задача уже подходит под запрос доски.',
});
export const queryAdd = msg({ en: 'Add', ru: 'Добавить' });
export const queryRefused = msg({
  en: 'This issue cannot join the board',
  ru: 'Эту задачу нельзя добавить на доску',
});
export const emptyValue = msg({ en: 'empty', ru: 'пусто' });
export const tagsField = msg({ en: 'Tags', ru: 'Теги' });
export const changeLine = msg<{ field: string; was: string; now: string }>({
  en: ({ field, was, now }) => `${field}: ${was} → ${now}`,
  ru: ({ field, was, now }) => `${field}: ${was} → ${now}`,
});

// «Основное»: sprints and how issues join (BRD-024…BRD-029).
export const joining = msg({ en: 'Issues on the board', ru: 'Задачи на доске' });
export const sprintsOn = msg({ en: 'Work in sprints', ru: 'Работать спринтами' });
export const modeLegend = msg({ en: 'How issues join', ru: 'Как задачи попадают на доску' });
export const modes = {
  manual: msg({ en: 'By hand', ru: 'Вручную' }),
  auto: msg({ en: 'Every new issue', ru: 'Все новые задачи' }),
  query: msg({ en: 'By a query', ru: 'По запросу' }),
};
export const modeHints = {
  manual: msg({
    en: 'Cards are added from the backlog or the issue page.',
    ru: 'Карточки добавляют из бэклога или со страницы задачи.',
  }),
  auto: msg({
    en: 'A new issue of the board projects joins it at once.',
    ru: 'Новая задача проектов доски сразу попадает на неё.',
  }),
  query: msg({
    en: 'The board shows every issue that matches its query. Not with sprints.',
    ru: 'Доска показывает все задачи, подходящие под её запрос. Без спринтов.',
  }),
};
export const autoSprint = msg({ en: 'New issues join', ru: 'Новые задачи попадают в' });
export const autoSprintCurrent = msg({ en: 'The current sprint', ru: 'Текущий спринт' });
export const boardQuery = msg({ en: 'Board query', ru: 'Запрос доски' });
export const boardQueryProblems = msg({
  en: 'The query refers to something that is gone: the board shows no cards until it is fixed.',
  ru: 'Запрос ссылается на то, чего больше нет: пока его не исправят, карточек на доске нет.',
});

// «Бэклог» (BRD-089, BRD-090).
export const backlogTab = msg({ en: 'Backlog', ru: 'Бэклог' });
export const backlogSearch = msg({ en: 'Saved search', ru: 'Сохранённый поиск' });
export const backlogNoSearch = msg({ en: 'No backlog', ru: 'Без бэклога' });
export const backlogCurrent = msg<{ name: string }>({
  en: ({ name }) => `The backlog is the saved search “${name}”.`,
  ru: ({ name }) => `Бэклог — сохранённый поиск «${name}».`,
});
export const backlogAbout = msg({
  en: 'The backlog is a saved search with the board projects only. Its order by hand is shared by every board that uses it.',
  ru: 'Бэклог — сохранённый поиск только по проектам доски. Порядок, заданный вручную, общий для всех досок с этим поиском.',
});
export const backlogCreate = msg({ en: 'Create a new backlog', ru: 'Создать новый бэклог' });
export const backlogCreated = msg({ en: 'Backlog created', ru: 'Бэклог создан' });
export const backlogSearchesFailed = msg({
  en: 'Could not load the saved searches',
  ru: 'Не удалось загрузить сохранённые поиски',
});

// Boards of an issue: the field panel and the new-issue form (ISS-020, ISS-007).
export const issueBoards = msg({ en: 'Boards', ru: 'Доски' });
export const noIssueBoards = msg({ en: 'Not on boards', ru: 'Не на досках' });
export const addToBoard = msg({ en: 'Add to board', ru: 'Добавить на доску' });
export const boardChoice = msg({ en: 'Board', ru: 'Доска' });
export const sprintChoice = msg({ en: 'Sprint', ru: 'Спринт' });
export const currentSprintChoice = msg({ en: 'The current sprint', ru: 'Текущий спринт' });
export const noBoardsForIssue = msg({
  en: 'No other boards you can open include the project of this issue.',
  ru: 'Других досок с проектом этой задачи, доступных вам, нет.',
});
export const autoBoard = msg({ en: 'automatically', ru: 'автоматически' });
export const autoBoardHint = msg({
  en: 'The issue joins this board when it is created.',
  ru: 'Задача попадёт на эту доску при создании.',
});
export const skippedBoard = msg({ en: 'will not join', ru: 'не попадёт' });
export const putBack = msg<{ name: string }>({
  en: ({ name }) => `Put back on “${name}”`,
  ru: ({ name }) => `Вернуть на «${name}»`,
});
export const putBackShort = msg({ en: 'Put back', ru: 'Вернуть' });
export const removeFromBoardOf = msg<{ name: string }>({
  en: ({ name }) => `Remove from “${name}”`,
  ru: ({ name }) => `Убрать с доски «${name}»`,
});
export const removedFromBoard = msg<{ name: string }>({
  en: ({ name }) => `Removed from “${name}”`,
  ru: ({ name }) => `Убрано с доски «${name}»`,
});
export const issueBoardsFailed = msg({
  en: 'Could not load the boards of the issue',
  ru: 'Не удалось загрузить доски задачи',
});
export const removeBoardFailed = msg({
  en: 'Could not remove the issue from the board',
  ru: 'Не удалось убрать задачу с доски',
});

// Lanes (BRD-049…BRD-057).
export const lanesLabel = msg({ en: 'Board lanes', ru: 'Дорожки доски' });
export const otherLane = msg({ en: 'Other', ru: 'Прочее' });
export const hiddenLane = msg({ en: 'Not available to you', ru: 'Вам недоступно' });
export const dateLanes: Record<string, () => string> = {
  later: msg({ en: 'Later', ru: 'Позже' }),
  today: msg({ en: 'Today', ru: 'Сегодня' }),
  yesterday: msg({ en: 'Yesterday', ru: 'Вчера' }),
  this_week: msg({ en: 'This week', ru: 'На этой неделе' }),
  last_week: msg({ en: 'Last week', ru: 'На прошлой неделе' }),
  this_month: msg({ en: 'This month', ru: 'В этом месяце' }),
  older: msg({ en: 'Earlier', ru: 'Раньше' }),
  none: msg({ en: 'No date', ru: 'Без даты' }),
};
export const laneLabel = msg<{ name: string }>({
  en: ({ name }) => `Lane “${name}”`,
  ru: ({ name }) => `Дорожка «${name}»`,
});
export const collapseLane = msg<{ name: string }>({
  en: ({ name }) => `Collapse lane “${name}”`,
  ru: ({ name }) => `Свернуть дорожку «${name}»`,
});
export const expandLane = msg<{ name: string }>({
  en: ({ name }) => `Expand lane “${name}”`,
  ru: ({ name }) => `Развернуть дорожку «${name}»`,
});
export const collapseAllLanes = msg({ en: 'Collapse all', ru: 'Свернуть все' });
export const expandAllLanes = msg({ en: 'Expand all', ru: 'Развернуть все' });
export const laneActions = msg<{ name: string }>({
  en: ({ name }) => `Lane “${name}”: actions`,
  ru: ({ name }) => `Дорожка «${name}»: действия`,
});
export const laneToBacklog = msg({ en: 'Back to the backlog', ru: 'Вернуть в бэклог' });
export const laneSentBack = msg<{ count: number }>({
  en: ({ count }) =>
    plural('en', count, {
      one: '# issue went back to the backlog',
      other: '# issues went back to the backlog',
    }),
  ru: ({ count }) =>
    plural('ru', count, {
      one: 'В бэклог вернулась # задача',
      few: 'В бэклог вернулись # задачи',
      many: 'В бэклог вернулись # задач',
      other: 'В бэклог вернулись # задачи',
    }),
});
export const laneBackFailed = msg({
  en: 'Could not take the lane back to the backlog',
  ru: 'Не удалось вернуть дорожку в бэклог',
});
export const laneMoveFailed = msg({
  en: 'Could not move the lane',
  ru: 'Не удалось переместить дорожку',
});
export const moreLanes = msg<{ count: number }>({
  en: ({ count }) => `More lanes: ${String(count)}`,
  ru: ({ count }) => `Ещё дорожки: ${String(count)}`,
});
export const moreLanesFailed = msg({
  en: 'Could not show more lanes',
  ru: 'Не удалось показать остальные дорожки',
});
export const laneCellCards = msg<{ column: string; lane: string }>({
  en: ({ column, lane }) => `Cards of “${column}” in lane “${lane}”`,
  ru: ({ column, lane }) => `Карточки колонки «${column}» в дорожке «${lane}»`,
});
export const moveToLane = msg({ en: 'To lane', ru: 'В дорожку' });

// The «Дорожки» settings (BRD-086, BRD-049…BRD-055).
export const lanesTab = msg({ en: 'Lanes', ru: 'Дорожки' });
export const cardTab = msg({ en: 'Card', ru: 'Карточка' });
export const cardFields = msg({ en: 'Fields on cards', ru: 'Поля на карточке' });
export const cardFieldsHint = msg({
  en: 'Up to ten, in this order. Medium cards and larger show them.',
  ru: 'До десяти, в этом порядке. Их показывают карточки среднего размера и крупнее.',
});
export const noCardFields = msg({ en: 'Cards show no fields.', ru: 'Полей на карточках нет.' });
export const addCardField = msg({ en: 'Add a field', ru: 'Добавить поле' });
export const chooseField = msg({ en: 'Choose a field', ru: 'Выберите поле' });
export const cardFieldsFull = msg({
  en: 'Ten fields is the most a card shows.',
  ru: 'Больше десяти полей карточка не показывает.',
});
export const initialOnly = msg({ en: 'First letter only', ru: 'Только первая буква' });
export const removeCardField = msg<{ name: string }>({
  en: ({ name }) => `Remove ${name} from cards`,
  ru: ({ name }) => `Убрать «${name}» с карточек`,
});
export const cardColor = msg({ en: 'Card color', ru: 'Цвет карточек' });
export const cardColors = {
  none: msg({ en: 'None', ru: 'Без цвета' }),
  project: msg({ en: 'By project', ru: 'По проекту' }),
  field: msg({ en: 'By a field', ru: 'По полю' }),
} as const;
export const cardColorHints = {
  none: msg({ en: 'Cards are all alike.', ru: 'Все карточки одинаковые.' }),
  project: msg({
    en: 'Each project gets a color of its own.',
    ru: 'У каждого проекта свой цвет.',
  }),
  field: msg({
    en: 'Cards take the color of the field value.',
    ru: 'Карточка берёт цвет значения поля.',
  }),
} as const;
export const colorField = msg({ en: 'Color field', ru: 'Поле цвета' });
export const projectColor = msg<{ name: string }>({
  en: ({ name }) => `Color of ${name}`,
  ru: ({ name }) => `Цвет проекта ${name}`,
});
export const estimationField = msg({ en: 'Estimation', ru: 'Оценка' });
export const estimationHint = msg({
  en: 'Shown on the card and summed in the headers.',
  ru: 'Видна на карточке и суммируется в заголовках.',
});
export const originalEstimationField = msg({
  en: 'Original estimation',
  ru: 'Исходная оценка',
});
export const noEstimation = msg({ en: 'None', ru: 'Нет' });
export const cardsFailed = msg({
  en: 'Could not load the fields of the board projects.',
  ru: 'Не удалось загрузить поля проектов доски.',
});
export const laneMode = msg({ en: 'Lanes', ru: 'Дорожки' });
export const laneModes = {
  none: msg({ en: 'No lanes', ru: 'Без дорожек' }),
  values: msg({ en: 'By the values of a field', ru: 'По значениям поля' }),
  parents: msg({ en: 'By parent issues', ru: 'По родительским задачам' }),
};
export const laneModeHints = {
  none: msg({ en: 'Cards only in columns.', ru: 'Карточки только в колонках.' }),
  values: msg({
    en: 'A lane per value: people, priorities, projects, dates.',
    ru: 'Дорожка на каждое значение: люди, приоритеты, проекты, даты.',
  }),
  parents: msg({
    en: 'A lane per parent issue; its subtasks are its cards.',
    ru: 'Дорожка на каждую родительскую задачу, её подзадачи — карточки.',
  }),
};
export const laneSource = msg({ en: 'Lanes by', ru: 'Дорожки по' });
export const laneSourceNone = msg({ en: 'Choose…', ru: 'Выберите…' });
export const laneAttributes = {
  project: msg({ en: 'Project', ru: 'Проект' }),
  tags: msg({ en: 'Tags', ru: 'Теги' }),
  created: msg({ en: 'Created', ru: 'Дата создания' }),
  updated: msg({ en: 'Updated', ru: 'Дата изменения' }),
};
export const parentField = msg({
  en: 'Only parents with a value of',
  ru: 'Только родители со значением поля',
});
export const anyParent = msg({ en: 'Any parent', ru: 'Любые родители' });
export const laneValues = msg({ en: 'Lanes in order', ru: 'Дорожки по порядку' });
export const parentValues = msg({ en: 'Parent values', ru: 'Значения родителей' });
export const noLaneValues = msg({
  en: 'No lanes yet: every card is in “Other”.',
  ru: 'Дорожек пока нет: все карточки в «Прочем».',
});
export const allDateLanes = msg({
  en: 'None chosen: every period has a lane.',
  ru: 'Ничего не выбрано: дорожка у каждого периода.',
});
export const anyParentValue = msg({
  en: 'None chosen: any parent.',
  ru: 'Ничего не выбрано: любой родитель.',
});
export const chooseLanes = msg({ en: 'Lanes to show', ru: 'Какие дорожки показать' });
export const findLaneValues = msg({ en: 'Find', ru: 'Найти' });
export const removeLane = msg<{ name: string }>({
  en: ({ name }) => `Remove lane “${name}”`,
  ru: ({ name }) => `Убрать дорожку «${name}»`,
});
export const otherPlace = msg({ en: 'Lane “Other”', ru: 'Дорожка «Прочее»' });
export const otherPlaces = {
  top: msg({ en: 'At the top', ru: 'Сверху' }),
  bottom: msg({ en: 'At the bottom', ru: 'Снизу' }),
  off: msg({ en: 'Hidden', ru: 'Не показывать' }),
};
export const otherOffHint = msg({
  en: 'Cards in no lane are not shown.',
  ru: 'Карточки вне дорожек не видны.',
});
export const newCardValue = msg({ en: 'New cards get', ru: 'Новые карточки получают' });
export const newCardValueNone = msg({ en: 'Nothing', ru: 'Ничего' });
export const lanesFailed = msg({
  en: 'Could not load the choices for lanes',
  ru: 'Не удалось загрузить варианты дорожек',
});

// Creating on the board (BRD-054, BRD-076, BRD-077), the issue popup (BRD-079) and the board's
// health (BRD-023).
export const newMenu = msg({ en: 'New', ru: 'Новая' });
export const newCardItem = msg({ en: 'Card', ru: 'Карточку' });
export const newLaneItem = msg({ en: 'Lane', ru: 'Дорожку' });
export const newSprintItem = msg({ en: 'Sprint', ru: 'Спринт' });
export const newLane = msg({ en: 'New lane', ru: 'Новая дорожка' });
export const laneTitle = msg({ en: 'Title', ru: 'Название' });
export const laneTitleHint = msg({
  en: 'A new issue with this summary becomes the lane.',
  ru: 'Дорожкой станет новая задача с этим названием.',
});
export const laneCreated = msg<{ name: string }>({
  en: ({ name }) => `Lane “${name}” created`,
  ru: ({ name }) => `Дорожка «${name}» создана`,
});
export const laneCreateFailed = msg({
  en: 'Could not create the lane',
  ru: 'Не удалось создать дорожку',
});
export const addInCell = msg<{ name: string }>({
  en: ({ name }) => `Add a card to “${name}”`,
  ru: ({ name }) => `Добавить карточку в «${name}»`,
});
export const cellForm = msg({ en: 'New card', ru: 'Новая карточка' });
export const cellDraftFailed = msg({
  en: 'Could not start a card here',
  ru: 'Не удалось начать карточку здесь',
});
export const summaryLabel = msg({ en: 'Summary', ru: 'Название' });
export const descriptionLabel = msg({ en: 'Description', ru: 'Описание' });
export const moreFields = msg({ en: 'Fields', ru: 'Поля' });
export const setByCell = msg({ en: 'Set by the cell', ru: 'Задано ячейкой' });
export const lockedPlace = msg<{ name: string; value: string }>({
  en: ({ name, value }) => `${name}: ${value}`,
  ru: ({ name, value }) => `${name}: ${value}`,
});
export const subtaskOfLane = msg<{ name: string }>({
  en: ({ name }) => `Subtask of “${name}”`,
  ru: ({ name }) => `Подзадача «${name}»`,
});
export const addCardButton = msg({ en: 'Add', ru: 'Добавить' });
export const addAndOpen = msg({ en: 'Add and open', ru: 'Добавить и открыть' });
export const cardCreated = msg<{ id: string }>({
  en: ({ id }) => `${id} added to the board`,
  ru: ({ id }) => `${id} добавлена на доску`,
});
export const fillRequired = msg({
  en: 'Fill in the required fields',
  ru: 'Заполните обязательные поля',
});
export const draftKept = msg({
  en: 'The draft stays in this cell',
  ru: 'Черновик остался в этой ячейке',
});
export const issuePopup = msg({ en: 'Issue', ru: 'Задача' });
export const issueGone = msg({
  en: 'The issue is gone or hidden from you.',
  ru: 'Задачи больше нет, или она вам не видна.',
});
export const healthTitle = msg({
  en: 'The board settings need attention',
  ru: 'Настройки доски требуют внимания',
});
export const healthOpen = msg({ en: 'Open', ru: 'Открыть' });
export const healthSlots = {
  lane: msg({ en: 'lanes', ru: 'дорожек' }),
  color: msg({ en: 'card color', ru: 'цвета карточек' }),
  estimation: msg({ en: 'estimation', ru: 'оценки' }),
  original_estimation: msg({ en: 'original estimation', ru: 'исходной оценки' }),
};
interface HealthArgs {
  project: string;
  value: string;
  slot: string;
}
export const healthKinds = {
  no_projects: msg<HealthArgs>({
    en: () => 'The board has no projects.',
    ru: () => 'На доске нет проектов.',
  }),
  no_column_field: msg<HealthArgs>({
    en: () => 'No field is chosen for the columns.',
    ru: () => 'Не выбрано поле колонок.',
  }),
  no_columns: msg<HealthArgs>({
    en: () => 'The board has no columns.',
    ru: () => 'На доске нет колонок.',
  }),
  column_field_missing: msg<HealthArgs>({
    en: ({ project }) =>
      `Project ${project} does not have the column field: its issues are not shown.`,
    ru: ({ project }) => `В проекте ${project} нет поля колонок: его задачи не видны.`,
  }),
  column_value_missing: msg<HealthArgs>({
    en: ({ project, value }) => `Project ${project} does not use the column value “${value}”.`,
    ru: ({ project, value }) => `Проект ${project} не использует значение колонки «${value}».`,
  }),
  archived_project: msg<HealthArgs>({
    en: ({ project }) => `Project ${project} is archived.`,
    ru: ({ project }) => `Проект ${project} в архиве.`,
  }),
  field_missing: msg<HealthArgs>({
    en: ({ slot }) => `The field of ${slot} is in no project of the board.`,
    ru: ({ slot }) => `Поля ${slot} нет ни в одном проекте доски.`,
  }),
  field_wrong_kind: msg<HealthArgs>({
    en: ({ slot }) => `The field of ${slot} is of a kind that does not fit.`,
    ru: ({ slot }) => `Поле ${slot} не подходит по типу.`,
  }),
  backlog_hidden: msg<HealthArgs>({
    en: () => 'The backlog search is not visible to you.',
    ru: () => 'Поиск бэклога вам не виден.',
  }),
  default_not_column: msg<HealthArgs>({
    en: ({ project }) =>
      `New issues of ${project} get a value that is not a column: the board misses them.`,
    ru: ({ project }) =>
      `Новые задачи ${project} получают значение не из колонок: доска их не покажет.`,
  }),
};
