// Texts of the projects list (PRJ-010…PRJ-013, PRJ-061…PRJ-065). Voice and glossary:
// docs/design.md §12.
import { msg } from '#lib/i18n/message.ts';

export const createProject = msg({ en: 'Create project', ru: 'Создать проект' });
export const findProjects = msg({ en: 'Find projects', ru: 'Найти проекты' });
export const findProjectsHint = msg({
  en: 'Part of the name or the start of the key.',
  ru: 'Часть названия или начало ключа.',
});
export const projectList = msg({ en: 'Projects', ru: 'Проекты' });
export const showMore = msg({ en: 'Show more', ru: 'Показать ещё' });
export const ownedBy = msg<{ name: string }>({
  en: ({ name }) => `Owner: ${name}`,
  ru: ({ name }) => `Владелец: ${name}`,
});
export const starProject = msg<{ name: string }>({
  en: ({ name }) => `Add “${name}” to favorites`,
  ru: ({ name }) => `Добавить «${name}» в избранное`,
});
export const unstarProject = msg<{ name: string }>({
  en: ({ name }) => `Remove “${name}” from favorites`,
  ru: ({ name }) => `Убрать «${name}» из избранного`,
});

// ---- Empty states ----

export const noProjects = msg({ en: 'No projects yet', ru: 'Проектов пока нет' });
export const noProjectsCreate = msg({
  en: 'A project holds issues, boards and articles of one team or product. Create the first one.',
  ru: 'В проекте собраны задачи, доски и статьи одной команды или продукта. Создайте первый.',
});
export const noProjectsJoin = msg({
  en: 'Projects you are added to will appear here.',
  ru: 'Здесь появятся проекты, в которые вас добавят.',
});
export const noProjectsFound = msg({ en: 'No projects found', ru: 'Проекты не найдены' });
export const noProjectsFoundText = msg({
  en: 'Try another part of the name or key.',
  ru: 'Попробуйте другую часть названия или ключа.',
});

// ---- Archived (PRJ-013, PRJ-063) ----

export const archivedSection = msg({ en: 'Archived', ru: 'В архиве' });
export const archivedList = msg({ en: 'Archived projects', ru: 'Проекты в архиве' });
export const noArchived = msg({
  en: 'No archived projects.',
  ru: 'В архиве нет проектов.',
});
export const noArchivedFound = msg({
  en: 'No archived projects match the search.',
  ru: 'В архиве нет проектов, подходящих под поиск.',
});
export const restore = msg({ en: 'Restore', ru: 'Восстановить' });
export const restoreNamed = msg<{ name: string }>({
  en: ({ name }) => `Restore “${name}”`,
  ru: ({ name }) => `Восстановить «${name}»`,
});

// ---- Being deleted (PRJ-065) ----

export const deletingSection = msg({ en: 'Being deleted', ru: 'Удаляются' });
export const deletingIntro = msg({
  en: 'Nobody can open these projects any more. Each leaves this list once its deletion is finished.',
  ru: 'Эти проекты уже никто не может открыть. Каждый пропадёт из списка, когда его удаление завершится.',
});
export const deletingList = msg({ en: 'Projects being deleted', ru: 'Удаляемые проекты' });
export const deletionStates = {
  queued: msg({ en: 'Waiting to start', ru: 'Ждёт начала' }),
  running: msg({ en: 'Deleting…', ru: 'Удаляется…' }),
  failed: msg({ en: 'Deletion failed', ru: 'Удаление не удалось' }),
} as const;
export const deleteAgain = msg({ en: 'Delete again', ru: 'Удалить снова' });
export const deleteAgainNamed = msg<{ name: string }>({
  en: ({ name }) => `Delete again: ${name}`,
  ru: ({ name }) => `Удалить снова: ${name}`,
});
export const deletionRestarted = msg<{ name: string }>({
  en: ({ name }) => `Deletion of “${name}” started again`,
  ru: ({ name }) => `Удаление проекта «${name}» запущено снова`,
});
