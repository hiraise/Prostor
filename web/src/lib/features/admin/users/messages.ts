// Texts of the user directory and of profiles as administrators see them (ACC-002…ACC-006,
// ACC-009, ACC-015, ACC-018, ACC-026, ACC-059, ACC-061, AUTH-010, AUTH-014, UI-010).
// Voice and wording: docs/design.md §12. Russian counts keep the number and the noun together
// with a non-breaking space.
import { msg, plural } from '#lib/i18n/message.ts';

interface Count {
  count: number;
}

/**
 * "Something done" for one account, "# accounts something done" for several. Russian `single`
 * is the text for exactly one; `one` covers 21, 31…
 */
function doneFor(
  en: { one: string; other: string },
  ru: { single: string; one: string; few: string; many: string; other: string },
): (params: Count) => string {
  return msg<Count>({
    en: ({ count }) => plural('en', count, en),
    ru: ({ count }) => (count === 1 ? ru.single : plural('ru', count, ru)),
  });
}

// ---- Directory (ACC-002) ----

export const createAccount = msg({ en: 'Create account', ru: 'Создать учётную запись' });
export const invitePeople = msg({ en: 'Invite people', ru: 'Пригласить людей' });
export const stateFilter = msg({ en: 'Show', ru: 'Показать' });
export const stateAll = msg({ en: 'All accounts', ru: 'Все учётные записи' });
export const stateEnabled = msg({ en: 'Enabled', ru: 'Включённые' });
export const stateDisabled = msg({ en: 'Disabled', ru: 'Отключённые' });
export const nobodyFound = msg({ en: 'Nobody found', ru: 'Никого не нашлось' });
export const nobodyFoundText = msg({
  en: 'Try another name, or show all accounts.',
  ru: 'Попробуйте другое имя или покажите все учётные записи.',
});
export const registeredPrefix = msg({ en: 'Registered', ru: 'Регистрация:' });
export const groupsOf = msg<{ name: string }>({
  en: ({ name }) => `Groups of ${name}`,
  ru: ({ name }) => `Группы: ${name}`,
});
export const selectPerson = msg<{ name: string }>({
  en: ({ name }) => `Select ${name}`,
  ru: ({ name }) => `Выбрать: ${name}`,
});
export const selectAllShown = msg({ en: 'Select all shown', ru: 'Выбрать всех показанных' });
export const selectedCount = msg<Count>({
  en: ({ count }) => plural('en', count, { one: '# selected', other: '# selected' }),
  ru: ({ count }) =>
    plural('ru', count, {
      one: 'Выбрана # запись',
      few: 'Выбрано # записи',
      many: 'Выбрано # записей',
      other: 'Выбрано # записи',
    }),
});
export const selectionActions = msg({
  en: 'Actions with the selected accounts',
  ru: 'Действия с выбранными учётными записями',
});
export const actions = msg({ en: 'Actions', ru: 'Действия' });
export const clearSelection = msg({ en: 'Clear selection', ru: 'Снять выбор' });
export const disable = msg({ en: 'Disable', ru: 'Отключить' });
export const enable = msg({ en: 'Enable', ru: 'Включить' });
export const remove = msg({ en: 'Delete', ru: 'Удалить' });
export const addToGroup = msg({ en: 'Add to group', ru: 'Добавить в группу' });
export const removeFromGroup = msg({ en: 'Remove from group', ru: 'Убрать из группы' });

// ---- Creating an account (ACC-003, AUTH-010) ----

