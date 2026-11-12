// Texts of the issue list's sidebar (QRY-082, ISS-011, ISS-036, QRY-064, QRY-065, ISS-037).
// Voice and glossary: docs/design.md §12.
import { msg, plural } from '#lib/i18n/message.ts';

export const sidebar = msg({ en: 'Lists', ru: 'Разделы' });
export const sidebarWidth = msg({ en: 'Width of the lists', ru: 'Ширина разделов' });
export const sidebarMode = msg({ en: 'How the lists are shown', ru: 'Как показывать разделы' });
export const modeOpen = msg({ en: 'Always shown', ru: 'Всегда показывать' });
export const modeAuto = msg({
  en: 'Shown on pointing or focus',
  ru: 'Показывать при наведении',
});
export const modeClosed = msg({ en: 'Hidden', ru: 'Скрыть' });
export const loadFailed = msg({
  en: 'The lists did not load.',
  ru: 'Разделы не загрузились.',
});
export const retry = msg({ en: 'Try again', ru: 'Повторить' });

export const drafts = msg({ en: 'Drafts', ru: 'Черновики' });
export const projects = msg({ en: 'Projects', ru: 'Проекты' });
export const tags = msg({ en: 'Tags', ru: 'Теги' });
export const searches = msg({ en: 'Saved searches', ru: 'Сохранённые поиски' });
export const noProjects = msg({ en: 'No projects', ru: 'Нет проектов' });
export const noTags = msg({ en: 'No tags yet', ru: 'Тегов пока нет' });
export const noSearches = msg({ en: 'No saved searches', ru: 'Нет сохранённых поисков' });
export const count = msg<{ count: number }>({
  en: ({ count }) => plural('en', count, { one: '# issue', other: '# issues' }),
  ru: ({ count }) =>
    plural('ru', count, { one: '# задача', few: '# задачи', many: '# задач', other: '# задачи' }),
});

// Drafts (ISS-011).
export const noSummary = msg({ en: 'Untitled', ru: 'Без названия' });
export const openDraft = msg<{ summary: string }>({
  en: ({ summary }) => `Continue the draft “${summary}”`,
  ru: ({ summary }) => `Продолжить черновик «${summary}»`,
});
export const deleteDraft = msg<{ summary: string }>({
  en: ({ summary }) => `Delete the draft “${summary}”`,
  ru: ({ summary }) => `Удалить черновик «${summary}»`,
});
export const draftDeleted = msg({ en: 'Draft deleted', ru: 'Черновик удалён' });
export const deleteAllDrafts = msg({ en: 'Delete all drafts', ru: 'Удалить все черновики' });
export const deleteAllTitle = msg({ en: 'Delete all drafts?', ru: 'Удалить все черновики?' });
export const deleteAllText = msg<{ count: number }>({
  en: ({ count }) =>
    plural('en', count, {
      one: 'The draft will be deleted. This cannot be undone.',
      other: '# drafts will be deleted. This cannot be undone.',
    }),
  ru: ({ count }) =>
    plural('ru', count, {
      one: '# черновик будет удалён. Это нельзя отменить.',
      few: '# черновика будут удалены. Это нельзя отменить.',
      many: '# черновиков будут удалены. Это нельзя отменить.',
      other: '# черновика будут удалены. Это нельзя отменить.',
    }),
});
export const draftsDeleted = msg({ en: 'Drafts deleted', ru: 'Черновики удалены' });

// Actions on tags and searches.
export const actionsOf = msg<{ name: string }>({
  en: ({ name }) => `Actions: ${name}`,
  ru: ({ name }) => `Действия: ${name}`,
});
export const edit = msg({ en: 'Edit', ru: 'Изменить' });
export const share = msg({ en: 'Sharing…', ru: 'Общий доступ…' });
export const remove = msg({ en: 'Delete', ru: 'Удалить' });
export const cancel = msg({ en: 'Cancel', ru: 'Отмена' });
export const save = msg({ en: 'Save', ru: 'Сохранить' });
export const name = msg({ en: 'Name', ru: 'Название' });
export const query = msg({ en: 'Query', ru: 'Запрос' });
export const nameRequired = msg({ en: 'Enter a name.', ru: 'Введите название.' });
export const queryRequired = msg({ en: 'Enter a query.', ru: 'Введите запрос.' });
export const nameTaken = msg({
  en: 'You already have one with this name.',
  ru: 'У вас уже есть такое название.',
});
export const conflict = msg({
  en: 'Someone changed it meanwhile. Close the dialog and try again.',
  ru: 'Его уже изменили. Закройте окно и попробуйте снова.',
});

