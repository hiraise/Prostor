// Texts of the application shell: navigation, preferences, notices (UI-001, UI-035).
import { msg } from '#lib/i18n/message.ts';

export const homeLink = msg({ en: 'Prostor, home page', ru: 'Простор, главная страница' });
export const skipToContent = msg({
  en: 'Skip to main content',
  ru: 'Перейти к основному содержимому',
});
export const mainNavigation = msg({ en: 'Main navigation', ru: 'Основная навигация' });
export const collapseNavigation = msg({ en: 'Collapse navigation', ru: 'Свернуть панель' });
export const expandNavigation = msg({ en: 'Expand navigation', ru: 'Развернуть панель' });
export const railWidth = msg({
  en: 'Width of the navigation panel',
  ru: 'Ширина панели навигации',
});

export const sectionIssues = msg({ en: 'Issues', ru: 'Задачи' });
export const sectionBoards = msg({ en: 'Boards', ru: 'Доски' });
export const sectionProjects = msg({ en: 'Projects', ru: 'Проекты' });
export const sectionAdmin = msg({ en: 'Administration', ru: 'Администрирование' });

export const preferences = msg({ en: 'Interface settings', ru: 'Настройки интерфейса' });
export const theme = msg({ en: 'Theme', ru: 'Тема' });
export const themeSystem = msg({ en: 'Same as system', ru: 'Как в системе' });
export const themeLight = msg({ en: 'Light', ru: 'Светлая' });
export const themeDark = msg({ en: 'Dark', ru: 'Тёмная' });
export const language = msg({ en: 'Language', ru: 'Язык' });

export const sectionInProgress = msg({
  en: 'This section is being built',
  ru: 'Раздел в разработке',
});
export const sectionInProgressText = msg({
  en: 'It will appear in one of the next versions of Prostor.',
  ru: 'Он появится в одной из следующих версий Простора.',
});

export const oldBrowser = msg({
  en: 'Your browser is older than Prostor supports, so some things may not work. Update it to the latest version.',
  ru: 'Ваш браузер старее, чем поддерживает Простор, поэтому что-то может не работать. Обновите его до последней версии.',
});
export const serverUnavailable = msg({
  en: 'The server is not available yet',
  ru: 'Сервер пока недоступен',
});
export const serverUnavailableText = msg({
  en: 'It may be starting or updating. We will keep trying to connect.',
  ru: 'Возможно, он запускается или обновляется. Мы продолжаем подключаться.',
});

export const accountMenu = msg<{ name: string }>({
  en: ({ name }) => `Account: ${name}`,
  ru: ({ name }) => `Учётная запись: ${name}`,
});
export const account = msg({ en: 'Account', ru: 'Учётная запись' });
export const signOut = msg({ en: 'Sign out', ru: 'Выйти' });

// The shortcut reference (UI-020, step 2.2 §5). The labels of the bindings themselves live in
// `lib/shortcuts/messages.ts`.
export const shortcutsTitle = msg({ en: 'Keyboard shortcuts', ru: 'Горячие клавиши' });
export const shortcutsFilterLabel = msg({ en: 'Filter the reference', ru: 'Фильтр справки' });
export const shortcutsFilterPlaceholder = msg({
  en: 'Action name or key',
  ru: 'Название действия или клавиша',
});
export const shortcutsNoMatches = msg({ en: 'Nothing found', ru: 'Ничего не найдено' });
export const shortcutsPlatform = msg({ en: 'Key glyphs for', ru: 'Клавиши для' });
export const shortcutsPlatformMac = msg({ en: 'macOS', ru: 'macOS' });
export const shortcutsPlatformPc = msg({ en: 'Windows and Linux', ru: 'Windows и Linux' });
export const shortcutsPrint = msg({ en: 'Print', ru: 'Печать' });
export const shortcutsCaption = msg({
  en: 'Keys are given as Latin-letter positions: on any keyboard layout, the same physical key acts. Two-key rows are pressed in order, one after the other.',
  ru: 'Клавиши указаны как позиции латинских букв: на любой раскладке клавиатуры срабатывает та же клавиша. Строки из двух клавиш нажимаются по очереди.',
});
