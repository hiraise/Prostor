// Texts of the groups pages (ACC-021…ACC-032, ACC-060). Voice and wording: docs/design.md §12.
import { msg, plural } from '#lib/i18n/message.ts';

// ---- The list (ACC-021, ACC-022) ----

export const groupsIntro = msg({
  en: 'A group gives roles to many people at once: every member holds the roles of the group.',
  ru: 'Группа даёт роли сразу многим: каждый участник получает роли группы.',
});
export const createGroup = msg({ en: 'Create group', ru: 'Создать группу' });
export const findGroups = msg({ en: 'Find groups', ru: 'Найти группы' });
export const findGroupsHint = msg({
  en: 'Part of a name.',
  ru: 'Часть названия.',
});
export const groupList = msg({ en: 'Groups', ru: 'Группы' });
export const noGroupsFound = msg({ en: 'No groups found', ru: 'Группы не найдены' });
export const noGroupsFoundText = msg({
  en: 'Try another part of the name.',
  ru: 'Попробуйте другую часть названия.',
});
export const builtinBadge = msg({ en: 'Built-in', ru: 'Встроенная' });
export const teamBadge = msg({ en: 'Project team', ru: 'Команда проекта' });
export const kindFilter = msg({ en: 'Show', ru: 'Показать' });
export const kindGroups = msg({ en: 'Groups of people', ru: 'Группы людей' });
export const kindTeams = msg({ en: 'Project teams', ru: 'Команды проектов' });
export const kindAll = msg({ en: 'Groups and teams', ru: 'Группы и команды' });
export const autoJoinBadge = msg({ en: 'Auto-join', ru: 'Автодобавление' });
export const memberCount = msg<{ count: number }>({
  en: ({ count }) => plural('en', count, { one: '# member', other: '# members' }),
  ru: ({ count }) =>
    plural('ru', count, {
      one: '# участник',
      few: '# участника',
      many: '# участников',
      other: '# участника',
    }),
});
export const groupRoles = msg<{ name: string }>({
  en: ({ name }) => `Roles of “${name}”`,
  ru: ({ name }) => `Роли группы «${name}»`,
});
export const globalScope = msg({ en: 'Global', ru: 'Глобально' });
export const selectGroup = msg<{ name: string }>({
  en: ({ name }) => `Select “${name}”`,
  ru: ({ name }) => `Выбрать «${name}»`,
});
export const selection = msg({ en: 'Selected groups', ru: 'Выбранные группы' });
export const selectedCount = msg<{ count: string }>({
  en: ({ count }) => `Selected: ${count}`,
  ru: ({ count }) => `Выбрано: ${count}`,
});
export const clearSelection = msg({ en: 'Clear selection', ru: 'Снять выбор' });
export const deleteGroups = msg<{ count: number }>({
  en: ({ count }) => (count === 1 ? 'Delete group' : 'Delete groups'),
  ru: ({ count }) => (count === 1 ? 'Удалить группу' : 'Удалить группы'),
});
export const showMore = msg({ en: 'Show more', ru: 'Показать ещё' });

// ---- Creating (ACC-023) ----

export const newGroupTitle = msg({ en: 'New group', ru: 'Новая группа' });
export const groupName = msg({ en: 'Name', ru: 'Название' });
export const nameTaken = msg({
  en: 'Another group has this name.',
  ru: 'Группа с таким названием уже есть.',
});
export const description = msg({ en: 'Description', ru: 'Описание' });
export const descriptionHint = msg({
  en: 'What the group is for and who belongs in it.',
  ru: 'Для чего группа и кто в неё входит.',
});
export const autoJoin = msg({
  en: 'Add new accounts automatically',
  ru: 'Добавлять новые учётные записи автоматически',
});
export const autoJoinHint = msg({
  en: 'Every new account joins this group. Existing accounts are not added.',
  ru: 'Каждая новая учётная запись попадёт в эту группу. Существующие не добавляются.',
});
export const groupCreated = msg({ en: 'Group created', ru: 'Группа создана' });

// ---- Members (ACC-026) ----

