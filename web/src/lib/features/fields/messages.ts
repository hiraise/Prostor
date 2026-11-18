// Texts of the fields and value sets screens (FLD). Voice and glossary: docs/design.md §12.
// Russian plurals put a non-breaking space between the number and the word.
import type { FieldKind } from '#lib/api/schema.gen.ts';
import { msg, plural } from '#lib/i18n/message.ts';

// ---- Kinds of fields (FLD-001…FLD-014) ----

export const kindNames: Record<FieldKind, () => string> = {
  string: msg({ en: 'Text line', ru: 'Строка' }),
  text: msg({ en: 'Formatted text', ru: 'Форматированный текст' }),
  integer: msg({ en: 'Whole number', ru: 'Целое число' }),
  float: msg({ en: 'Decimal number', ru: 'Дробное число' }),
  date: msg({ en: 'Date', ru: 'Дата' }),
  date_time: msg({ en: 'Date and time', ru: 'Дата и время' }),
  duration: msg({ en: 'Duration', ru: 'Длительность' }),
  enum: msg({ en: 'Choice', ru: 'Выбор из списка' }),
  state: msg({ en: 'State', ru: 'Состояние' }),
  user: msg({ en: 'User', ru: 'Пользователь' }),
  group: msg({ en: 'Group', ru: 'Группа' }),
  version: msg({ en: 'Version', ru: 'Версия' }),
  build: msg({ en: 'Build', ru: 'Сборка' }),
  owned: msg({ en: 'Value with owner', ru: 'Значение с владельцем' }),
};

export const typeWithMany = msg<{ kind: string }>({
  en: ({ kind }) => `${kind} · several values`,
  ru: ({ kind }) => `${kind} · несколько значений`,
});
export const fieldType = msg({ en: 'Type', ru: 'Тип' });
export const severalValues = msg({ en: 'Several values', ru: 'Несколько значений' });
export const severalValuesHint = msg({
  en: 'An issue can hold more than one value.',
  ru: 'В задаче можно выбрать больше одного значения.',
});

// ---- Value colors (FLD-044) ----

export const colorNames: readonly (() => string)[] = [
  msg({ en: 'Gray', ru: 'Серый' }),
  msg({ en: 'Red', ru: 'Красный' }),
  msg({ en: 'Orange', ru: 'Оранжевый' }),
  msg({ en: 'Amber', ru: 'Янтарный' }),
  msg({ en: 'Lime', ru: 'Салатовый' }),
  msg({ en: 'Green', ru: 'Зелёный' }),
  msg({ en: 'Teal', ru: 'Бирюзовый' }),
  msg({ en: 'Sky', ru: 'Голубой' }),
  msg({ en: 'Blue', ru: 'Синий' }),
  msg({ en: 'Violet', ru: 'Фиолетовый' }),
  msg({ en: 'Pink', ru: 'Розовый' }),
  msg({ en: 'Brown', ru: 'Коричневый' }),
];
export const color = msg({ en: 'Color', ru: 'Цвет' });
export const noColor = msg({ en: 'No color', ru: 'Без цвета' });

// ---- Properties of a field ----

export const name = msg({ en: 'Name', ru: 'Название' });
export const fieldNameHint = msg({
  en: 'Up to 100 characters, without <, >, / and :.',
  ru: 'До 100 символов, без <, >, / и :.',
});
export const aliases = msg({ en: 'Aliases', ru: 'Синонимы' });
export const aliasesHint = msg({
  en: 'Other words for the field in searches and commands, separated by commas.',
  ru: 'Другие слова для поля в поиске и командах, через запятую.',
});
export const aliasesLine = msg<{ list: string }>({
  en: ({ list }) => `Aliases: ${list}`,
  ru: ({ list }) => `Синонимы: ${list}`,
});
export const canBeEmpty = msg({ en: 'Can be empty', ru: 'Может быть пустым' });
export const canBeEmptyHint = msg({
  en: 'When off, new issues get the default value; without one, the field must be filled.',
  ru: 'Если выключено, новые задачи получают значение по умолчанию; без него поле нужно заполнить.',
});
export const emptyLabel = msg({ en: 'Empty value label', ru: 'Подпись пустого значения' });
export const emptyLabelHint = msg({
  en: 'Shown when the field has no value, for example “Unassigned”. Works in searches too.',
  ru: 'Показывается, когда у поля нет значения, например «Не назначено». Работает и в поиске.',
});
export const emptyLabelOff = msg({
  en: 'A field that cannot be empty has no such label.',
  ru: 'У поля, которое не может быть пустым, такой подписи нет.',
});
export const privateField = msg({ en: 'Private field', ru: 'Скрытое поле' });
export const privateHint = msg<{ read: string; update: string }>({
  en: ({ read, update }) =>
    `Only people whose roles include “${read}” see it, and changing it needs “${update}”.`,
  ru: ({ read, update }) =>
    `Его видят только те, чьи роли включают «${read}», а для изменения нужно «${update}».`,
});
export const privateBadge = msg({ en: 'Private', ru: 'Скрытое' });
export const privateByDefault = msg({ en: 'Private by default', ru: 'Скрытое по умолчанию' });
export const requiredBadge = msg({ en: 'Required', ru: 'Обязательное' });
export const requiredNote = msg({
  en: 'Required: it cannot be empty and has no default value, so every new issue must fill it.',
  ru: 'Обязательное: поле не может быть пустым и не имеет значения по умолчанию, поэтому его нужно заполнить в каждой новой задаче.',
});
export const builtIn = msg({ en: 'Built-in', ru: 'Встроенное' });
export const autoAttach = msg({ en: 'Add to new projects', ru: 'Добавлять в новые проекты' });
export const autoAttachBadge = msg({
  en: 'Added to new projects',
  ru: 'Добавляется в новые проекты',
});
export const autoAttachHint = msg({
  en: 'Every project created from the blank template gets it. The Scrum and Kanban templates bring their own fields.',
  ru: 'Его получает каждый проект, созданный из пустого шаблона. Шаблоны Scrum и Kanban приносят свои поля.',
});

