// Texts of people, groups and roles shown across screens (ACC-006, ACC-016, ACC-070).
// Voice: docs/design.md §12.
import { msg } from '#lib/i18n/message.ts';

export const maskedPerson = msg<{ number: number }>({
  en: ({ number }) => `User ${String(number)}`,
  ru: ({ number }) => `Пользователь ${String(number)}`,
});
export const deletedPerson = msg({ en: 'Deleted user', ru: 'Удалённый пользователь' });
export const disabledBadge = msg({ en: 'Disabled', ru: 'Отключена' });
export const invitedBadge = msg({ en: 'Invited', ru: 'Приглашение' });
export const guestBadge = msg({ en: 'Guest', ru: 'Гость' });
export const youBadge = msg({ en: 'You', ru: 'Вы' });

export const searchPeople = msg({ en: 'Find people', ru: 'Найти людей' });
export const searchPeopleHint = msg({
  en: 'Start of a name, username or email.',
  ru: 'Начало имени, имени пользователя или почты.',
});
export const noPeopleFound = msg({ en: 'Nobody found.', ru: 'Никого не найдено.' });
export const typeToSearch = msg({
  en: 'Type to find people.',
  ru: 'Начните вводить, чтобы найти людей.',
});
export const showMore = msg({ en: 'Show more', ru: 'Показать ещё' });
export const chosenCount = msg<{ count: string }>({
  en: ({ count }) => `Chosen: ${count}`,
  ru: ({ count }) => `Выбрано: ${count}`,
});
export const teamName = msg<{ project: string }>({
  en: ({ project }) => `${project} team`,
  ru: ({ project }) => `Команда «${project}»`,
});