export const newAccountTitle = msg({ en: 'New account', ru: 'Новая учётная запись' });
export const newAccountIntro = msg({
  en: 'The account gets what the built-in groups give. Its profile opens next, where you can add groups and roles.',
  ru: 'Учётная запись получит то, что дают встроенные группы. Затем откроется её профиль — там можно добавить группы и роли.',
});
export const fullName = msg({ en: 'Full name', ru: 'Имя и фамилия' });
export const email = msg({ en: 'Email', ru: 'Почта' });
export const username = msg({ en: 'Username', ru: 'Имя пользователя' });
export const usernameHint = msg({
  en: 'For signing in. Latin letters, digits, dots, hyphens and underscores.',
  ru: 'Для входа. Латинские буквы, цифры, точки, дефисы и подчёркивания.',
});
export const password = msg({ en: 'Password', ru: 'Пароль' });
export const passwordHint = msg({
  en: 'At least 8 characters. Pass it on to the person safely.',
  ru: 'Не меньше 8 символов. Передайте его человеку надёжным способом.',
});
export const repeatPassword = msg({ en: 'Repeat the password', ru: 'Повторите пароль' });
export const passwordTooShort = msg({
  en: 'Use at least 8 characters.',
  ru: 'Нужно не меньше 8 символов.',
});
export const passwordsDiffer = msg({
  en: 'The passwords do not match.',
  ru: 'Пароли не совпадают.',
});
export const mustChangePassword = msg({
  en: 'Ask for a new password at the first sign-in',
  ru: 'Попросить сменить пароль при первом входе',
});
export const mustChangePasswordHint = msg({
  en: 'Then only the person knows the password they use.',
  ru: 'Тогда рабочий пароль будет знать только сам человек.',
});
export const usernameTaken = msg({
  en: 'Someone already has this username.',
  ru: 'Это имя пользователя уже занято.',
});
export const emailTaken = msg({
  en: 'Another account has this email.',
  ru: 'Эта почта уже у другой учётной записи.',
});
export const accountCreated = msg<{ name: string }>({
  en: ({ name }) => `Account created for ${name}`,
  ru: ({ name }) => `Учётная запись создана: ${name}`,
});

// ---- Invitations (ACC-004) ----

export const inviteIntro = msg({
  en: 'Each address gets an account at once and an email with a link to choose a password. You can add groups and roles before people join.',
  ru: 'Для каждого адреса сразу создаётся учётная запись, а на почту приходит ссылка, чтобы задать пароль. Группы и роли можно выдать ещё до того, как человек присоединится.',
});
export const emails = msg({ en: 'Email addresses', ru: 'Адреса почты' });
export const emailsHint = msg({
  en: 'Separate them with spaces, commas or new lines. Up to 100 at once.',
  ru: 'Разделяйте пробелами, запятыми или переносами строк. Не больше 100 за раз.',
});
export const emailsRequired = msg({
  en: 'Enter at least one address.',
  ru: 'Введите хотя бы один адрес.',
});
export const emailsTooMany = msg({
  en: 'Invite at most 100 people at once.',
  ru: 'За раз можно пригласить не больше 100 человек.',
});
export const emailsInvalid = msg<{ list: string }>({
  en: ({ list }) => `These addresses are not valid: ${list}.`,
  ru: ({ list }) => `Неверные адреса: ${list}.`,
});
export const emailsTaken = msg<{ list: string }>({
  en: ({ list }) => `These addresses already have accounts: ${list}.`,
  ru: ({ list }) => `У этих адресов уже есть учётные записи: ${list}.`,
});
export const emailOff = msg({
  en: 'Email is not set up on this server, so invitations cannot be sent. Create accounts with a password instead, or ask a system administrator to set up email.',
  ru: 'На этом сервере не настроена почта, поэтому приглашения не отправить. Создайте учётные записи с паролем или попросите системного администратора настроить почту.',
});
export const sendInvitations = msg({ en: 'Send invitations', ru: 'Отправить приглашения' });
export const invitationsSent = doneFor(
  { one: 'Invitation sent', other: '# invitations sent' },
  {
    single: 'Приглашение отправлено',
    one: '# приглашение отправлено',
    few: '# приглашения отправлены',
    many: '# приглашений отправлено',
    other: '# приглашения отправлено',
  },
);

// ---- Disabling and enabling (ACC-005) ----