// ---- Default values (FLD-019, FLD-055) ----

export const defaultValue = msg({ en: 'Default value', ru: 'Значение по умолчанию' });
export const defaultValues = msg({ en: 'Default values', ru: 'Значения по умолчанию' });
export const defaultsHint = msg({
  en: 'New issues get it. Archived values are not offered.',
  ru: 'Его получают новые задачи. Значения в архиве не предлагаются.',
});
export const noDefault = msg({ en: 'No default value', ru: 'Без значения по умолчанию' });
export const defaultLine = msg({ en: 'Default:', ru: 'По умолчанию:' });
export const nobodyByDefault = msg({ en: 'Nobody by default.', ru: 'По умолчанию никого.' });
export const choosePeople = msg({ en: 'Choose people', ru: 'Выбрать людей' });
export const choosePerson = msg({ en: 'Choose a person', ru: 'Выбрать человека' });
export const removeDefaultPerson = msg<{ name: string }>({
  en: ({ name }) => `Remove ${name} from the default`,
  ru: ({ name }) => `Убрать ${name} из значения по умолчанию`,
});
export const defaultPeopleHint = msg({
  en: 'Only people the field’s set offers can be chosen.',
  ru: 'Выбрать можно только людей, которых предлагает набор поля.',
});
export const defaultNotInSet = msg({
  en: 'A default value must be one the field offers: an active value of its set, or a person it lists.',
  ru: 'Значение по умолчанию должно быть из тех, что предлагает поле: действующее значение набора или человек из его списка.',
});
export const defaultsLoading = msg({ en: 'Loading values…', ru: 'Загрузка значений…' });

// ---- Value sets (FLD-029…FLD-031) ----

export const values = msg({ en: 'Values', ru: 'Значения' });
export const valueSet = msg({ en: 'Value set', ru: 'Набор значений' });
export const setLine = msg<{ name: string }>({
  en: ({ name }) => `Values: ${name}`,
  ru: ({ name }) => `Значения: ${name}`,
});
export const sharedWith = msg<{ count: number }>({
  en: ({ count }) =>
    plural('en', count, {
      one: 'shared with # other project',
      other: 'shared with # other projects',
    }),
  ru: ({ count }) =>
    plural('ru', count, {
      one: 'общие ещё с # проектом',
      few: 'общие ещё с # проектами',
      many: 'общие ещё с # проектами',
      other: 'общие ещё с # проектами',
    }),
});
export const usedInNames = msg<{ names: string }>({
  en: ({ names }) => `Used in: ${names}.`,
  ru: ({ names }) => `Используется в проектах: ${names}.`,
});
export const usedInHidden = msg<{ count: number }>({
  en: ({ count }) =>
    plural('en', count, {
      one: 'Also # project you cannot see.',
      other: 'Also # projects you cannot see.',
    }),
  ru: ({ count }) =>
    plural('ru', count, {
      one: 'И ещё # недоступный вам проект.',
      few: 'И ещё # недоступных вам проекта.',
      many: 'И ещё # недоступных вам проектов.',
      other: 'И ещё # недоступного вам проекта.',
    }),
});
export const setNotUsed = msg({
  en: 'No project uses this set.',
  ru: 'Этот набор не используется ни в одном проекте.',
});
export const defaultSetOf = msg<{ count: number }>({
  en: ({ count }) =>
    plural('en', count, { one: 'Default set of # field', other: 'Default set of # fields' }),
  ru: ({ count }) =>
    plural('ru', count, {
      one: 'Набор по умолчанию у # поля',
      few: 'Набор по умолчанию у # полей',
      many: 'Набор по умолчанию у # полей',
      other: 'Набор по умолчанию у # поля',
    }),
});
export const sharedNotice = msg<{ count: number }>({
  en: ({ count }) =>
    plural('en', count, {
      one: 'Changes to these values apply to # project using the set.',
      other: 'Changes to these values apply to all # projects using the set.',
    }),
  ru: ({ count }) =>
    plural('ru', count, {
      one: 'Изменения этих значений коснутся # проекта с этим набором.',
      few: 'Изменения этих значений коснутся всех # проектов с этим набором.',
      many: 'Изменения этих значений коснутся всех # проектов с этим набором.',
      other: 'Изменения этих значений коснутся всех # проекта с этим набором.',
    }),
});
export const notEditableInProject = msg({
  en: 'You cannot change these values: projects you do not configure use them too.',
  ru: 'Изменить эти значения нельзя: они используются и в проектах, которые вы не настраиваете.',
});
export const notEditable = msg({
  en: 'Only someone who configures every project using this set can change it.',
  ru: 'Изменить набор может только тот, кто настраивает все проекты, где он используется.',
});
export const forkOffer = msg({
  en: 'Make a copy for this project to change its values freely.',
  ru: 'Сделайте копию для этого проекта, чтобы менять значения свободно.',
});
export const fork = msg({ en: 'Make a copy for this project', ru: 'Сделать копию для проекта' });
export const forked = msg({
  en: 'The project now has its own copy of the values',
  ru: 'Теперь у проекта своя копия значений',
});
export const groupFieldNote = msg({
  en: 'A group field offers every group people can see; it has no list of its own.',
  ru: 'В поле-группе можно выбрать любую группу, которую видит человек; своего списка у него нет.',
});
export const simpleFieldNote = msg({
  en: 'People type the value of this field; it has no list of values.',
  ru: 'Значение этого поля вводят вручную; списка значений у него нет.',
});

