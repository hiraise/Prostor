// Texts shared by the project pages (PRJ). Each page keeps its own texts next to it.
// Voice and glossary: docs/design.md §12.
import { msg, plural } from '#lib/i18n/message.ts';

export const projects = msg({ en: 'Projects', ru: 'Проекты' });
export const overview = msg({ en: 'Overview', ru: 'Обзор' });
export const issues = msg({ en: 'Issues', ru: 'Задачи' });
export const boards = msg({ en: 'Boards', ru: 'Доски' });
export const settings = msg({ en: 'Settings', ru: 'Настройки' });
export const projectNavigation = msg({ en: 'Project', ru: 'Проект' });
export const settingsNavigation = msg({ en: 'Project settings', ru: 'Настройки проекта' });
export const general = msg({ en: 'General', ru: 'Общие' });
export const people = msg({ en: 'People', ru: 'Люди' });
export const fields = msg({ en: 'Fields', ru: 'Поля' });
export const rules = msg({ en: 'Rules', ru: 'Правила' });
export const archived = msg({ en: 'Archived', ru: 'В архиве' });
export const archivedNotice = msg({
  en: 'This project is archived: it is read-only and does not appear in pickers and searches. Restore it to change it.',
  ru: 'Проект в архиве: он доступен только для чтения и не появляется в выборе проектов и в поиске. Чтобы изменить его, восстановите проект.',
});
export const star = msg({ en: 'Add to favorites', ru: 'Добавить в избранное' });
export const unstar = msg({ en: 'Remove from favorites', ru: 'Убрать из избранного' });
export const starFailed = msg({
  en: 'The favorites did not change.',
  ru: 'Избранное не изменилось.',
});
export const keyLabel = msg({ en: 'Key', ru: 'Ключ' });

// ---- Facts shown in the list, on the overview and in settings (PRJ-008, PRJ-014) ----

export const nameLabel = msg({ en: 'Name', ru: 'Название' });
export const descriptionLabel = msg({ en: 'Description', ru: 'Описание' });
export const descriptionHint = msg({
  en: 'What the project is for. Shown on its overview.',
  ru: 'Для чего этот проект. Видно на его обзоре.',
});
export const owner = msg({ en: 'Owner', ru: 'Владелец' });
export const created = msg({ en: 'Created', ru: 'Создан' });
export const team = msg({ en: 'Team', ru: 'Команда' });
export const formerKeys = msg({ en: 'Former keys', ru: 'Прежние ключи' });
export const teamSize = msg<{ count: number }>({
  en: ({ count }) =>
    plural('en', count, { one: '# person in the team', other: '# people in the team' }),
  ru: ({ count }) =>
    plural('ru', count, {
      one: '# человек в команде',
      few: '# человека в команде',
      many: '# человек в команде',
      other: '# человека в команде',
    }),
});
export const templateNames = {
  blank: msg({ en: 'Blank', ru: 'Пустой' }),
  scrum: msg({ en: 'Scrum', ru: 'Scrum' }),
  kanban: msg({ en: 'Kanban', ru: 'Канбан' }),
} as const;

// ---- Actions on a project (PRJ-011, PRJ-061, PRJ-063, PRJ-064) ----

export const projectActions = msg<{ name: string }>({
  en: ({ name }) => `Actions for “${name}”`,
  ru: ({ name }) => `Действия с проектом «${name}»`,
});
export const openProject = msg({ en: 'Open project', ru: 'Открыть проект' });
export const projectSettings = msg({ en: 'Project settings', ru: 'Настройки проекта' });
export const archiveProject = msg({ en: 'Archive project', ru: 'Архивировать проект' });
export const restoreProject = msg({ en: 'Restore project', ru: 'Восстановить проект' });
export const deleteProject = msg({ en: 'Delete project', ru: 'Удалить проект' });
export const projectArchived = msg<{ name: string }>({
  en: ({ name }) => `Project “${name}” archived`,
  ru: ({ name }) => `Проект «${name}» архивирован`,
});
export const projectRestored = msg<{ name: string }>({
  en: ({ name }) => `Project “${name}” restored`,
  ru: ({ name }) => `Проект «${name}» восстановлен`,
});
export const projectDeleting = msg<{ name: string }>({
  en: ({ name }) => `Project “${name}” is being deleted`,
  ru: ({ name }) => `Проект «${name}» удаляется`,
});

