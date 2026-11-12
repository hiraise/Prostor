// Texts of signing in, the first-run setup and the account page (ADM-047, AUTH-001…AUTH-030).
// Voice and wording: docs/design.md §12.
import { msg } from '#lib/i18n/message.ts';

// ---- Sign-in ----

export const signInTitle = msg({ en: 'Sign in to trackflow', ru: 'Вход в trackflow' });
export const login = msg({ en: 'Username or email', ru: 'Имя пользователя или почта' });
export const password = msg({ en: 'Password', ru: 'Пароль' });
export const rememberMe = msg({
  en: 'Remember me on this device',
  ru: 'Запомнить меня на этом устройстве',
});
export const rememberMeHint = msg({
  en: 'Leave it off on a shared computer.',
  ru: 'Не включайте на общем компьютере.',
});
export const signIn = msg({ en: 'Sign in', ru: 'Войти' });
export const wrongCredentials = msg({
  en: 'The username or password is not correct.',
  ru: 'Неверное имя пользователя или пароль.',
});
export const tooManyAttempts = msg({
  en: 'Too many failed attempts. Try again in a few minutes.',
  ru: 'Слишком много неудачных попыток. Попробуйте через несколько минут.',
});
export const signedOut = msg({ en: 'You have signed out.', ru: 'Вы вышли из trackflow.' });
export const sessionEnded = msg({
  en: 'Your session has ended. Sign in again.',
  ru: 'Сеанс завершён. Войдите снова.',
});
export const forgotPassword = msg({
  en: 'Forgot your password? Ask your administrator to set a new one.',
  ru: 'Забыли пароль? Попросите администратора задать новый.',
});
export const forgotPasswordLink = msg({ en: 'Forgot your password?', ru: 'Забыли пароль?' });

// ---- Setup ----

export const setupTitle = msg({ en: 'Set up trackflow', ru: 'Настройка trackflow' });
export const setupIntro = msg({
  en: 'Create the administrator account and choose the basics. You can change the address, language and time zone later.',
  ru: 'Создайте учётную запись администратора и выберите основное. Адрес, язык и часовой пояс можно изменить позже.',
});
export const setupCode = msg({ en: 'Setup code', ru: 'Код установки' });
export const setupCodeHint = msg({
  en: 'Find it in the server log, in the line “trackflow is not set up yet”. It shows that the server is yours.',
  ru: 'Он в журнале сервера, в строке «trackflow is not set up yet». Код подтверждает, что сервер ваш.',
});
export const setupCodeWrong = msg({
  en: 'The setup code is not correct.',
  ru: 'Неверный код установки.',
});
export const administrator = msg({ en: 'Administrator', ru: 'Администратор' });
export const fullName = msg({ en: 'Full name', ru: 'Имя и фамилия' });
export const username = msg({ en: 'Username', ru: 'Имя пользователя' });
export const usernameHint = msg({
  en: 'For signing in. Latin letters, digits, dots, hyphens and underscores.',
  ru: 'Для входа. Латинские буквы, цифры, точки, дефисы и подчёркивания.',
});
export const email = msg({ en: 'Email', ru: 'Почта' });
export const passwordHint = msg({
  en: 'At least 8 characters. A phrase of a few words is easy to remember and hard to guess.',
  ru: 'Не меньше 8 символов. Фразу из нескольких слов легко запомнить и трудно угадать.',
});
export const passwordTooShort = msg({
  en: 'Use at least 8 characters.',
  ru: 'Нужно не меньше 8 символов.',
});
export const repeatPassword = msg({ en: 'Repeat the password', ru: 'Повторите пароль' });
export const passwordsDiffer = msg({
  en: 'The passwords do not match.',
  ru: 'Пароли не совпадают.',
});
export const installation = msg({ en: 'This installation', ru: 'Эта установка' });
export const systemLanguage = msg({ en: 'Language of the tracker', ru: 'Язык трекера' });
export const systemLanguageHint = msg({
  en: 'For search keywords, built-in names and emails. Everyone chooses the language of the interface for themselves.',
  ru: 'Для ключевых слов поиска, встроенных названий и писем. Язык интерфейса каждый выбирает сам.',
});
export const baseUrl = msg({ en: 'Address', ru: 'Адрес' });
export const baseUrlHint = msg({
  en: 'Where people open trackflow; used in links in emails.',
  ru: 'По этому адресу открывают trackflow; он нужен для ссылок в письмах.',
});
export const timeZone = msg({ en: 'Default time zone', ru: 'Часовой пояс по умолчанию' });
export const timeZoneHint = msg({
  en: 'For people who have not chosen their own.',
  ru: 'Для тех, кто не выбрал свой.',
});
export const finishSetup = msg({ en: 'Finish setup', ru: 'Завершить настройку' });
export const setupDone = msg({
  en: 'trackflow is set up. You are signed in as the administrator.',
  ru: 'trackflow настроен. Вы вошли как администратор.',
});