export const membersTitle = msg({ en: 'Members', ru: 'Участники' });
export const addMembers = msg({ en: 'Add members', ru: 'Добавить участников' });
export const findMembers = msg({ en: 'Find members', ru: 'Найти участников' });
export const findMembersHint = msg({
  en: 'Start of a name or username.',
  ru: 'Начало имени или имени пользователя.',
});
export const memberList = msg<{ name: string }>({
  en: ({ name }) => `Members of “${name}”`,
  ru: ({ name }) => `Участники группы «${name}»`,
});
export const noMembers = msg({ en: 'No members yet', ru: 'Участников пока нет' });
export const noMembersText = msg({
  en: 'Add people, and they get the roles of the group.',
  ru: 'Добавьте людей — они получат роли группы.',
});
export const nobodyFound = msg({ en: 'Nobody found.', ru: 'Никого не найдено.' });
export const automaticMembership = msg({
  en: 'Membership is automatic: nobody is added or removed by hand.',
  ru: 'Состав определяется автоматически: добавить или убрать человека вручную нельзя.',
});
export const removeMemberLabel = msg<{ name: string }>({
  en: ({ name }) => `Remove ${name} from the group`,
  ru: ({ name }) => `Убрать из группы: ${name}`,
});
export const removeMemberTitle = msg({
  en: 'Remove from the group?',
  ru: 'Убрать из группы?',
});
export const removeMemberText = msg<{ name: string; group: string }>({
  en: ({ name, group }) =>
    `${name} loses the roles that “${group}” gives, unless they hold them another way.`,
  ru: ({ name, group }) =>
    `${name} потеряет роли, которые даёт группа «${group}», если не получает их другим путём.`,
});
export const removeMember = msg({ en: 'Remove member', ru: 'Убрать участника' });
export const memberRemoved = msg({ en: 'Member removed', ru: 'Участник убран из группы' });

export const addMembersTitle = msg<{ group: string }>({
  en: ({ group }) => `Add members to “${group}”`,
  ru: ({ group }) => `Добавить участников в «${group}»`,
});
export const peopleToAdd = msg({ en: 'People to add', ru: 'Кого добавить' });
export const choosePeople = msg({
  en: 'Choose at least one person.',
  ru: 'Выберите хотя бы одного человека.',
});
export const membersAdded = msg<{ count: number }>({
  en: ({ count }) => plural('en', count, { one: '# member added', other: '# members added' }),
  ru: ({ count }) =>
    plural('ru', count, {
      one: 'Добавлен # участник',
      few: 'Добавлены # участника',
      many: 'Добавлено # участников',
      other: 'Добавлено # участника',
    }),
});
export const alreadyMembers = msg({
  en: 'They are members already',
  ru: 'Они уже в группе',
});
export const lackingToAdd = msg({
  en: 'To add people to this group, you need every permission it gives. You lack:',
  ru: 'Чтобы добавлять людей в эту группу, нужны все права, которые она даёт. Вам не хватает:',
});
export const noAdministratorLeft = msg({
  en: 'This would leave no system administrator. Give the role to someone else first.',
  ru: 'Тогда не останется ни одного системного администратора. Сначала выдайте эту роль кому-то ещё.',
});

// ---- Roles (ACC-060) ----

export const rolesTitle = msg({ en: 'Roles', ru: 'Роли' });
export const rolesIntro = msg({
  en: 'Every member holds these roles where they were assigned: globally or in a project.',
  ru: 'Эти роли есть у каждого участника там, где они выданы: глобально или в проекте.',
});
export const assignRole = msg({ en: 'Assign role', ru: 'Выдать роль' });
export const noRoles = msg({ en: 'No roles yet', ru: 'Ролей пока нет' });
export const noRolesText = msg({
  en: 'Members get only what their other groups and their own roles give.',
  ru: 'Участники получают только то, что дают их другие группы и личные роли.',
});
export const revokeRoleLabel = msg<{ role: string }>({
  en: ({ role }) => `Revoke “${role}”`,
  ru: ({ role }) => `Отозвать роль «${role}»`,
});
export const revokeRoleTitle = msg<{ role: string }>({
  en: ({ role }) => `Revoke “${role}”?`,
  ru: ({ role }) => `Отозвать роль «${role}»?`,
});
export const revokeRoleText = msg<{ group: string }>({
  en: ({ group }) =>
    `Members of “${group}” lose the permissions of this role, unless they hold it another way.`,
  ru: ({ group }) =>
    `Участники группы «${group}» потеряют права этой роли, если не получают её другим путём.`,
});
export const revokeRole = msg({ en: 'Revoke role', ru: 'Отозвать роль' });
export const roleRevoked = msg({ en: 'Role revoked', ru: 'Роль отозвана' });

export const assignRoleTitle = msg<{ group: string }>({
  en: ({ group }) => `Assign a role to “${group}”`,
  ru: ({ group }) => `Выдать роль группе «${group}»`,
});
export const role = msg({ en: 'Role', ru: 'Роль' });
export const chooseRoleOption = msg({ en: 'Choose a role…', ru: 'Выберите роль…' });
export const chooseRole = msg({ en: 'Choose a role.', ru: 'Выберите роль.' });
export const everyRoleHeld = msg({
  en: 'The group holds every role already.',
  ru: 'У группы уже есть все роли.',
});
export const rolePermissions = msg({ en: 'What it allows', ru: 'Что она разрешает' });
export const roleAllowsNothing = msg({
  en: 'Nothing yet: the role has no permissions.',
  ru: 'Пока ничего: в роли нет прав.',
});
export const roleAssigned = msg({ en: 'Role assigned', ru: 'Роль выдана' });
export const lackingToAssign = msg({
  en: 'You can pass on only permissions you hold yourself. You lack:',
  ru: 'Передать можно только права, которые есть у вас самих. Вам не хватает:',
});