export const disableOneTitle = msg<{ name: string }>({
  en: ({ name }) => `Disable the account of ${name}?`,
  ru: ({ name }) => `Отключить учётную запись «${name}»?`,
});
export const disableManyTitle = msg<Count>({
  en: ({ count }) =>
    plural('en', count, { one: 'Disable # account?', other: 'Disable # accounts?' }),
  ru: ({ count }) =>
    plural('ru', count, {
      one: 'Отключить # учётную запись?',
      few: 'Отключить # учётные записи?',
      many: 'Отключить # учётных записей?',
      other: 'Отключить # учётной записи?',
    }),
});
export const disableText = msg({
  en: 'Their sessions end at once, and signing in or using API tokens stops working. Their work stays as it is.',
  ru: 'Сеансы сразу завершатся, войти или воспользоваться API-токенами будет нельзя. Сделанная работа останется на месте.',
});
export const statusLabel = msg({ en: 'Status shown', ru: 'Показываемый статус' });
export const statusLabelHint = msg({
  en: 'Only a word next to the name: every status works the same.',
  ru: 'Это только подпись рядом с именем: любой статус действует одинаково.',
});
export const reason = msg({ en: 'Reason', ru: 'Причина' });
export const reasonHint = msg({
  en: 'Optional. People who manage accounts see it on the profile.',
  ru: 'Необязательно. Её увидят те, кто управляет учётными записями.',
});
export const disableConfirm = msg<Count>({
  en: ({ count }) => (count === 1 ? 'Disable account' : 'Disable accounts'),
  ru: ({ count }) => (count === 1 ? 'Отключить учётную запись' : 'Отключить учётные записи'),
});
export const disabledDone = doneFor(
  { one: 'Account disabled', other: '# accounts disabled' },
  {
    single: 'Учётная запись отключена',
    one: '# учётная запись отключена',
    few: '# учётные записи отключены',
    many: '# учётных записей отключено',
    other: '# учётной записи отключено',
  },
);
export const disableConflict = msg({
  en: 'Your own account and the guest cannot be disabled.',
  ru: 'Нельзя отключить свою учётную запись и гостя.',
});

export const enableOneTitle = msg<{ name: string }>({
  en: ({ name }) => `Enable the account of ${name}?`,
  ru: ({ name }) => `Включить учётную запись «${name}»?`,
});
export const enableManyTitle = msg<Count>({
  en: ({ count }) => plural('en', count, { one: 'Enable # account?', other: 'Enable # accounts?' }),
  ru: ({ count }) =>
    plural('ru', count, {
      one: 'Включить # учётную запись?',
      few: 'Включить # учётные записи?',
      many: 'Включить # учётных записей?',
      other: 'Включить # учётной записи?',
    }),
});
export const enableText = msg({
  en: 'Signing in works again, with the same password and API tokens.',
  ru: 'Вход снова заработает — с прежним паролем и API-токенами.',
});
export const enableConfirm = msg<Count>({
  en: ({ count }) => (count === 1 ? 'Enable account' : 'Enable accounts'),
  ru: ({ count }) => (count === 1 ? 'Включить учётную запись' : 'Включить учётные записи'),
});
export const enabledDone = doneFor(
  { one: 'Account enabled', other: '# accounts enabled' },
  {
    single: 'Учётная запись включена',
    one: '# учётная запись включена',
    few: '# учётные записи включены',
    many: '# учётных записей включено',
    other: '# учётной записи включено',
  },
);
export const enableConflict = msg({
  en: 'The guest account stays disabled until guest access arrives.',
  ru: 'Гостевая учётная запись остаётся отключённой, пока не появится гостевой доступ.',
});

// ---- Deleting (ACC-006) ----