// ---- Account page ----

export const accountTitle = msg({ en: 'Account', ru: 'Учётная запись' });
export const profile = msg({ en: 'Profile', ru: 'Профиль' });
export const noEmail = msg({ en: 'No email', ru: 'Почта не указана' });

export const changePasswordTitle = msg({ en: 'Password', ru: 'Пароль' });
export const currentPassword = msg({ en: 'Current password', ru: 'Текущий пароль' });
export const currentPasswordWrong = msg({
  en: 'The current password is not correct.',
  ru: 'Текущий пароль указан неверно.',
});
export const newPassword = msg({ en: 'New password', ru: 'Новый пароль' });
export const repeatNewPassword = msg({
  en: 'Repeat the new password',
  ru: 'Повторите новый пароль',
});
export const changePasswordNote = msg({
  en: 'You will be signed out on your other devices. API tokens keep working.',
  ru: 'На других устройствах произойдёт выход. API-токены продолжат работать.',
});
export const changePassword = msg({ en: 'Change password', ru: 'Сменить пароль' });
export const passwordChanged = msg({ en: 'Password changed', ru: 'Пароль изменён' });

export const tokensTitle = msg({ en: 'API tokens', ru: 'API-токены' });
export const tokensIntro = msg({
  en: 'Tokens let scripts and other programs work with trackflow on your behalf. A token has your rights, limited to the areas you choose.',
  ru: 'Токены позволяют скриптам и другим программам работать с trackflow от вашего имени. У токена ваши права, ограниченные выбранными областями.',
});
export const createToken = msg({ en: 'Create token', ru: 'Создать токен' });
export const noTokens = msg({ en: 'No tokens yet', ru: 'Токенов пока нет' });
export const noTokensText = msg({
  en: 'Create one for a script or an integration.',
  ru: 'Создайте токен для скрипта или интеграции.',
});
export const tokenCreated = msg({ en: 'Created', ru: 'Создан' });
export const tokenUsed = msg({ en: 'last used', ru: 'использован' });
export const tokenNeverUsed = msg({ en: 'never used', ru: 'ещё не использовался' });
export const deleteTokenLabel = msg<{ name: string }>({
  en: ({ name }) => `Delete token “${name}”`,
  ru: ({ name }) => `Удалить токен «${name}»`,
});
export const deleteTokenTitle = msg<{ name: string }>({
  en: ({ name }) => `Delete token “${name}”?`,
  ru: ({ name }) => `Удалить токен «${name}»?`,
});
export const deleteTokenText = msg({
  en: 'Programs that use it lose access at once. This cannot be undone.',
  ru: 'Программы, которые им пользуются, сразу потеряют доступ. Отменить это нельзя.',
});
export const deleteToken = msg({ en: 'Delete token', ru: 'Удалить токен' });
export const tokenDeleted = msg({ en: 'Token deleted', ru: 'Токен удалён' });
export const loadMore = msg({ en: 'Show more', ru: 'Показать ещё' });

