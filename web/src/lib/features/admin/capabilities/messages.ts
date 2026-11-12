// Texts of the capabilities screen (ACC-064, ADM-023; plan step 1.11 §6.1).
// Voice and glossary: docs/design.md §12.
import { msg } from '#lib/i18n/message.ts';

export const capabilitiesTitle = msg({ en: 'Capabilities', ru: 'Возможности' });
export const capabilitiesIntro = msg({
  en: 'Features switched on for chosen groups: a person has one when they belong to any group on its list, built-in groups included. An empty list switches the capability off.',
  ru: 'Функции, включённые для выбранных групп: возможность есть у человека, если он состоит хоть в одной группе её списка, включая встроенные. Пустой список выключает возможность.',
});
export const readOnlyNote = msg({
  en: 'Only system administrators can change capabilities.',
  ru: 'Менять возможности могут только системные администраторы.',
});

// One capability row.
export const shareViewsName = msg({ en: 'Sharing settings', ru: 'Настройка доступа' });
export const shareViewsDescription = msg({
  en: 'Who may change who views, uses and edits tags, saved searches and boards. On for everyone by default.',
  ru: 'Кто может менять, кто смотрит, использует и редактирует теги, сохранённые поиски и доски. По умолчанию — все.',
});
export const groupsLegend = msg({ en: 'Groups', ru: 'Группы' });
export const findGroupsLabel = msg({ en: 'Find groups', ru: 'Найти группы' });
export const findGroupsHint = msg({ en: 'Part of the name.', ru: 'Часть названия.' });
export const noGroupsFound = msg({ en: 'No such group.', ru: 'Такой группы нет.' });
export const offForEverybody = msg({
  en: 'Off: the list is empty, so nobody has it.',
  ru: 'Выключена: список пуст, её нет ни у кого.',
});
export const saveCapability = msg({ en: 'Save', ru: 'Сохранить' });
export const savedToast = msg({ en: 'Capability saved', ru: 'Возможность сохранена' });
export const switchOffTitle = msg({
  en: 'Switch off for everybody?',
  ru: 'Выключить для всех?',
});
export const switchOffText = msg({
  en: 'The list is empty, so nobody will have this capability.',
  ru: 'Список пуст — возможности не будет ни у кого.',
});
export const switchOffConfirm = msg({ en: 'Switch off', ru: 'Выключить' });

// The «Общие для всех» flags (ADM-023): empty while none exist.
export const flagsTitle = msg({ en: 'On for the whole instance', ru: 'Общие для всех' });
export const flagsIntro = msg({
  en: 'Switches of the instance as a whole; they apply without a restart.',
  ru: 'Переключатели инстанса в целом; применяются без перезапуска.',
});
export const flagStored = msg({
  en: 'Differs from the default.',
  ru: 'Отличается от стандартного.',
});
export const flagDefault = msg({
  en: 'Return to the default',
  ru: 'Вернуть стандартное',
});
export const flagMayNotSwitchOff = msg({
  en: 'This switch may not be turned off.',
  ru: 'Этот переключатель нельзя выключить.',
});
export const flagSavedToast = msg({ en: 'Switch saved', ru: 'Переключатель сохранён' });