export const deleteOneTitle = msg<{ name: string }>({
  en: ({ name }) => `Delete the account of ${name}?`,
  ru: ({ name }) => `Удалить учётную запись «${name}»?`,
});
export const deleteManyTitle = msg<Count>({
  en: ({ count }) => plural('en', count, { one: 'Delete # account?', other: 'Delete # accounts?' }),
  ru: ({ count }) =>
    plural('ru', count, {
      one: 'Удалить # учётную запись?',
      few: 'Удалить # учётные записи?',
      many: 'Удалить # учётных записей?',
      other: 'Удалить # учётной записи?',
    }),
});
export const cannotUndo = msg({
  en: 'This cannot be undone.',
  ru: 'Отменить это будет нельзя.',
});
export const countingImpact = msg({
  en: 'Counting what goes with it…',
  ru: 'Считаем, что будет удалено…',
});
export const impactIntro = msg({ en: 'Deleted as well:', ru: 'Также будут удалены:' });
export const impactSessions = msg<Count>({
  en: ({ count }) => plural('en', count, { one: '# session', other: '# sessions' }),
  ru: ({ count }) =>
    plural('ru', count, {
      one: '# сеанс',
      few: '# сеанса',
      many: '# сеансов',
      other: '# сеанса',
    }),
});
export const impactTokens = msg<Count>({
  en: ({ count }) => plural('en', count, { one: '# API token', other: '# API tokens' }),
  ru: ({ count }) =>
    plural('ru', count, {
      one: '# API-токен',
      few: '# API-токена',
      many: '# API-токенов',
      other: '# API-токена',
    }),
});
export const impactMemberships = msg<Count>({
  en: ({ count }) =>
    plural('en', count, { one: '# group membership', other: '# group memberships' }),
  ru: ({ count }) =>
    plural('ru', count, {
      one: '# членство в группе',
      few: '# членства в группах',
      many: '# членств в группах',
      other: '# членства в группах',
    }),
});
export const impactAssignments = msg<Count>({
  en: ({ count }) =>
    plural('en', count, { one: '# role given directly', other: '# roles given directly' }),
  ru: ({ count }) =>
    plural('ru', count, {
      one: '# лично выданная роль',
      few: '# лично выданные роли',
      many: '# лично выданных ролей',
      other: '# лично выданной роли',
    }),
});
export const impactNothing = msg({
  en: 'Nothing else: no sessions, tokens, groups or roles of their own.',
  ru: 'Больше ничего: ни сеансов, ни токенов, ни групп, ни своих ролей.',
});
export const replacementIntro = msg({
  en: 'Issues, comments and other work stay. Choose who takes the place of the deleted: links to them will lead to this person.',
  ru: 'Задачи, комментарии и другая работа останутся. Выберите, кто займёт место удалённых: ссылки на них будут вести к этому человеку.',
});
export const replacement = msg({ en: 'Replacement', ru: 'Замена' });
export const chooseReplacement = msg({
  en: 'Choose who takes their place.',
  ru: 'Выберите, кто займёт их место.',
});
export const deleteConfirm = msg<Count>({
  en: ({ count }) => (count === 1 ? 'Delete account' : 'Delete accounts'),
  ru: ({ count }) => (count === 1 ? 'Удалить учётную запись' : 'Удалить учётные записи'),
});
export const deletedDone = doneFor(
  { one: 'Account deleted', other: '# accounts deleted' },
  {
    single: 'Учётная запись удалена',
    one: '# учётная запись удалена',
    few: '# учётные записи удалены',
    many: '# учётных записей удалено',
    other: '# учётной записи удалено',
  },
);
export const deleteConflict = msg({
  en: 'Your own account and the guest cannot be deleted here.',
  ru: 'Здесь нельзя удалить свою учётную запись и гостя.',
});
export const deletedTitle = msg({ en: 'Account deleted', ru: 'Учётная запись удалена' });
export const deletedText = msg<{ name: string }>({
  en: ({ name }) => `Links to it now lead to ${name}.`,
  ru: ({ name }) => `Ссылки на неё теперь ведут к профилю: ${name}.`,
});
export const backToUsers = msg({ en: 'Back to users', ru: 'К списку пользователей' });
export const allUsers = msg({ en: 'All users', ru: 'Все пользователи' });

// ---- Groups (ACC-026) ----

