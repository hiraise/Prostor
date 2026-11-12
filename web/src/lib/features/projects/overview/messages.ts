// Texts of a project's overview (PRJ-005, PRJ-008, PRJ-014). Voice and glossary:
// docs/design.md §12.
import { msg } from '#lib/i18n/message.ts';

export const noDescription = msg({
  en: 'No description yet.',
  ru: 'Описания пока нет.',
});
export const descriptionChanged = msg({
  en: 'Someone changed the description at the same time. It shows their version now.',
  ru: 'Кто-то изменил описание одновременно с вами. Теперь здесь их версия.',
});
export const addDescription = msg({ en: 'Add a description', ru: 'Добавить описание' });
export const details = msg({ en: 'Details', ru: 'Сведения' });
export const noOwner = msg({ en: 'Nobody', ru: 'Нет' });
export const formerKeysNote = msg({
  en: 'Issue IDs and links with these keys still lead here.',
  ru: 'ID задач и ссылки с этими ключами по-прежнему ведут сюда.',
});
export const peopleAndRoles = msg({ en: 'People and roles', ru: 'Люди и роли' });
export const template = msg({ en: 'Template', ru: 'Шаблон' });
export const audience = msg({
  en: 'New issues and articles visible to',
  ru: 'Новые задачи и статьи видны',
});
export const settingsLinks = msg({ en: 'Settings', ru: 'Настройки' });
export const generalLink = msg({ en: 'General', ru: 'Общие' });
export const generalText = msg({
  en: 'Name, key, description, archiving and deletion',
  ru: 'Название, ключ, описание, архивирование и удаление',
});
export const peopleLink = msg({ en: 'People', ru: 'Люди' });
export const peopleText = msg({
  en: 'The owner, the team and roles in the project',
  ru: 'Владелец, команда и роли в проекте',
});
export const fieldsLink = msg({ en: 'Fields', ru: 'Поля' });
export const fieldsText = msg({
  en: 'Fields of the project’s issues and their values',
  ru: 'Поля задач проекта и их значения',
});
export const newIssue = msg({ en: 'New issue', ru: 'Новая задача' });