// ---- Values (FLD-032…FLD-049) ----

export const valueList = msg<{ name: string }>({
  en: ({ name }) => `Values of “${name}”`,
  ru: ({ name }) => `Значения набора «${name}»`,
});
export const valueActions = msg<{ name: string }>({
  en: ({ name }) => `Actions for “${name}”`,
  ru: ({ name }) => `Действия со значением «${name}»`,
});
export const addValue = msg({ en: 'Add value', ru: 'Добавить значение' });
export const addValueAfter = msg({ en: 'Add value after it', ru: 'Добавить значение после' });
export const noValues = msg({ en: 'No values yet', ru: 'Значений пока нет' });
export const noValuesText = msg({
  en: 'Values appear in the field’s list in the order you give them.',
  ru: 'Значения появятся в списке поля в том порядке, который вы зададите.',
});
export const allValuesHidden = msg({
  en: 'All values are hidden by the options above.',
  ru: 'Все значения скрыты настройками выше.',
});
export const edit = msg({ en: 'Edit', ru: 'Изменить' });
export const rename = msg({ en: 'Rename', ru: 'Переименовать' });
export const moveUp = msg({ en: 'Move up', ru: 'Переместить выше' });
export const moveDown = msg({ en: 'Move down', ru: 'Переместить ниже' });
export const archive = msg({ en: 'Archive', ru: 'Архивировать' });
export const restore = msg({ en: 'Restore', ru: 'Восстановить' });
export const deleteAction = msg({ en: 'Delete', ru: 'Удалить' });
export const archivedBadge = msg({ en: 'Archived', ru: 'В архиве' });
export const archivedHint = msg({
  en: 'Archived values are not offered for new choices; issues that hold them keep them.',
  ru: 'Значения в архиве не предлагаются для выбора; задачи, где они уже выбраны, их сохраняют.',
});
export const archived = msg({ en: 'Archived', ru: 'В архиве' });
export const valueArchived = msg<{ name: string }>({
  en: ({ name }) => `“${name}” archived`,
  ru: ({ name }) => `«${name}» в архиве`,
});
export const valueRestored = msg<{ name: string }>({
  en: ({ name }) => `“${name}” restored`,
  ru: ({ name }) => `«${name}» восстановлено`,
});
export const resolved = msg({ en: 'Resolved', ru: 'Решённое' });
export const resolvedHint = msg({
  en: 'Issues in this state count as resolved.',
  ru: 'Задачи в этом состоянии считаются решёнными.',
});
export const released = msg({ en: 'Released', ru: 'Выпущена' });
export const releasedHint = msg({
  en: 'The version is out; the release date is when it came out.',
  ru: 'Версия вышла; дата выпуска — когда это произошло.',
});
export const releaseDate = msg({ en: 'Release date', ru: 'Дата выпуска' });
export const startDate = msg({ en: 'Start date', ru: 'Дата начала' });
export const assembledOn = msg({ en: 'Build date', ru: 'Дата сборки' });
export const releaseLine = msg({ en: 'Release:', ru: 'Выпуск:' });
export const startLine = msg({ en: 'Start:', ru: 'Начало:' });
export const builtLine = msg({ en: 'Built:', ru: 'Собрана:' });
export const ownerLine = msg({ en: 'Owner:', ru: 'Владелец:' });
export const owner = msg({ en: 'Owner', ru: 'Владелец' });
export const ownerHint = msg({
  en: 'Automation can assign new issues with this value to the owner.',
  ru: 'Автоматизация может назначать владельцу новые задачи с этим значением.',
});
export const noOwner = msg({ en: 'No owner', ru: 'Без владельца' });
export const chooseOwner = msg({ en: 'Choose owner', ru: 'Выбрать владельца' });
export const changeOwner = msg({ en: 'Change owner', ru: 'Сменить владельца' });
export const removeOwner = msg({ en: 'Remove owner', ru: 'Убрать владельца' });
export const description = msg({ en: 'Description', ru: 'Описание' });
export const descriptionHint = msg({
  en: 'Shown as a hint next to the chosen value.',
  ru: 'Показывается подсказкой рядом с выбранным значением.',
});
export const showArchived = msg({ en: 'Show archived values', ru: 'Показывать значения в архиве' });
export const showReleased = msg({
  en: 'Show released versions',
  ru: 'Показывать выпущенные версии',
});
export const newValueTitle = msg({ en: 'New value', ru: 'Новое значение' });
export const editValueTitle = msg<{ name: string }>({
  en: ({ name }) => `Edit “${name}”`,
  ru: ({ name }) => `Изменить «${name}»`,
});
export const positionAfter = msg<{ name: string }>({
  en: ({ name }) => `It goes right after “${name}”.`,
  ru: ({ name }) => `Оно встанет сразу после «${name}».`,
});
export const positionEnd = msg({
  en: 'It goes at the end of the list.',
  ru: 'Оно встанет в конец списка.',
});
export const saveChanges = msg({ en: 'Save changes', ru: 'Сохранить изменения' });
export const save = msg({ en: 'Save', ru: 'Сохранить' });
export const valueAdded = msg({ en: 'Value added', ru: 'Значение добавлено' });
export const valueSaved = msg({ en: 'Value saved', ru: 'Значение сохранено' });
export const valueDeleted = msg({ en: 'Value deleted', ru: 'Значение удалено' });
export const deleteValueTitle = msg<{ name: string }>({
  en: ({ name }) => `Delete “${name}”?`,
  ru: ({ name }) => `Удалить «${name}»?`,
});
export const deleteValueText = msg({
  en: 'The value leaves the set, and issues that hold it lose it. This cannot be undone; to keep it on issues, archive it instead.',
  ru: 'Значение исчезнет из набора, а задачи, где оно выбрано, его потеряют. Это нельзя отменить; чтобы сохранить его в задачах, архивируйте значение.',
});
export const deleteValueUsed = msg<{ count: number }>({
  en: ({ count }) =>
    plural('en', count, {
      one: '# issue holds this value. Choose what it gets instead, or leave the field empty there.',
      other:
        '# issues hold this value. Choose what they get instead, or leave the field empty there.',
    }),
  ru: ({ count }) =>
    plural('ru', count, {
      one: 'Это значение выбрано в # задаче. Выберите, что поставить вместо него, или оставьте поле пустым.',
      few: 'Это значение выбрано в # задачах. Выберите, что поставить вместо него, или оставьте поле пустым.',
      many: 'Это значение выбрано в # задачах. Выберите, что поставить вместо него, или оставьте поле пустым.',
      other:
        'Это значение выбрано в # задачи. Выберите, что поставить вместо него, или оставьте поле пустым.',
    }),
});
export const replacementLabel = msg({ en: 'Put instead', ru: 'Поставить вместо него' });
export const noReplacement = msg({
  en: 'Don’t replace — clear the field',
  ru: 'Не заменять — очистить поле',
});
export const replacementHint = msg({
  en: 'Issues are updated in the background.',
  ru: 'Задачи обновятся в фоне.',
});
export const deleteValueUsedNote = msg({
  en: 'This cannot be undone; to keep the value on issues, archive it instead.',
  ru: 'Это нельзя отменить; чтобы сохранить значение в задачах, архивируйте его.',
});
export const deleteValueButton = msg({ en: 'Delete value', ru: 'Удалить значение' });
export const valueNameTaken = msg({
  en: 'The set has a value with this name.',
  ru: 'В наборе уже есть значение с таким названием.',
});
export const setFull = msg({
  en: 'The set holds the most values it can: 2,000.',
  ru: 'В наборе уже максимум значений: 2 000.',
});
export const moved = msg<{ name: string; position: number; total: number }>({
  en: ({ name, position, total }) => `“${name}” is now ${String(position)} of ${String(total)}.`,
  ru: ({ name, position, total }) =>
    `«${name}» теперь на ${String(position)}-м месте из ${String(total)}.`,
});
export const renameLabel = msg<{ name: string }>({
  en: ({ name }) => `New name for “${name}”`,
  ru: ({ name }) => `Новое название для «${name}»`,
});
export const flagFor = msg<{ flag: string; name: string }>({
  en: ({ flag, name }) => `${flag}: ${name}`,
  ru: ({ flag, name }) => `${flag}: ${name}`,
});