export const addOneToGroupTitle = msg<{ name: string }>({
  en: ({ name }) => `Add ${name} to a group`,
  ru: ({ name }) => `Добавить «${name}» в группу`,
});
export const addManyToGroupTitle = msg<Count>({
  en: ({ count }) =>
    plural('en', count, { one: 'Add # person to a group', other: 'Add # people to a group' }),
  ru: ({ count }) =>
    plural('ru', count, {
      one: 'Добавить # человека в группу',
      few: 'Добавить # человека в группу',
      many: 'Добавить # человек в группу',
      other: 'Добавить # человека в группу',
    }),
});
export const removeOneFromGroupTitle = msg<{ name: string }>({
  en: ({ name }) => `Remove ${name} from a group`,
  ru: ({ name }) => `Убрать «${name}» из группы`,
});
export const removeManyFromGroupTitle = msg<Count>({
  en: ({ count }) =>
    plural('en', count, {
      one: 'Remove # person from a group',
      other: 'Remove # people from a group',
    }),
  ru: ({ count }) =>
    plural('ru', count, {
      one: 'Убрать # человека из группы',
      few: 'Убрать # человека из группы',
      many: 'Убрать # человек из группы',
      other: 'Убрать # человека из группы',
    }),
});
export const findGroup = msg({ en: 'Find a group', ru: 'Найти группу' });
export const findGroupHint = msg({ en: 'Start of its name.', ru: 'Начало названия.' });
export const group = msg({ en: 'Group', ru: 'Группа' });
export const noGroupsFound = msg({ en: 'No groups found.', ru: 'Групп не нашлось.' });
export const noGroupsToLeave = msg({
  en: 'There is no group to remove from: everyone is in the built-in groups by themselves.',
  ru: 'Убирать не из чего: во встроенные группы все входят сами.',
});
export const chooseGroup = msg({ en: 'Choose a group.', ru: 'Выберите группу.' });
export const memberCount = msg<Count>({
  en: ({ count }) => plural('en', count, { one: '# member', other: '# members' }),
  ru: ({ count }) =>
    plural('ru', count, {
      one: '# участник',
      few: '# участника',
      many: '# участников',
      other: '# участника',
    }),
});
export const addedToGroup = msg<{ group: string; count: number }>({
  en: ({ group, count }) =>
    count === 0 ? `Everyone was in “${group}” already` : `Added to “${group}”: ${String(count)}`,
  ru: ({ group, count }) =>
    count === 0
      ? `Все уже были в группе «${group}»`
      : `Добавлено в группу «${group}»: ${String(count)}`,
});
export const removedFromGroup = msg<{ group: string; count: number }>({
  en: ({ group, count }) =>
    count === 0 ? `Nobody of them was in “${group}”` : `Removed from “${group}”: ${String(count)}`,
  ru: ({ group, count }) =>
    count === 0
      ? `Никого из них не было в группе «${group}»`
      : `Убрано из группы «${group}»: ${String(count)}`,
});
export const builtinGroupsConflict = msg({
  en: 'Built-in groups include accounts by themselves.',
  ru: 'Во встроенные группы учётные записи входят сами.',
});
export const lastAdministrator = msg({
  en: 'This would leave nobody able to administer TaskMaster. Make someone else a system administrator first.',
  ru: 'Тогда в TaskMaster не останется ни одного системного администратора. Сначала назначьте администратором кого-то ещё.',
});

// ---- Profile (ACC-009, ACC-015, UI-010) ----

export const accountActions = msg({
  en: 'Account actions',
  ru: 'Действия с учётной записью',
});
export const disableAccount = msg({ en: 'Disable account', ru: 'Отключить учётную запись' });
export const enableAccount = msg({ en: 'Enable account', ru: 'Включить учётную запись' });
export const deleteAccount = msg({ en: 'Delete account', ru: 'Удалить учётную запись' });
export const askNewPassword = msg({
  en: 'Ask for a new password',
  ru: 'Попросить сменить пароль',
});
export const newPasswordAsked = msg({
  en: 'New password asked for',
  ru: 'Смена пароля уже запрошена',
});
export const resendInvitation = msg({
  en: 'Send invitation again',
  ru: 'Отправить приглашение ещё раз',
});
export const sendVerification = msg({
  en: 'Send confirmation link',
  ru: 'Отправить ссылку для подтверждения',
});
export const disabledSince = msg({ en: 'Disabled since', ru: 'Отключена с' });
export const disabledReason = msg<{ reason: string }>({
  en: ({ reason }) => `Reason: ${reason}`,
  ru: ({ reason }) => `Причина: ${reason}`,
});
export const invitedNotice = msg({
  en: 'Invited and has not joined yet.',
  ru: 'Приглашение отправлено, но человек ещё не присоединился.',
});
export const guestNotice = msg({
  en: 'This account stands for visitors who are not signed in.',
  ru: 'Эта учётная запись представляет посетителей, которые не вошли в TaskMaster.',
});
export const basicTierNote = msg({
  en: 'You can see only the name of this person.',
  ru: 'Вам видно только имя этого человека.',
});
export const askPasswordTitle = msg<{ name: string }>({
  en: ({ name }) => `Ask ${name} for a new password?`,
  ru: ({ name }) => `Запросить смену пароля для «${name}»?`,
});
export const askPasswordText = msg({
  en: 'At the next sign-in, choosing a new password comes before anything else. Signed-in sessions are asked at once.',
  ru: 'При следующем входе сначала нужно будет задать новый пароль. В открытых сеансах это потребуется сразу.',
});
export const passwordAsked = msg({ en: 'New password asked for', ru: 'Смена пароля запрошена' });
export const askPasswordConflict = msg({
  en: 'This account has no password to change.',
  ru: 'У этой учётной записи нет пароля, который можно сменить.',
});
export const invitationResent = msg<{ email: string }>({
  en: ({ email }) => `Invitation sent again to ${email}`,
  ru: ({ email }) => `Приглашение снова отправлено на ${email}`,
});
export const resendConflict = msg({
  en: 'The person has joined already, or email is not set up.',
  ru: 'Человек уже присоединился, или почта не настроена.',
});
export const verificationSent = msg<{ email: string }>({
  en: ({ email }) => `Confirmation link sent to ${email}`,
  ru: ({ email }) => `Ссылка для подтверждения отправлена на ${email}`,
});
export const verificationConflict = msg({
  en: 'The address is confirmed already, or email is not set up.',
  ru: 'Адрес уже подтверждён, или почта не настроена.',
});