export const newTokenTitle = msg({ en: 'New API token', ru: 'Новый API-токен' });
export const tokenName = msg({ en: 'Name', ru: 'Название' });
export const tokenNameHint = msg({
  en: 'What it is for, such as the name of a script.',
  ru: 'Для чего он, например название скрипта.',
});
export const trackerArea = msg({ en: 'Tracker', ru: 'Трекер' });
export const trackerAreaHint = msg({
  en: 'Projects, issues, boards, reports',
  ru: 'Проекты, задачи, доски, отчёты',
});
export const adminArea = msg({ en: 'Administration', ru: 'Администрирование' });
export const adminAreaHint = msg({
  en: 'Users, groups, roles and settings, as far as your rights go',
  ru: 'Пользователи, группы, роли и настройки — в пределах ваших прав',
});
export const accessNone = msg({ en: 'No access', ru: 'Нет доступа' });
export const accessRead = msg({ en: 'Read', ru: 'Чтение' });
export const accessWrite = msg({ en: 'Read and change', ru: 'Чтение и изменение' });
export const chooseAccess = msg({
  en: 'Choose access to at least one area.',
  ru: 'Выберите доступ хотя бы к одной области.',
});
export const tokenCreatedTitle = msg({ en: 'Token created', ru: 'Токен создан' });
export const copyTokenNow = msg({
  en: 'Copy the token now: you will not be able to see it again.',
  ru: 'Скопируйте токен сейчас: потом посмотреть его будет нельзя.',
});
export const tokenValue = msg({ en: 'Token', ru: 'Токен' });
export const tokenUsage = msg({
  en: 'Programs send it in the header “Authorization: Bearer <token>”.',
  ru: 'Программы передают его в заголовке «Authorization: Bearer <токен>».',
});
export const copyToken = msg({ en: 'Copy token', ru: 'Скопировать токен' });
export const tokenCopied = msg({ en: 'Token copied', ru: 'Токен скопирован' });
export const done = msg({ en: 'Done', ru: 'Готово' });

/** How a token's access to one area reads in its list row. */
export const areaAccess = msg<{ area: string; access: string }>({
  en: ({ area, access }) => `${area}: ${access.toLowerCase()}`,
  ru: ({ area, access }) => `${area}: ${access.toLowerCase()}`,
});

// ---- Profile (ACC-017, AUTH-014, NTF-007) ----

export const profileHint = msg({
  en: 'Your name appears wherever your work does. You sign in with the username or the email.',
  ru: 'Ваше имя видно везде, где видна ваша работа. Входят по имени пользователя или почте.',
});
export const saveProfile = msg({ en: 'Save profile', ru: 'Сохранить профиль' });
export const profileSaved = msg({ en: 'Profile saved', ru: 'Профиль сохранён' });
export const usernameTaken = msg({
  en: 'Someone already has this username.',
  ru: 'Это имя пользователя уже занято.',
});
export const emailTaken = msg({
  en: 'Another account has this email.',
  ru: 'Эта почта уже у другой учётной записи.',
});
export const emailChangeHint = msg({
  en: 'To change the email, confirm it with your current password. We will send a link to the new address.',
  ru: 'Чтобы сменить почту, подтвердите её текущим паролем. На новый адрес придёт ссылка.',
});
export const emailVerified = msg({ en: 'Confirmed', ru: 'Подтверждена' });
export const emailNotVerified = msg({ en: 'Not confirmed', ru: 'Не подтверждена' });
export const emailNotVerifiedText = msg({
  en: 'Confirm the address to be able to reset a forgotten password.',
  ru: 'Подтвердите адрес, чтобы можно было сбросить забытый пароль.',
});
export const sendVerification = msg({ en: 'Send confirmation link', ru: 'Отправить ссылку' });
export const verificationSent = msg<{ email: string }>({
  en: ({ email }) => `We sent a link to ${email}.`,
  ru: ({ email }) => `Ссылка отправлена на ${email}.`,
});
export const sendTestEmail = msg({ en: 'Send test email', ru: 'Отправить проверочное письмо' });
export const testEmailSent = msg<{ email: string }>({
  en: ({ email }) => `A test email is on its way to ${email}.`,
  ru: ({ email }) => `Проверочное письмо отправлено на ${email}.`,
});
export const emailOff = msg({
  en: 'Email is not set up on this server, so no links or test messages can be sent.',
  ru: 'На этом сервере не настроена почта, поэтому ссылки и проверочные письма не отправить.',
});
export const profileChangedElsewhere = msg({
  en: 'The profile changed meanwhile. We loaded the new version; check it and save again.',
  ru: 'Профиль успел измениться. Загружена новая версия — проверьте её и сохраните снова.',
});
export const registered = msg({ en: 'Registered', ru: 'Дата регистрации' });

// ---- My groups and roles (ACC-018, ACC-061) ----