// ---- Members of user sets (FLD-039, FLD-040) ----

export const people = msg({ en: 'People', ru: 'Люди' });
export const groups = msg({ en: 'Groups', ru: 'Группы' });
export const addPeople = msg({ en: 'Add people', ru: 'Добавить людей' });
export const addGroups = msg({ en: 'Add groups', ru: 'Добавить группы' });
export const membersIntro = msg({
  en: 'The field offers the people listed here and every member of these groups.',
  ru: 'Поле предлагает людей из этого списка и всех участников этих групп.',
});
export const noPeopleListed = msg({ en: 'No people listed.', ru: 'Людей в списке нет.' });
export const noGroupsListed = msg({ en: 'No groups listed.', ru: 'Групп в списке нет.' });
export const peopleList = msg<{ name: string }>({
  en: ({ name }) => `People of “${name}”`,
  ru: ({ name }) => `Люди набора «${name}»`,
});
export const groupList = msg<{ name: string }>({
  en: ({ name }) => `Groups of “${name}”`,
  ru: ({ name }) => `Группы набора «${name}»`,
});
export const removeMember = msg<{ name: string }>({
  en: ({ name }) => `Remove ${name} from the set`,
  ru: ({ name }) => `Убрать ${name} из набора`,
});
export const peopleAdded = msg<{ count: number }>({
  en: ({ count }) => plural('en', count, { one: '# person added', other: '# people added' }),
  ru: ({ count }) =>
    plural('ru', count, {
      one: 'Добавлен # человек',
      few: 'Добавлено # человека',
      many: 'Добавлено # человек',
      other: 'Добавлено # человека',
    }),
});
export const groupsAdded = msg<{ count: number }>({
  en: ({ count }) => plural('en', count, { one: '# group added', other: '# groups added' }),
  ru: ({ count }) =>
    plural('ru', count, {
      one: 'Добавлена # группа',
      few: 'Добавлено # группы',
      many: 'Добавлено # групп',
      other: 'Добавлено # группы',
    }),
});
export const memberRemoved = msg<{ name: string }>({
  en: ({ name }) => `${name} removed from the set`,
  ru: ({ name }) => `${name}: убрано из набора`,
});
export const addPeopleTitle = msg<{ name: string }>({
  en: ({ name }) => `Add people to “${name}”`,
  ru: ({ name }) => `Добавить людей в «${name}»`,
});
export const addGroupsTitle = msg<{ name: string }>({
  en: ({ name }) => `Add groups to “${name}”`,
  ru: ({ name }) => `Добавить группы в «${name}»`,
});
export const peopleToAdd = msg({ en: 'People to add', ru: 'Кого добавить' });
export const groupsToAdd = msg({ en: 'Groups to add', ru: 'Какие группы добавить' });
export const findGroups = msg({ en: 'Find groups', ru: 'Найти группы' });
export const findGroupsHint = msg({ en: 'Part of a name.', ru: 'Часть названия.' });
export const noGroupsFound = msg({ en: 'No groups found.', ru: 'Группы не найдены.' });
export const chooseSomeone = msg({
  en: 'Choose at least one person.',
  ru: 'Выберите хотя бы одного человека.',
});
export const chooseGroups = msg({
  en: 'Choose at least one group.',
  ru: 'Выберите хотя бы одну группу.',
});
export const listedAlready = msg({ en: 'Listed already', ru: 'Уже в списке' });
export const choose = msg({ en: 'Choose', ru: 'Выбрать' });