export const editTag = msg({ en: 'Edit tag', ru: 'Изменить тег' });
export const tagSaved = msg({ en: 'Tag saved', ru: 'Тег сохранён' });
export const deleteTagTitle = msg<{ name: string }>({
  en: ({ name }) => `Delete the tag “${name}”?`,
  ru: ({ name }) => `Удалить тег «${name}»?`,
});
export const tagUsage = msg<{ count: number }>({
  en: ({ count }) =>
    count === 0
      ? 'No issue carries it. This cannot be undone.'
      : plural('en', count, {
          one: 'It will be taken off # issue. This cannot be undone.',
          other: 'It will be taken off # issues. This cannot be undone.',
        }),
  ru: ({ count }) =>
    count === 0
      ? 'Его нет ни на одной задаче. Это нельзя отменить.'
      : plural('ru', count, {
          one: 'Он будет снят с # задачи. Это нельзя отменить.',
          few: 'Он будет снят с # задач. Это нельзя отменить.',
          many: 'Он будет снят с # задач. Это нельзя отменить.',
          other: 'Он будет снят с # задачи. Это нельзя отменить.',
        }),
});
export const tagDeleted = msg({ en: 'Tag deleted', ru: 'Тег удалён' });

export const editSearch = msg({ en: 'Edit saved search', ru: 'Изменить сохранённый поиск' });
export const searchSaved = msg({ en: 'Search saved', ru: 'Поиск сохранён' });
export const deleteSearchTitle = msg<{ name: string }>({
  en: ({ name }) => `Delete the search “${name}”?`,
  ru: ({ name }) => `Удалить поиск «${name}»?`,
});
export const deleteSearchText = msg({
  en: 'The search goes for everyone it is shared with. Issues stay.',
  ru: 'Поиск пропадёт у всех, с кем им поделились. Задачи останутся.',
});
export const searchDeleted = msg({ en: 'Search deleted', ru: 'Поиск удалён' });

// Sharing (QRY-065, ISS-037).
export const sharingOf = msg<{ name: string }>({
  en: ({ name }) => `Sharing: ${name}`,
  ru: ({ name }) => `Общий доступ: ${name}`,
});
export const sharingSaved = msg({ en: 'Sharing saved', ru: 'Доступ сохранён' });
export const tagView = msg({ en: 'Who sees the tag on issues', ru: 'Кто видит тег на задачах' });
export const tagUse = msg({
  en: 'Who puts it on issues and takes it off',
  ru: 'Кто ставит и снимает тег',
});
export const tagEdit = msg({ en: 'Who renames and recolors it', ru: 'Кто меняет название и цвет' });
export const searchView = msg({ en: 'Who sees and runs it', ru: 'Кто видит и запускает поиск' });
export const searchEdit = msg({
  en: 'Who changes its name and query',
  ru: 'Кто меняет название и запрос',
});
export const onlyMe = msg({ en: 'Only me', ru: 'Только я' });
export const chosen = msg({ en: 'Chosen people and groups', ru: 'Выбранные люди и группы' });
export const findShared = msg({ en: 'Find people and groups', ru: 'Найти людей и группы' });
export const sharedOptions = msg<{ audience: string }>({
  en: ({ audience }) => `People and groups: ${audience}`,
  ru: ({ audience }) => `Люди и группы: ${audience}`,
});
export const nobodyChosen = msg({
  en: 'Choose someone, or keep it to yourself.',
  ru: 'Выберите кого-нибудь или оставьте «Только я».',
});
export const group = msg({ en: 'group', ru: 'группа' });
