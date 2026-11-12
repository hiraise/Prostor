// Texts of the link types screens (LNK-018…LNK-021). Voice and glossary: docs/design.md §12.
// Russian plurals put a non-breaking space between the number and the word.
import type { LinkDirection } from '#lib/api/schema.gen.ts';
import { msg, plural } from '#lib/i18n/message.ts';

// ---- The list (LNK-018) ----

export const linkTypesTitle = msg({ en: 'Link types', ru: 'Типы связей' });
export const linkTypesIntro = msg({
  en: 'Links connect issues. Each type has a name for administrators and the words issues use for each other; those words are also used in searches and commands.',
  ru: 'Связи соединяют задачи. У каждого типа есть название для администраторов и слова, которыми задачи называют друг друга; эти же слова используются в поиске и командах.',
});
export const readOnlyNote = msg({
  en: 'Only system administrators can change link types.',
  ru: 'Изменять типы связей могут только системные администраторы.',
});
export const linkTypeList = msg({ en: 'Link types', ru: 'Типы связей' });
export const createLinkType = msg({ en: 'Create link type', ru: 'Создать тип связи' });
export const noLinkTypes = msg({ en: 'No link types yet', ru: 'Типов связей пока нет' });
export const noLinkTypesText = msg({
  en: 'Create a type to link issues to each other.',
  ru: 'Создайте тип, чтобы связывать задачи друг с другом.',
});
export const builtIn = msg({ en: 'Built-in', ru: 'Встроенный' });
export const locked = msg({ en: 'Cannot be changed', ru: 'Нельзя изменить' });

export const directionNames: Record<LinkDirection, () => string> = {
  undirected: msg({ en: 'Undirected', ru: 'Без направления' }),
  directed: msg({ en: 'Directed', ru: 'Направленная' }),
  hierarchy: msg({ en: 'Hierarchy', ru: 'Иерархия' }),
};

export const bothSides = msg<{ name: string }>({
  en: ({ name }) => `Both sides: “${name}”`,
  ru: ({ name }) => `Обе стороны: «${name}»`,
});
export const twoSides = msg<{ source: string; target: string }>({
  en: ({ source, target }) => `Sides: “${source}” and “${target}”`,
  ru: ({ source, target }) => `Стороны: «${source}» и «${target}»`,
});
export const usageCount = msg<{ count: number }>({
  en: ({ count }) =>
    count === 0 ? 'No links' : plural('en', count, { one: '# link', other: '# links' }),
  ru: ({ count }) =>
    count === 0
      ? 'Нет связей'
      : plural('ru', count, {
          one: '# связь',
          few: '# связи',
          many: '# связей',
          other: '# связи',
        }),
});
export const typeActions = msg<{ name: string }>({
  en: ({ name }) => `Actions for “${name}”`,
  ru: ({ name }) => `Действия с «${name}»`,
});
export const edit = msg({ en: 'Edit', ru: 'Изменить' });
export const remove = msg({ en: 'Delete', ru: 'Удалить' });

// ---- Deleted types (LNK-021) ----

export const deletedTitle = msg({ en: 'Deleted types', ru: 'Удалённые типы' });
export const deletedIntro = msg({
  en: 'Links of a deleted type are hidden from issues. Restore the type to bring them back.',
  ru: 'Связи удалённого типа скрыты в задачах. Восстановите тип, чтобы вернуть их.',
});
export const restore = msg({ en: 'Restore', ru: 'Восстановить' });
export const restoreNamed = msg<{ name: string }>({
  en: ({ name }) => `Restore “${name}”`,
  ru: ({ name }) => `Восстановить «${name}»`,
});
export const restored = msg({ en: 'Link type restored', ru: 'Тип связи восстановлен' });
export const restoreTaken = msg({
  en: 'Another type uses one of its names now. Rename that type first, then restore this one.',
  ru: 'Одно из его названий теперь занято другим типом. Сначала переименуйте тот тип, затем восстановите этот.',
});

// ---- Deleting (LNK-021) ----