export const accessTitle = msg({ en: 'Groups and roles', ru: 'Группы и роли' });
export const accessIntro = msg({
  en: 'What you may do in trackflow comes from roles, given to you directly or to your groups.',
  ru: 'Что вы можете делать в trackflow, задают роли — выданные вам лично или вашим группам.',
});
export const myGroups = msg({ en: 'Groups', ru: 'Группы' });
export const myRoles = msg({ en: 'Roles', ru: 'Роли' });
export const myProjects = msg({ en: 'Project teams', ru: 'Команды проектов' });
export const noRoles = msg({ en: 'You have no roles yet.', ru: 'Ролей пока нет.' });
export const globalScope = msg({ en: 'Global', ru: 'Глобально' });
export const sourceDirect = msg({ en: 'assigned directly', ru: 'выдана лично' });
export const sourceTeam = msg({ en: 'through the team of', ru: 'через команду проекта' });
export const sourceGroup = msg<{ group: string }>({
  en: ({ group }) => `through the group “${group}”`,
  ru: ({ group }) => `через группу «${group}»`,
});
export const showPermissions = msg<{ role: string }>({
  en: ({ role }) => `Permissions of “${role}”`,
  ru: ({ role }) => `Права роли «${role}»`,
});

// ---- Forgotten password (AUTH-008) ----

export const forgotTitle = msg({ en: 'Reset your password', ru: 'Сброс пароля' });
export const forgotIntro = msg({
  en: 'Enter the email of your account. If it is confirmed, we will send a link to choose a new password.',
  ru: 'Введите почту вашей учётной записи. Если она подтверждена, мы пришлём ссылку для нового пароля.',
});
export const sendResetLink = msg({ en: 'Send link', ru: 'Отправить ссылку' });
export const resetLinkSent = msg({
  en: 'If an account has this confirmed address, a link is on its way. It works for one hour.',
  ru: 'Если у учётной записи есть такой подтверждённый адрес, ссылка уже в пути. Она действует один час.',
});
export const resetUnavailable = msg({
  en: 'Email is not set up on this server. Ask your administrator to set a new password for you.',
  ru: 'На этом сервере не настроена почта. Попросите администратора задать вам новый пароль.',
});
export const backToSignIn = msg({ en: 'Back to sign-in', ru: 'Вернуться ко входу' });
export const resetTitle = msg({ en: 'Choose a new password', ru: 'Новый пароль' });
export const resetFor = msg<{ username: string }>({
  en: ({ username }) => `For the account ${username}. You will be signed out everywhere else.`,
  ru: ({ username }) =>
    `Для учётной записи ${username}. На всех других устройствах произойдёт выход.`,
});
export const setPassword = msg({ en: 'Set password', ru: 'Задать пароль' });
export const passwordReset = msg({
  en: 'Your new password is set. You are signed in.',
  ru: 'Новый пароль задан. Вы вошли в trackflow.',
});

// ---- Links that no longer work ----

export const linkInvalidTitle = msg({ en: 'The link does not work', ru: 'Ссылка не работает' });
export const linkInvalid = msg({
  en: 'It was used already or is not complete. Copy the whole link from the email.',
  ru: 'Ею уже воспользовались, или она скопирована не целиком. Скопируйте ссылку из письма полностью.',
});
export const linkExpiredTitle = msg({ en: 'The link has expired', ru: 'Срок ссылки истёк' });
export const resetExpired = msg({
  en: 'Ask for a new link: it works for one hour.',
  ru: 'Запросите новую ссылку: она действует один час.',
});
export const invitationExpired = msg({
  en: 'Ask your administrator to send the invitation again.',
  ru: 'Попросите администратора отправить приглашение ещё раз.',
});
export const verificationExpired = msg({
  en: 'Send a new link from your account page.',
  ru: 'Отправьте новую ссылку со страницы учётной записи.',
});
export const askNewLink = msg({ en: 'Ask for a new link', ru: 'Запросить новую ссылку' });

// ---- Invitations (ACC-004) ----

export const invitationTitle = msg({ en: 'Join trackflow', ru: 'Присоединиться к trackflow' });
export const invitationIntro = msg<{ email: string }>({
  en: ({ email }) =>
    `An account for ${email} is waiting. Check your name and username and choose a password.`,
  ru: ({ email }) =>
    `Учётная запись для ${email} уже создана. Проверьте имя и имя пользователя и задайте пароль.`,
});
export const join = msg({ en: 'Join', ru: 'Присоединиться' });
export const joined = msg({ en: 'Welcome to trackflow.', ru: 'Добро пожаловать в trackflow.' });