// ---- A project's fields (FLD-051…FLD-060) ----

export const fieldsTitle = msg({ en: 'Fields', ru: 'Поля' });
export const projectFieldsIntro = msg({
  en: 'Fields hold the details of each issue in this project. Issues and the new-issue form show them in this order.',
  ru: 'Поля хранят подробности каждой задачи проекта. В задачах и в форме новой задачи они идут в этом порядке.',
});
export const addField = msg({ en: 'Add field', ru: 'Добавить поле' });
export const projectFieldList = msg({ en: 'Fields of the project', ru: 'Поля проекта' });
export const noProjectFields = msg({ en: 'No fields yet', ru: 'Полей пока нет' });
export const noProjectFieldsText = msg({
  en: 'Add the fields issues of this project need: priority, type, assignee and others.',
  ru: 'Добавьте поля, которые нужны задачам проекта: приоритет, тип, исполнителя и другие.',
});
export const fieldActions = msg<{ name: string }>({
  en: ({ name }) => `Actions for “${name}”`,
  ru: ({ name }) => `Действия с полем «${name}»`,
});
export const settings = msg({ en: 'Settings', ru: 'Настройки' });
export const fieldSettings = msg<{ name: string }>({
  en: ({ name }) => `Settings of “${name}”`,
  ru: ({ name }) => `Настройки поля «${name}»`,
});
export const closeSettings = msg({ en: 'Close settings', ru: 'Закрыть настройки' });
export const canBeEmptyShort = msg({ en: 'Can be empty', ru: 'Может быть пустым' });
export const emptyShown = msg<{ label: string }>({
  en: ({ label }) => `Empty: “${label}”`,
  ru: ({ label }) => `Пустое: «${label}»`,
});
export const cannotBeEmpty = msg({ en: 'Cannot be empty', ru: 'Не может быть пустым' });
export const severalStates = msg({
  en: 'This project has several state fields. An issue counts as resolved only when each of them holds a resolved value.',
  ru: 'В проекте несколько полей-состояний. Задача считается решённой, только когда в каждом из них решённое значение.',
});
export const sharedProperties = msg({
  en: 'Name and aliases are shared by every project using the field.',
  ru: 'Название и синонимы общие для всех проектов с этим полем.',
});
export const openInAdministration = msg({
  en: 'Open in administration',
  ru: 'Открыть в администрировании',
});
export const settingsInProject = msg({ en: 'In this project', ru: 'В этом проекте' });
export const settingsSaved = msg({ en: 'Settings saved', ru: 'Настройки сохранены' });
export const settingsConflict = msg({
  en: 'Someone changed these settings at the same time. Your choices are kept: check them and save again.',
  ru: 'Кто-то изменил эти настройки одновременно с вами. Ваш выбор сохранён: проверьте его и сохраните снова.',
});
export const fieldsLoading = msg({ en: 'Loading fields…', ru: 'Загрузка полей…' });
export const detach = msg({ en: 'Remove from project', ru: 'Убрать из проекта' });
export const detachTitle = msg<{ name: string }>({
  en: ({ name }) => `Remove “${name}” from the project?`,
  ru: ({ name }) => `Убрать поле «${name}» из проекта?`,
});
export const detachText = msg({
  en: 'Its values in the project’s issues are erased. Adding the field again does not bring them back.',
  ru: 'Его значения в задачах проекта будут стёрты. Если добавить поле снова, они не вернутся.',
});
export const detachIssues = msg<{ count: number }>({
  en: ({ count }) =>
    plural('en', count, {
      one: 'Its values are erased in # issue of the project. Adding the field again does not bring them back.',
      other:
        'Its values are erased in # issues of the project. Adding the field again does not bring them back.',
    }),
  ru: ({ count }) =>
    plural('ru', count, {
      one: 'Его значения будут стёрты в # задаче проекта. Если добавить поле снова, они не вернутся.',
      few: 'Его значения будут стёрты в # задачах проекта. Если добавить поле снова, они не вернутся.',
      many: 'Его значения будут стёрты в # задачах проекта. Если добавить поле снова, они не вернутся.',
      other:
        'Его значения будут стёрты в # задачи проекта. Если добавить поле снова, они не вернутся.',
    }),
});
export const detachNoIssues = msg({
  en: 'No issue of the project has a value in it, so nothing is lost.',
  ru: 'Ни в одной задаче проекта оно не заполнено, так что ничего не потеряется.',
});
export const impactLoading = msg({
  en: 'Counting issues…',
  ru: 'Считаем задачи…',
});
export const detachButton = msg({ en: 'Remove field', ru: 'Убрать поле' });
export const detached = msg({ en: 'Field removed from the project', ru: 'Поле убрано из проекта' });