// ---- Settings (ACC-024) ----

export const settingsTitle = msg({ en: 'Settings', ru: 'Настройки' });
export const builtinName = msg({
  en: 'A built-in group keeps its name; everyone reads it in their own language.',
  ru: 'Название встроенной группы не меняется: каждый читает его на своём языке.',
});
export const builtinAutoJoinHint = msg({
  en: 'A built-in group always includes every account it covers.',
  ru: 'Во встроенную группу всегда входят все учётные записи, которые она охватывает.',
});
export const saveChanges = msg({ en: 'Save changes', ru: 'Сохранить изменения' });
export const settingsSaved = msg({ en: 'Settings saved', ru: 'Настройки сохранены' });
export const changedMeanwhile = msg({
  en: 'Someone changed these settings at the same time. Their values are shown now; make your changes again if they are still needed.',
  ru: 'Кто-то изменил эти настройки одновременно с вами. Сейчас показаны их значения; если ваши изменения ещё нужны, внесите их снова.',
});
export const deleteGroupTitle = msg({ en: 'Delete this group', ru: 'Удаление группы' });
export const deleteGroupText = msg({
  en: 'Members leave the group and its roles are revoked. A group you choose takes over what was limited to this one.',
  ru: 'Участники покинут группу, её роли будут отозваны. То, что было доступно только этой группе, перейдёт к выбранной вами группе.',
});

// ---- Deleting (ACC-032) ----

export const deleteTitle = msg<{ count: number; name: string }>({
  en: ({ count, name }) =>
    count === 1
      ? `Delete “${name}”?`
      : plural('en', count, { one: 'Delete # group?', other: 'Delete # groups?' }),
  ru: ({ count, name }) =>
    count === 1
      ? `Удалить группу «${name}»?`
      : plural('ru', count, {
          one: 'Удалить # группу?',
          few: 'Удалить # группы?',
          many: 'Удалить # групп?',
          other: 'Удалить # группы?',
        }),
});
export const groupsToDelete = msg({ en: 'Groups to delete', ru: 'Удаляемые группы' });
export const whatEnds = msg({ en: 'What ends', ru: 'Что прекратится' });
/** `count` memberships end across `groups` groups. */
export const membershipsEnd = msg<{ count: number; groups: number }>({
  en: ({ count }) =>
    count === 0
      ? 'Nobody is a member.'
      : plural('en', count, { one: '# membership ends.', other: '# memberships end.' }),
  ru: ({ count, groups }) => {
    if (count === 0) return 'Участников нет.';
    const from = groups === 1 ? 'из группы' : 'из этих групп';
    return plural('ru', count, {
      one: `# участник выйдет ${from}.`,
      few: `# участника выйдут ${from}.`,
      many: `# участников выйдут ${from}.`,
      other: `# участника выйдут ${from}.`,
    });
  },
});
export const assignmentsRevoked = msg<{ count: number }>({
  en: ({ count }) =>
    count === 0
      ? 'No roles to revoke.'
      : plural('en', count, {
          one: '# role assignment is revoked.',
          other: '# role assignments are revoked.',
        }),
  ru: ({ count }) =>
    count === 0
      ? 'Отзывать нечего: ролей нет.'
      : plural('ru', count, {
          one: '# назначение роли будет отозвано.',
          few: '# назначения ролей будут отозваны.',
          many: '# назначений ролей будут отозваны.',
          other: '# назначения ролей будут отозваны.',
        }),
});
export const replacement = msg({ en: 'Replacement group', ru: 'Группа на замену' });
export const replacementHint = msg({
  en: 'It takes over everything limited to the deleted groups, such as issues and comments only they could see, so restricted content stays restricted. Members and roles do not move to it.',
  ru: 'К ней перейдёт всё, что было доступно только удаляемым группам, например задачи и комментарии, которые видели только они, — закрытое останется закрытым. Участники и роли к ней не переходят.',
});
export const findReplacement = msg({ en: 'Find a group', ru: 'Найти группу' });
export const noOtherGroups = msg({
  en: 'No other groups found.',
  ru: 'Других групп не найдено.',
});
export const chooseReplacement = msg({
  en: 'Choose a replacement group.',
  ru: 'Выберите группу на замену.',
});
export const cannotUndo = msg({ en: 'This cannot be undone.', ru: 'Отменить это нельзя.' });
export const groupsDeleted = msg<{ count: number }>({
  en: ({ count }) =>
    count === 1
      ? 'Group deleted'
      : plural('en', count, { one: '# group deleted', other: '# groups deleted' }),
  ru: ({ count }) =>
    count === 1
      ? 'Группа удалена'
      : plural('ru', count, {
          one: '# группа удалена',
          few: '# группы удалены',
          many: '# групп удалено',
          other: '# группы удалены',
        }),
});