// ---- Archiving (PRJ-061) ----

export const archiveTitle = msg<{ name: string }>({
  en: ({ name }) => `Archive “${name}”?`,
  ru: ({ name }) => `Архивировать проект «${name}»?`,
});
export const archiveEffects = msg({ en: 'What happens', ru: 'Что произойдёт' });
export const archiveReadOnly = msg({
  en: 'The project becomes read-only: no new issues or articles, and its settings do not change.',
  ru: 'Проект станет доступен только для чтения: в нём нельзя создавать задачи и статьи и менять настройки.',
});
export const archiveHidden = msg({
  en: 'It leaves project pickers and searches; the projects list shows it under “Archived”.',
  ru: 'Он пропадёт из выбора проектов и из поиска; в списке проектов он будет в разделе «В архиве».',
});
export const archiveReversible = msg({
  en: 'Nothing is deleted. Restore the project at any time to bring everything back.',
  ru: 'Ничего не удаляется. Восстановите проект в любой момент, и всё вернётся.',
});

// ---- Deleting (PRJ-064, PRJ-065) ----

export const deleteTitle = msg<{ name: string }>({
  en: ({ name }) => `Delete “${name}”?`,
  ru: ({ name }) => `Удалить проект «${name}»?`,
});
export const deleteIssueCount = msg<{ count: number }>({
  en: ({ count }) =>
    plural('en', count, {
      one: 'The project has # issue.',
      other: 'The project has # issues.',
    }),
  ru: ({ count }) =>
    plural('ru', count, {
      one: 'В проекте # задача.',
      few: 'В проекте # задачи.',
      many: 'В проекте # задач.',
      other: 'В проекте # задачи.',
    }),
});
export const deleteForever = msg({
  en: 'Every issue and article of the project and all its settings are deleted for good. This cannot be undone: deleted projects do not go to the trash.',
  ru: 'Все задачи и статьи проекта и все его настройки будут удалены навсегда. Это нельзя отменить: удалённые проекты не попадают в корзину.',
});
export const deleteInBackground = msg({
  en: 'Deletion runs in the background. From now on nobody can open the project; the projects list shows how the deletion goes under “Being deleted”.',
  ru: 'Удаление идёт в фоне. С этого момента проект никто не сможет открыть; ход удаления виден в списке проектов в разделе «Удаляются».',
});
export const typeName = msg({
  en: 'Type the project name to confirm',
  ru: 'Введите название проекта для подтверждения',
});
export const typeNameHint = msg<{ name: string }>({
  en: ({ name }) => `Exactly as shown: ${name}`,
  ru: ({ name }) => `В точности так: ${name}`,
});
export const nameMismatch = msg({
  en: 'The name does not match.',
  ru: 'Название не совпадает.',
});

// ---- Problems of a name and a key (PRJ-002, PRJ-003, PRJ-018) ----

export const nameTaken = msg({
  en: 'Another project has this name.',
  ru: 'Проект с таким названием уже есть.',
});
export const keyTaken = msg({
  en: 'Another project has or had this key.',
  ru: 'Этот ключ есть или был у другого проекта.',
});
export const keyInvalid = msg({
  en: 'Use Latin letters, digits and _, starting with a letter.',
  ru: 'Используйте латинские буквы, цифры и _, первая — буква.',
});
export const keyLength = msg({
  en: 'Use 2 to 10 characters.',
  ru: 'Нужно от 2 до 10 символов.',
});
export const firstNumberRange = msg<{ max: string }>({
  en: ({ max }) => `Use a whole number from 1 to ${max}.`,
  ru: ({ max }) => `Нужно целое число от 1 до ${max}.`,
});