// ---- Adding a field to a project (FLD-052, FLD-053) ----

export const whatToAdd = msg({ en: 'What to add', ru: 'Что добавить' });
export const existingField = msg({ en: 'An existing field', ru: 'Существующее поле' });
export const existingFieldHint = msg({
  en: 'A field other projects may use, with its name and aliases.',
  ru: 'Поле, которое могут использовать и другие проекты, с его названием и синонимами.',
});
export const newField = msg({ en: 'A new field', ru: 'Новое поле' });
export const newFieldHint = msg({
  en: 'Created for this project; other projects can add it later.',
  ru: 'Создаётся для этого проекта; другие проекты смогут добавить его позже.',
});
export const findFields = msg({ en: 'Find fields', ru: 'Найти поля' });
export const findFieldsHint = msg({
  en: 'Part of a name or an alias.',
  ru: 'Часть названия или синоним.',
});
export const fieldsLegend = msg({ en: 'Field', ru: 'Поле' });
export const noFieldsFound = msg({ en: 'No fields found.', ru: 'Поля не найдены.' });
export const inProject = msg({ en: 'In this project', ru: 'Уже в проекте' });
export const nameInProject = msg({
  en: 'The project has a field with this name',
  ru: 'В проекте уже есть поле с таким названием',
});
export const chooseField = msg({ en: 'Choose a field.', ru: 'Выберите поле.' });
export const setUse = msg({ en: 'Values in this project', ru: 'Значения в этом проекте' });
export const shareValues = msg({ en: 'Share the field’s values', ru: 'Общие значения поля' });
export const shareValuesHint = msg({
  en: 'Changes to the values apply to every project sharing them.',
  ru: 'Изменения значений коснутся всех проектов, где они общие.',
});
export const copyValues = msg({
  en: 'Copy them for this project',
  ru: 'Копия для этого проекта',
});
export const copyValuesHint = msg({
  en: 'The project gets its own set to change freely.',
  ru: 'У проекта будет свой набор, который можно менять свободно.',
});
export const createField = msg({ en: 'Create field', ru: 'Создать поле' });
export const attached = msg({ en: 'Field added to the project', ru: 'Поле добавлено в проект' });
export const createdInProject = msg({
  en: 'Field created and added to the project',
  ru: 'Поле создано и добавлено в проект',
});
export const nameTakenInProject = msg({
  en: 'This project already has a field with this name.',
  ru: 'В проекте уже есть поле с таким названием.',
});
export const fieldNameTaken = msg({
  en: 'A field of this type with this name exists already.',
  ru: 'Поле этого типа с таким названием уже есть.',
});
export const fieldNameInvalid = msg({
  en: 'A name cannot contain <, >, / or :.',
  ru: 'Название не может содержать <, >, / и :.',
});
export const attachConflict = msg({
  en: 'The field is in the project already, or the project has the most fields it can have: 100.',
  ru: 'Поле уже есть в проекте, или в проекте уже максимум полей: 100.',
});
export const tooManyAliases = msg({
  en: 'Up to 10 aliases of up to 50 characters, not repeating the name.',
  ru: 'До 10 синонимов длиной до 50 символов, не повторяющих название.',
});

// ---- Fields of the instance (FLD-015…FLD-025, FLD-070) ----