export const deleteTitle = msg<{ name: string }>({
  en: ({ name }) => `Delete “${name}”?`,
  ru: ({ name }) => `Удалить тип связи «${name}»?`,
});
export const deleteHides = msg<{ count: number }>({
  en: ({ count }) =>
    count === 0
      ? 'No issue is linked with this type.'
      : plural('en', count, {
          one: '# link of this type is hidden from issues.',
          other: '# links of this type are hidden from issues.',
        }),
  ru: ({ count }) =>
    count === 0
      ? 'Ни одна задача не связана этим типом.'
      : plural('ru', count, {
          one: '# связь этого типа будет скрыта в задачах.',
          few: '# связи этого типа будут скрыты в задачах.',
          many: '# связей этого типа будут скрыты в задачах.',
          other: '# связи этого типа будут скрыты в задачах.',
        }),
});
export const deleteRestorable = msg({
  en: 'The type moves to “Deleted types”; restore it there to bring its links back.',
  ru: 'Тип попадёт в «Удалённые типы»; восстановите его там, чтобы вернуть связи.',
});
export const countingLinks = msg({ en: 'Counting links…', ru: 'Считаем связи…' });
export const deleteLinkType = msg({ en: 'Delete type', ru: 'Удалить тип' });
export const deleted = msg({ en: 'Link type deleted', ru: 'Тип связи удалён' });

// ---- Creating and editing (LNK-019, LNK-020) ----

export const newTitle = msg({ en: 'New link type', ru: 'Новый тип связи' });
export const editTitle = msg<{ name: string }>({
  en: ({ name }) => `Edit “${name}”`,
  ru: ({ name }) => `Изменить «${name}»`,
});
export const name = msg({ en: 'Name', ru: 'Название' });
export const nameHint = msg({
  en: 'What administrators call the type: “Dependency”.',
  ru: 'Как тип называют администраторы: «Зависимость».',
});
export const direction = msg({ en: 'Direction', ru: 'Направление' });
export const directionHints: Record<LinkDirection, () => string> = {
  undirected: msg({
    en: 'Both issues use the same words.',
    ru: 'Обе задачи называют связь одинаково.',
  }),
  directed: msg({
    en: 'Each side has its own words: one issue blocks, the other is blocked.',
    ru: 'У каждой стороны свои слова: одна задача блокирует, другая заблокирована.',
  }),
  hierarchy: msg({
    en: 'Like directed, but an issue has at most one parent and links never form a loop.',
    ru: 'Как направленная, но у задачи не больше одного родителя, а связи не образуют петлю.',
  }),
};
export const sourceName = msg({ en: 'Source side', ru: 'Прямое название' });
export const sourceHint = msg({
  en: 'How an issue refers to the one it links to: “blocks”.',
  ru: 'Как задача называет ту, с которой связана: «блокирует».',
});
export const sharedName = msg({ en: 'Link name', ru: 'Название связи' });
export const sharedHint = msg({
  en: 'Both issues use it: “related to”.',
  ru: 'Его используют обе задачи: «связана с».',
});
export const targetName = msg({ en: 'Target side', ru: 'Обратное название' });
export const targetHint = msg({
  en: 'How the linked issue refers back: “blocked by”.',
  ru: 'Как связанная задача называет исходную: «заблокирована».',
});
export const sameSides = msg({
  en: 'The two sides need different names.',
  ru: 'У сторон должны быть разные названия.',
});
export const nameInvalid = msg({
  en: 'One line without <, >, / or :.',
  ru: 'Одна строка без символов <, >, / и :.',
});
export const typeNameTaken = msg({
  en: 'Another link type has this name.',
  ru: 'Тип связи с таким названием уже есть.',
});
export const sideNameTaken = msg({
  en: 'Another link type uses these words.',
  ru: 'Эти слова уже использует другой тип связи.',
});
export const renameWarning = msg({
  en: 'New words apply to every link of this type at once, and searches and commands use them instead of the old ones.',
  ru: 'Новые слова сразу появятся у всех связей этого типа, а поиск и команды будут понимать их вместо старых.',
});
export const undirectedWarning = msg<{ name: string }>({
  en: ({ name }) =>
    `Without a direction the target side “${name}” is dropped: both issues of every link use the source side.`,
  ru: ({ name }) =>
    `Без направления обратное название «${name}» удалится: обе задачи каждой связи будут использовать прямое.`,
});
export const editConflict = msg({
  en: 'Someone else changed this type meanwhile. Save again to keep your version.',
  ru: 'Кто-то изменил этот тип одновременно с вами. Сохраните ещё раз, чтобы оставить свой вариант.',
});
export const create = msg({ en: 'Create type', ru: 'Создать тип' });
export const save = msg({ en: 'Save', ru: 'Сохранить' });
export const created = msg({ en: 'Link type created', ru: 'Тип связи создан' });
export const saved = msg({ en: 'Link type saved', ru: 'Тип связи сохранён' });
