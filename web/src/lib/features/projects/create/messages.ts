// Texts of the project creation page (PRJ-017…PRJ-019, PRJ-021). Voice and glossary:
// docs/design.md §12.
import { msg } from '#lib/i18n/message.ts';

export const newProject = msg({ en: 'New project', ru: 'Новый проект' });
export const createProject = msg({ en: 'Create project', ru: 'Создать проект' });
export const projectCreated = msg<{ name: string }>({
  en: ({ name }) => `Project “${name}” created`,
  ru: ({ name }) => `Проект «${name}» создан`,
});

// ---- Templates (PRJ-021) ----

export const template = msg({ en: 'Template', ru: 'Шаблон' });
export const templateHint = msg({
  en: 'What the project starts with. Everything can be changed later in its settings.',
  ru: 'С чем проект начнёт работу. Всё можно изменить позже в его настройках.',
});
export const brings = msg({ en: 'Brings', ru: 'Что даёт' });
export const templates = {
  blank: {
    summary: msg({ en: 'Start from scratch', ru: 'Начать с чистого листа' }),
    items: [
      msg({
        en: 'The fields marked for new projects',
        ru: 'Поля, отмеченные для новых проектов',
      }),
      msg({
        en: 'Add the rest when you need it',
        ru: 'Остальное добавите, когда понадобится',
      }),
    ],
  },
  scrum: {
    summary: msg({ en: 'Plan the work in sprints', ru: 'Работа спринтами' }),
    items: [
      msg({
        en: 'Types: epic, user story, task, bug',
        ru: 'Типы: эпик, пользовательская история, задача, ошибка',
      }),
      msg({
        en: 'States: open, in progress, to verify, done, duplicate',
        ru: 'Состояния: открыта, в работе, на проверке, готово, дубликат',
      }),
      msg({
        en: 'Sprints, story points and ideal days',
        ru: 'Спринты, стори-поинты и идеальные дни',
      }),
      msg({ en: 'Priority, assignee and estimation', ru: 'Приоритет, исполнитель и оценка' }),
    ],
  },
  kanban: {
    summary: msg({ en: 'Keep the work flowing', ru: 'Непрерывный поток задач' }),
    items: [
      msg({
        en: 'Stages: backlog, develop, review, test, staging, done',
        ru: 'Этапы: бэклог, разработка, ревью, тестирование, предрелиз, готово',
      }),
      msg({
        en: 'Readiness: ready to pull or blocked',
        ru: 'Готовность: можно брать или заблокирована',
      }),
      msg({ en: 'Priority and assignee', ru: 'Приоритет и исполнитель' }),
    ],
  },
} as const;

// ---- Name, key, numbering, description (PRJ-002…PRJ-004, PRJ-006, PRJ-018) ----

export const keyHint = msg<{ example: string }>({
  en: ({ example }) =>
    `Starts every issue ID, such as ${example}. Latin letters, digits and _, starting with a letter; 2 to 10 characters.`,
  ru: ({ example }) =>
    `С него начинаются ID задач, например ${example}. Латинские буквы, цифры и _, первая — буква; от 2 до 10 символов.`,
});
export const keySuggested = msg({
  en: 'Suggested from the name; type your own to change it.',
  ru: 'Предложен по названию; чтобы изменить, введите свой.',
});
export const firstNumber = msg({ en: 'First issue number', ru: 'Номер первой задачи' });
export const firstNumberHint = msg<{ example: string }>({
  en: ({ example }) => `The first issue gets the ID ${example}.`,
  ru: ({ example }) => `Первая задача получит ID ${example}.`,
});