export const profileTitle = msg({ en: 'Profile', ru: 'Профиль' });
export const profileEditHint = msg({
  en: 'The name shows wherever the person’s work does. They sign in with the username or the email.',
  ru: 'Имя видно везде, где видна работа человека. Входят по имени пользователя или почте.',
});
export const emailConfirmed = msg({ en: 'Confirmed', ru: 'Подтверждена' });
export const emailNotConfirmed = msg({ en: 'Not confirmed', ru: 'Не подтверждена' });
export const noEmail = msg({ en: 'No email', ru: 'Почта не указана' });
export const registered = msg({ en: 'Registered', ru: 'Дата регистрации' });
export const emailChangeHint = msg({
  en: 'A new address stays unconfirmed until the person opens the link sent to it.',
  ru: 'Новый адрес останется неподтверждённым, пока человек не откроет ссылку из письма.',
});
export const saveProfile = msg({ en: 'Save profile', ru: 'Сохранить профиль' });
export const profileSaved = msg({ en: 'Profile saved', ru: 'Профиль сохранён' });
export const profileConflict = msg({
  en: 'Someone else changed this profile meanwhile. The form shows their version: check it and save again.',
  ru: 'Профиль успели изменить. В форме теперь новая версия — проверьте её и сохраните снова.',
});

export const groupsTitle = msg({ en: 'Groups', ru: 'Группы' });
export const projectsTitle = msg({ en: 'Projects', ru: 'Проекты' });
export const projectsIntro = msg({
  en: 'Projects whose team includes the person, directly or through a group.',
  ru: 'Проекты, в команду которых человек входит — лично или через группу.',
});
export const noProjects = msg({
  en: 'Not in the team of any project you can see.',
  ru: 'Не входит в команду ни одного из видимых вам проектов.',
});
export const projectsOf = msg<{ name: string }>({
  en: ({ name }) => `Projects of ${name}`,
  ru: ({ name }) => `Проекты: ${name}`,
});
export const moreProjects = msg<Count>({
  en: ({ count }) => plural('en', count, { one: 'and # more', other: 'and # more' }),
  ru: ({ count }) =>
    plural('ru', count, { one: 'и ещё #', few: 'и ещё #', many: 'и ещё #', other: 'и ещё #' }),
});
export const archivedBadge = msg({ en: 'Archived', ru: 'В архиве' });
export const builtinGroup = msg({ en: 'built-in', ru: 'встроенная' });
export const noOwnGroups = msg({ en: 'No other groups yet.', ru: 'Других групп пока нет.' });
export const removeFromGroupLabel = msg<{ group: string }>({
  en: ({ group }) => `Remove from “${group}”`,
  ru: ({ group }) => `Убрать из группы «${group}»`,
});
export const leaveGroupTitle = msg<{ name: string; group: string }>({
  en: ({ name, group }) => `Remove ${name} from “${group}”?`,
  ru: ({ name, group }) => `Убрать «${name}» из группы «${group}»?`,
});
export const leaveGroupText = msg({
  en: 'Roles that come through this group stop applying.',
  ru: 'Роли, которые даёт эта группа, перестанут действовать.',
});