export const fieldsIntro = msg({
  en: 'Fields of the whole TaskMaster. Each project chooses which of them it uses and sets them up its own way; new projects start with the order shown here.',
  ru: 'Поля всего TaskMaster. Каждый проект выбирает, какие из них использовать, и настраивает их по-своему; новые проекты начинают с этого порядка.',
});
export const onlyUnused = msg({ en: 'Show only unused', ru: 'Только неиспользуемые' });
export const usedInCount = msg<{ count: number }>({
  en: ({ count }) => plural('en', count, { one: 'Used in # project', other: 'Used in # projects' }),
  ru: ({ count }) =>
    plural('ru', count, {
      one: 'Используется в # проекте',
      few: 'Используется в # проектах',
      many: 'Используется в # проектах',
      other: 'Используется в # проекта',
    }),
});
export const notUsed = msg({
  en: 'Not used in any project',
  ru: 'Не используется ни в одном проекте',
});
export const fieldList = msg({ en: 'Fields', ru: 'Поля' });
export const noFieldsMatch = msg({ en: 'No fields found', ru: 'Поля не найдены' });
export const noFieldsMatchText = msg({
  en: 'Try another name or alias, or show used fields too.',
  ru: 'Попробуйте другое название или синоним, или покажите и используемые поля.',
});
export const reorderHint = msg({
  en: 'Clear the search and filters to change the order.',
  ru: 'Чтобы менять порядок, очистите поиск и фильтры.',
});
export const fieldCreated = msg({ en: 'Field created', ru: 'Поле создано' });
export const showMore = msg({ en: 'Show more', ru: 'Показать ещё' });
export const newFieldTitle = msg({ en: 'New field', ru: 'Новое поле' });
export const valuesSource = msg({ en: 'Values', ru: 'Значения' });
export const newSet = msg({ en: 'A new set', ru: 'Новый набор' });
export const newSetHint = msg({
  en: 'Starts empty; add values on the set’s page.',
  ru: 'Сначала пустой; значения добавляются на странице набора.',
});
export const existingSet = msg({ en: 'An existing set', ru: 'Существующий набор' });
export const existingSetHint = msg({
  en: 'Values other fields use; default values can be chosen from it.',
  ru: 'Значения, которые используют другие поля; из них можно выбрать значения по умолчанию.',
});
export const setName = msg({ en: 'Name of the set', ru: 'Название набора' });
export const setNameHint = msg({
  en: 'Leave it empty to name the set after the field.',
  ru: 'Оставьте пустым, чтобы набор назывался как поле.',
});
export const set = msg({ en: 'Set', ru: 'Набор' });
export const chooseSet = msg({ en: 'Choose a set', ru: 'Выберите набор' });
export const noSetsOfKind = msg({
  en: 'There are no sets of this type yet.',
  ru: 'Наборов этого типа пока нет.',
});
export const startingSettings = msg({
  en: 'Settings for new projects',
  ru: 'Настройки для новых проектов',
});
export const startingHint = msg({
  en: 'A project starts with these settings when it adds the field, and can change them. Projects that use the field already keep their own.',
  ru: 'С этими настройками поле попадает в проект, и проект может их изменить. Проекты, где поле уже есть, сохраняют свои.',
});
export const nameAndAliases = msg({ en: 'Name and aliases', ru: 'Название и синонимы' });
export const sharedHint = msg({
  en: 'Shared by every project using the field: a change applies to all of them.',
  ru: 'Общие для всех проектов с этим полем: изменение коснётся всех.',
});
export const autoAttachedRename = msg({
  en: 'Only a system administrator can rename a field that is added to new projects.',
  ru: 'Переименовать поле, которое добавляется в новые проекты, может только системный администратор.',
});
export const projectsTitle = msg({ en: 'Projects', ru: 'Проекты' });
export const noProjects = msg({
  en: 'No project uses this field.',
  ru: 'Это поле не используется ни в одном проекте.',
});
export const projectsOfField = msg<{ name: string }>({
  en: ({ name }) => `Projects using “${name}”`,
  ru: ({ name }) => `Проекты с полем «${name}»`,
});
// The boards of the field (BRD-022), with the wording of the board's own settings.
export const boardsTitle = msg({ en: 'Boards', ru: 'Доски' });
export const noFieldBoards = msg({
  en: 'No board uses this field.',
  ru: 'Это поле не используется ни на одной доске.',
});
export const fieldBoardsLabel = msg<{ name: string }>({
  en: ({ name }) => `Boards using “${name}”`,
  ru: ({ name }) => `Доски с полем «${name}»`,
});
export const fieldBoardsHidden = msg<{ count: number }>({
  en: ({ count }) =>
    plural('en', count, {
      one: '# more board, which you do not view.',
      other: '# more boards, which you do not view.',
    }),
  ru: ({ count }) =>
    plural('ru', count, {
      one: 'Ещё # доска, которую вы не просматриваете.',
      few: 'Ещё # доски, которые вы не просматриваете.',
      many: 'Ещё # досок, которые вы не просматриваете.',
      other: 'Ещё # доски, которые вы не просматриваете.',
    }),
});
export const boardUses = {
  column: msg({ en: 'columns', ru: 'колонки' }),
  lane: msg({ en: 'lanes', ru: 'дорожки' }),
  color: msg({ en: 'card color', ru: 'цвет карточек' }),
  estimation: msg({ en: 'estimation', ru: 'оценка' }),
  original_estimation: msg({ en: 'original estimation', ru: 'исходная оценка' }),
  cards: msg({ en: 'fields on cards', ru: 'поля на карточке' }),
} as const;
export const deleteFieldBoards = msg<{ count: number }>({
  en: ({ count }) =>
    plural('en', count, {
      one: '# board stops using it.',
      other: '# boards stop using it.',
    }),
  ru: ({ count }) =>
    plural('ru', count, {
      one: '# доска перестанет его использовать.',
      few: '# доски перестанут его использовать.',
      many: '# досок перестанут его использовать.',
      other: '# доски перестанут его использовать.',
    }),
});
export const fieldSaved = msg({ en: 'Field saved', ru: 'Поле сохранено' });
export const fieldConflict = msg({
  en: 'Someone changed the field at the same time. It shows their changes now; save again to apply yours.',
  ru: 'Кто-то изменил поле одновременно с вами. Сейчас показаны его изменения; сохраните снова, чтобы применить свои.',
});
export const deleteField = msg({ en: 'Delete field', ru: 'Удалить поле' });
export const deleteFieldTitle = msg<{ name: string }>({
  en: ({ name }) => `Delete “${name}”?`,
  ru: ({ name }) => `Удалить поле «${name}»?`,
});
export const deleteFieldImpact = msg<{ count: number }>({
  en: ({ count }) =>
    plural('en', count, {
      one: 'It is removed from # project.',
      other: 'It is removed from # projects.',
    }),
  ru: ({ count }) =>
    plural('ru', count, {
      one: 'Поле будет убрано из # проекта.',
      few: 'Поле будет убрано из # проектов.',
      many: 'Поле будет убрано из # проектов.',
      other: 'Поле будет убрано из # проекта.',
    }),
});
export const deleteFieldIssues = msg<{ count: number }>({
  en: ({ count }) =>
    plural('en', count, {
      one: 'Its values are erased in # issue.',
      other: 'Its values are erased in # issues.',
    }),
  ru: ({ count }) =>
    plural('ru', count, {
      one: 'Его значения будут стёрты в # задаче.',
      few: 'Его значения будут стёрты в # задачах.',
      many: 'Его значения будут стёрты в # задачах.',
      other: 'Его значения будут стёрты в # задачи.',
    }),
});
export const deleteFieldNoIssues = msg({
  en: 'No issue has a value in it.',
  ru: 'Ни в одной задаче оно не заполнено.',
});
export const deleteFieldUnused = msg({
  en: 'No project uses it.',
  ru: 'Оно не используется ни в одном проекте.',
});
export const cannotUndo = msg({ en: 'This cannot be undone.', ru: 'Это нельзя отменить.' });
export const fieldDeleted = msg({ en: 'Field deleted', ru: 'Поле удалено' });
export const openSet = msg<{ name: string }>({
  en: ({ name }) => `Open the set “${name}”`,
  ru: ({ name }) => `Открыть набор «${name}»`,
});

