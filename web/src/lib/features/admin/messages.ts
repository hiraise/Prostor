// Texts shared by the administration pages (ADM-001). Each page keeps its own texts next to it.
// Voice: docs/design.md §12.
import { msg } from '#lib/i18n/message.ts';

export const administration = msg({ en: 'Administration', ru: 'Администрирование' });
export const users = msg({ en: 'Users', ru: 'Пользователи' });
export const groups = msg({ en: 'Groups', ru: 'Группы' });
export const roles = msg({ en: 'Roles', ru: 'Роли' });
export const fields = msg({ en: 'Fields', ru: 'Поля' });
export const valueSets = msg({ en: 'Value sets', ru: 'Наборы значений' });
export const linkTypes = msg({ en: 'Link types', ru: 'Типы связей' });
export const mail = msg({ en: 'Mail', ru: 'Почта' });
export const settings = msg({ en: 'Settings', ru: 'Настройки' });
export const capabilities = msg({ en: 'Capabilities', ru: 'Возможности' });
export const jobs = msg({ en: 'Jobs', ru: 'Задания' });
export const nothingToAdminister = msg({
  en: 'There is nothing here you can manage. Ask a system administrator for access.',
  ru: 'Здесь нет ничего, чем вы можете управлять. Попросите доступ у системного администратора.',
});