export const rolesTitle = msg({ en: 'Roles', ru: 'Роли' });
export const rolesIntro = msg({
  en: 'What the person may do comes from roles, given directly or through groups.',
  ru: 'Что может человек, задают роли — выданные лично или через группы.',
});
export const globalScope = msg({ en: 'Global', ru: 'Глобально' });
export const sourceDirect = msg({ en: 'Assigned directly', ru: 'Выдана лично' });
export const sourceGroup = msg({ en: 'Through the group', ru: 'Через группу' });
export const sourceTeam = msg({ en: 'Through the team of', ru: 'Через команду проекта' });
export const noRoles = msg({ en: 'No roles yet.', ru: 'Ролей пока нет.' });
export const permissionCount = msg<Count>({
  en: ({ count }) => plural('en', count, { one: '# permission', other: '# permissions' }),
  ru: ({ count }) =>
    plural('ru', count, {
      one: '# право',
      few: '# права',
      many: '# прав',
      other: '# права',
    }),
});
export const assignRole = msg({ en: 'Assign role', ru: 'Выдать роль' });
export const revoke = msg({ en: 'Revoke', ru: 'Отозвать' });
export const revokeLabel = msg<{ role: string }>({
  en: ({ role }) => `Revoke “${role}”`,
  ru: ({ role }) => `Отозвать роль «${role}»`,
});
export const revokeTitle = msg<{ role: string; name: string }>({
  en: ({ role, name }) => `Revoke “${role}” from ${name}?`,
  ru: ({ role, name }) => `Отозвать роль «${role}» у «${name}»?`,
});
export const revokeText = msg({
  en: 'If a group gives the same role, it stays.',
  ru: 'Если ту же роль даёт группа, она останется.',
});
export const roleRevoked = msg({ en: 'Role revoked', ru: 'Роль отозвана' });
export const assignRoleTitle = msg<{ name: string }>({
  en: ({ name }) => `Assign a role to ${name}`,
  ru: ({ name }) => `Выдать роль: ${name}`,
});
export const role = msg({ en: 'Role', ru: 'Роль' });
export const chooseRolePlaceholder = msg({ en: 'Choose a role', ru: 'Выберите роль' });
export const chooseRole = msg({ en: 'Choose a role.', ru: 'Выберите роль.' });
export const scope = msg({ en: 'Where it applies', ru: 'Где действует' });
export const scopeGlobal = msg({ en: 'Globally', ru: 'Глобально' });
export const scopeGlobalHint = msg({
  en: 'In the whole system and in every project.',
  ru: 'Во всей системе и во всех проектах.',
});
export const scopeProjects = msg({ en: 'In chosen projects', ru: 'В выбранных проектах' });
export const scopeProjectsHint = msg({
  en: 'Only in projects you manage; the role gives there only its project permissions.',
  ru: 'Только в проектах, которыми вы управляете; там роль даёт только права, относящиеся к проекту.',
});
export const projectsLegend = msg({ en: 'Projects', ru: 'Проекты' });
export const teamScope = msg({ en: 'Only in its project:', ru: 'Только в своём проекте:' });
export const teamScopeHint = msg({
  en: 'A project team holds roles only in its own project.',
  ru: 'Команда проекта получает роли только в своём проекте.',
});
export const chooseProjects = msg({
  en: 'Choose at least one project.',
  ru: 'Выберите хотя бы один проект.',
});
export const projectRolesOnly = msg({
  en: 'Only roles that give something in a project are offered.',
  ru: 'Предлагаются только роли, которые что-то дают в проекте.',
});
export const roleAssigned = msg({ en: 'Role assigned', ru: 'Роль выдана' });

// ---- Refusals ----

export const missingPermissions = msg<{ names: string }>({
  en: ({ names }) => `You need more rights for this: ${names}.`,
  ru: ({ names }) => `Для этого нужны права: ${names}.`,
});