// ---- Value sets of the instance (FLD-025, FLD-029) ----

export const setsTitle = msg({ en: 'Value sets', ru: 'Наборы значений' });
export const setsIntro = msg({
  en: 'A set holds the values of fields: priorities, states, versions. Several fields and projects can share one set.',
  ru: 'Набор хранит значения полей: приоритеты, состояния, версии. Один набор могут использовать несколько полей и проектов.',
});
export const createSet = msg({ en: 'Create set', ru: 'Создать набор' });
export const newSetTitle = msg({ en: 'New value set', ru: 'Новый набор значений' });
export const findSets = msg({ en: 'Find sets', ru: 'Найти наборы' });
export const findSetsHint = msg({ en: 'Part of a name.', ru: 'Часть названия.' });
export const anyType = msg({ en: 'Any type', ru: 'Любой тип' });
export const setList = msg({ en: 'Value sets', ru: 'Наборы значений' });
export const noSetsMatch = msg({ en: 'No sets found', ru: 'Наборы не найдены' });
export const noSetsMatchText = msg({
  en: 'Try another name or type.',
  ru: 'Попробуйте другое название или тип.',
});
export const setCreated = msg({ en: 'Set created', ru: 'Набор создан' });
export const setNameTaken = msg({
  en: 'Another set of this type has this name.',
  ru: 'Набор этого типа с таким названием уже есть.',
});
export const renameSetTitle = msg({ en: 'Rename the set', ru: 'Переименовать набор' });
export const setRenamed = msg({ en: 'Set renamed', ru: 'Набор переименован' });
export const renameConflict = msg({
  en: 'Someone renamed the set at the same time. Save again to keep your name.',
  ru: 'Кто-то переименовал набор одновременно с вами. Сохраните снова, чтобы оставить своё название.',
});
export const deleteSet = msg({ en: 'Delete set', ru: 'Удалить набор' });
export const deleteSetTitle = msg<{ name: string }>({
  en: ({ name }) => `Delete “${name}”?`,
  ru: ({ name }) => `Удалить набор «${name}»?`,
});
export const deleteSetText = msg({
  en: 'No field or project uses this set. Deleting it cannot be undone.',
  ru: 'Этот набор не используют ни поля, ни проекты. Удаление нельзя отменить.',
});
export const setDeleted = msg({ en: 'Set deleted', ru: 'Набор удалён' });
export const setInUse = msg({
  en: 'A set that fields or projects use cannot be deleted.',
  ru: 'Набор, который используют поля или проекты, удалить нельзя.',
});

// ---- Refusals ----

export const missingPermissions = msg<{ names: string }>({
  en: ({ names }) => `You need more rights for this: ${names}.`,
  ru: ({ names }) => `Для этого нужны права: ${names}.`,
});
