// Texts of a project's general settings (PRJ-037…PRJ-039, PRJ-061, PRJ-063, PRJ-064). Voice and
// glossary: docs/design.md §12.
import { msg } from '#lib/i18n/message.ts';

export const title = msg({ en: 'General settings', ru: 'Общие настройки' });
export const keyHint = msg<{ example: string }>({
  en: ({ example }) =>
    `Starts every issue ID, such as ${example}. A new key changes the IDs of all issues; the old IDs keep leading to them.`,
  ru: ({ example }) =>
    `С него начинаются ID задач, например ${example}. С новым ключом изменятся ID всех задач, а старые ID продолжат вести к ним.`,
});
export const formerKeysNote = msg({
  en: 'They stay reserved for this project: issue IDs and links with them still lead here.',
  ru: 'Они закреплены за этим проектом: ID задач и ссылки с ними по-прежнему ведут сюда.',
});
export const saveChanges = msg({ en: 'Save changes', ru: 'Сохранить изменения' });
export const settingsSaved = msg({ en: 'Settings saved', ru: 'Настройки сохранены' });
export const changedMeanwhile = msg({
  en: 'Someone changed these settings at the same time. The form shows their version now; make your changes again if they are still needed.',
  ru: 'Кто-то изменил эти настройки одновременно с вами. Теперь в форме их версия; если ваши изменения ещё нужны, внесите их снова.',
});

// ---- Default audience (PRJ-039) ----

export const audience = msg({ en: 'Default audience', ru: 'Видимость по умолчанию' });
export const audienceHint = msg({
  en: 'New issues and articles of the project are visible only to this group or team at first; their authors can change that. Existing ones do not change.',
  ru: 'Новые задачи и статьи проекта сначала видны только этой группе или команде; их авторы могут это изменить. Существующие не меняются.',
});
export const everyone = msg({
  en: 'Everyone who can see the project',
  ru: 'Все, кто видит проект',
});
export const chooseAudience = msg({
  en: 'Choose group or team',
  ru: 'Выбрать группу или команду',
});
export const clearAudience = msg({ en: 'Remove restriction', ru: 'Снять ограничение' });
export const audienceTitle = msg({ en: 'Default audience', ru: 'Видимость по умолчанию' });
export const findAudience = msg({ en: 'Find groups and teams', ru: 'Найти группы и команды' });
export const visibleTo = msg({ en: 'Visible to', ru: 'Кому видны' });
export const noAudienceFound = msg({
  en: 'No groups or teams found.',
  ru: 'Группы и команды не найдены.',
});
export const choose = msg({ en: 'Choose', ru: 'Выбрать' });
export const showMore = msg({ en: 'Show more', ru: 'Показать ещё' });

// ---- Archive and delete (PRJ-061, PRJ-063, PRJ-064) ----

export const dangerZone = msg({ en: 'Archive and delete', ru: 'Архив и удаление' });
export const archiveText = msg({
  en: 'An archived project is read-only and leaves pickers and searches. Nothing is deleted; restore it at any time.',
  ru: 'Проект в архиве доступен только для чтения и не появляется в выборе проектов и в поиске. Ничего не удаляется; восстановить его можно в любой момент.',
});
export const restoreText = msg({
  en: 'The project is archived. Restoring it brings everything back: it can change again and shows up in pickers and searches.',
  ru: 'Проект в архиве. После восстановления всё вернётся: его снова можно менять, и он снова появится в выборе проектов и в поиске.',
});
export const deleteText = msg({
  en: 'Deletes the project with all its issues and articles for good. It cannot be undone.',
  ru: 'Удаляет проект со всеми задачами и статьями навсегда. Это нельзя отменить.',
});

// ---- Logo (PRJ-007) ----

export const logo = msg({ en: 'Logo', ru: 'Логотип' });
export const uploadLogo = msg({ en: 'Upload logo', ru: 'Загрузить логотип' });
export const resetLogo = msg({ en: 'Restore the default', ru: 'Вернуть стандартный' });
export const logoHint = msg({
  en: 'PNG, JPEG, GIF or WebP, fitted into a square. Without a picture the project shows the letters of its key.',
  ru: 'PNG, JPEG, GIF или WebP; картинка вписывается в квадрат. Без картинки у проекта буквы его ключа.',
});
export const logoSaved = msg({ en: 'Logo updated.', ru: 'Логотип обновлён.' });
export const logoReset = msg({
  en: 'The default logo is back.',
  ru: 'Возвращён стандартный логотип.',
});
export const notAPicture = msg({
  en: 'This file is not a picture. Choose a PNG, JPEG, GIF or WebP file.',
  ru: 'Этот файл — не картинка. Выберите файл PNG, JPEG, GIF или WebP.',
});