// ---- Email confirmation (AUTH-014) ----

export const verifyTitle = msg({ en: 'Confirm your email', ru: 'Подтверждение почты' });
export const verifying = msg({ en: 'Confirming the address…', ru: 'Подтверждаем адрес…' });
export const verified = msg<{ email: string }>({
  en: ({ email }) => `${email} is confirmed.`,
  ru: ({ email }) => `Адрес ${email} подтверждён.`,
});
export const openTrackflow = msg({ en: 'Open trackflow', ru: 'Открыть trackflow' });

// ---- A required new password (AUTH-010) ----

export const forcedTitle = msg({ en: 'Choose a new password', ru: 'Задайте новый пароль' });
export const forcedIntro = msg({
  en: 'Your administrator asked you to choose a new password before you continue.',
  ru: 'Администратор попросил вас задать новый пароль, прежде чем продолжить.',
});
export const forcedSameAsOld = msg({
  en: 'Choose a password different from the current one.',
  ru: 'Новый пароль должен отличаться от текущего.',
});

// ---- Photo (ACC-017) ----

export const photo = msg({ en: 'Photo', ru: 'Фото' });
export const uploadPhoto = msg({ en: 'Upload photo', ru: 'Загрузить фото' });
export const gravatarPhoto = msg({ en: 'Use Gravatar', ru: 'Взять из Gravatar' });
export const removePhoto = msg({ en: 'Remove photo', ru: 'Убрать фото' });
export const photoHint = msg({
  en: 'PNG, JPEG, GIF or WebP. The picture is cropped to a square.',
  ru: 'PNG, JPEG, GIF или WebP. Картинка обрезается до квадрата.',
});
export const photoSaved = msg({ en: 'Photo updated.', ru: 'Фото обновлено.' });
export const photoRemoved = msg({
  en: 'Photo removed. Your initials are shown instead.',
  ru: 'Фото убрано. Вместо него показываются инициалы.',
});
export const notAPicture = msg({
  en: 'This file is not a picture. Choose a PNG, JPEG, GIF or WebP file.',
  ru: 'Этот файл — не картинка. Выберите файл PNG, JPEG, GIF или WebP.',
});

// ---- Interface (UI-010, UI-004) ----
export const interfaceTitle = msg({ en: 'Interface', ru: 'Интерфейс' });
export const interfaceAbout = msg({
  en: 'These settings follow you to every device and apply at once.',
  ru: 'Настройки действуют сразу и на всех ваших устройствах.',
});
export const homePage = msg({ en: 'Home page', ru: 'Домашняя страница' });
/** Marks the page the whole instance falls back to (set in «Настройки», plan step 1.11). */
export const instanceDefault = msg({ en: 'The instance default', ru: 'По умолчанию у всех' });
export const homeIssues = msg({ en: 'Issues', ru: 'Задачи' });
export const homeProjects = msg({ en: 'Projects', ru: 'Проекты' });
export const homeBoards = msg({ en: 'Boards', ru: 'Доски' });
export const panelSide = msg({
  en: 'Field panel on the issue page',
  ru: 'Панель полей на странице задачи',
});
export const panelEnd = msg({ en: 'On the right', ru: 'Справа' });
export const panelStart = msg({ en: 'On the left', ru: 'Слева' });
export const activityOrder = msg({ en: 'Activity order', ru: 'Порядок активности' });
export const oldestFirst = msg({ en: 'Oldest first', ru: 'Сначала старые' });
export const newestFirst = msg({ en: 'Newest first', ru: 'Сначала новые' });
export const longComments = msg({ en: 'Long comments', ru: 'Длинные комментарии' });
export const commentsCollapsed = msg({ en: 'Fold', ru: 'Сворачивать' });
export const commentsFull = msg({ en: 'Show in full', ru: 'Показывать целиком' });
export const listSidebar = msg({
  en: 'Sidebar of the issue list',
  ru: 'Боковая панель списка задач',
});
export const sidebarOpen = msg({ en: 'Always shown', ru: 'Всегда показывать' });
export const sidebarAuto = msg({
  en: 'Shown on pointing or focus',
  ru: 'Показывать при наведении',
});
export const sidebarClosed = msg({ en: 'Hidden', ru: 'Скрыть' });
