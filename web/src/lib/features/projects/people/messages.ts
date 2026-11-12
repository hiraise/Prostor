// Texts of a project's people page (PRJ-046…PRJ-056, ACC-033) and of the project pickers used by
// the access screens. Voice and glossary: docs/design.md §12.
import { msg, plural } from '#lib/i18n/message.ts';

interface Count {
  count: number;
}

// ---- Page ----

export const addPeople = msg({ en: 'Add people', ru: 'Добавить людей' });
export const readOnly = msg({
  en: 'You can see who has access to this project; its administrators change it.',
  ru: 'Вы видите, у кого есть доступ к проекту; менять его могут администраторы проекта.',
});
export const loadingPeople = msg({ en: 'Loading people…', ru: 'Загрузка людей…' });

// ---- Visibility (PRJ-056) ----

export const privateProject = msg({
  en: 'Private project: only its team and people with roles in it see it.',
  ru: 'Закрытый проект: его видят только команда и те, у кого есть роль в проекте.',
});
export const membersSee = msg<{ group: string }>({
  en: ({ group }) => `Every member sees this project: “${group}” has access to it.`,
  ru: ({ group }) => `Проект видят все участники: у группы «${group}» есть к нему доступ.`,
});
export const everyoneSees = msg<{ group: string }>({
  en: ({ group }) => `Everyone sees this project, the guest included: “${group}” has access to it.`,
  ru: ({ group }) => `Проект видят все, включая гостя: у группы «${group}» есть к нему доступ.`,
});

// ---- Owner (PRJ-008, PRJ-053) ----

export const ownerTitle = msg({ en: 'Owner', ru: 'Владелец' });
export const ownerIntro = msg({
  en: 'The person who answers for the project.',
  ru: 'Человек, который отвечает за проект.',
});
export const noOwner = msg({ en: 'The project has no owner.', ru: 'У проекта нет владельца.' });
export const transferOwnership = msg({ en: 'Transfer ownership', ru: 'Передать владение' });
export const transferTitle = msg<{ project: string }>({
  en: ({ project }) => `Transfer ownership of “${project}”`,
  ru: ({ project }) => `Передать владение проектом «${project}»`,
});
export const newOwner = msg({ en: 'New owner', ru: 'Новый владелец' });
export const transferHint = msg({
  en: 'The new owner gets the project administrator role. The current owner keeps their roles.',
  ru: 'Новый владелец получит роль администратора проекта. Нынешний владелец сохранит свои роли.',
});
export const transferText = msg<{ name: string; owner: string }>({
  en: ({ name, owner }) =>
    `${name} becomes the owner and gets the project administrator role. ${owner} keeps their roles.`,
  ru: ({ name, owner }) =>
    `«${name}» станет владельцем и получит роль администратора проекта. «${owner}» сохранит свои роли.`,
});
export const transferToText = msg<{ name: string }>({
  en: ({ name }) => `${name} becomes the owner and gets the project administrator role.`,
  ru: ({ name }) => `«${name}» станет владельцем и получит роль администратора проекта.`,
});
export const chooseOwner = msg({
  en: 'Choose the new owner.',
  ru: 'Выберите нового владельца.',
});
export const ownershipTransferred = msg<{ name: string }>({
  en: ({ name }) => `${name} owns the project now`,
  ru: ({ name }) => `Теперь владелец проекта — ${name}`,
});

// ---- Team (PRJ-047, PRJ-050, PRJ-051, PRJ-054) ----

export const teamTitle = msg({ en: 'Team', ru: 'Команда' });
export const teamSize = msg<Count>({
  en: ({ count }) => plural('en', count, { one: '# person', other: '# people' }),
  ru: ({ count }) =>
    plural('ru', count, {
      one: '# человек',
      few: '# человека',
      many: '# человек',
      other: '# человека',
    }),
});
export const teamIntro = msg({
  en: 'Team members hold the team roles in this project. A group brings all its members: whoever joins the group joins the team.',
  ru: 'Участники команды получают роли команды в этом проекте. Группа приводит всех своих участников: кто вступает в группу, попадает и в команду.',
});
export const assigneesNote = msg({
  en: 'Anyone in the team can be assigned to issues of this project: the list of assignees includes the team.',
  ru: 'Любого участника команды можно назначить исполнителем задач этого проекта: команда входит в список исполнителей.',
});
export const teamRolesTitle = msg({ en: 'Team roles', ru: 'Роли команды' });
export const teamRolesIntro = msg({
  en: 'Every team member holds these roles in this project.',
  ru: 'Эти роли есть у каждого участника команды в этом проекте.',
});
export const noTeamRoles = msg({
  en: 'No team roles: being in the team gives nothing in this project.',
  ru: 'Ролей команды нет: участие в команде ничего не даёт в этом проекте.',
});
export const teamRoleList = msg({ en: 'Roles of the team', ru: 'Роли команды' });
export const addTeamRole = msg({ en: 'Add role', ru: 'Добавить роль' });
export const addTeamRoleTitle = msg({ en: 'Add a team role', ru: 'Добавить роль команды' });
export const everyRoleHeld = msg({
  en: 'The team holds every role that works in a project.',
  ru: 'У команды уже есть все роли, которые действуют в проекте.',
});
export const teamRoleAdded = msg({ en: 'Team role added', ru: 'Роль команды добавлена' });
export const removeTeamRoleLabel = msg<{ role: string }>({
  en: ({ role }) => `Remove the team role “${role}”`,
  ru: ({ role }) => `Убрать роль команды «${role}»`,
});
export const removeTeamRoleTitle = msg<{ role: string }>({
  en: ({ role }) => `Remove the team role “${role}”?`,
  ru: ({ role }) => `Убрать роль команды «${role}»?`,
});
export const removeTeamRoleText = msg({
  en: 'Team members lose its permissions in this project, unless they hold the role another way.',
  ru: 'Участники команды потеряют её права в этом проекте, если не получают эту роль другим путём.',
});
export const removeRole = msg({ en: 'Remove role', ru: 'Убрать роль' });
export const teamRoleRemoved = msg({ en: 'Team role removed', ru: 'Роль команды убрана' });

export const teamGroupsTitle = msg({ en: 'Groups in the team', ru: 'Группы в команде' });
export const teamGroupsIntro = msg({
  en: 'Members of these groups are in the team while they are in the group. To take one of them out, remove them from the group.',
  ru: 'Участники этих групп входят в команду, пока состоят в группе. Чтобы убрать одного из них, уберите его из группы.',
});
export const noTeamGroups = msg({ en: 'No groups in the team.', ru: 'Групп в команде нет.' });
export const teamPeopleTitle = msg({ en: 'Added directly', ru: 'Добавлены лично' });
export const noTeamPeople = msg({
  en: 'Nobody is added directly.',
  ru: 'Лично никто не добавлен.',
});
export const findTeamPeople = msg({ en: 'Find in the team', ru: 'Найти в команде' });
export const findTeamPeopleHint = msg({
  en: 'Start of a name or username.',
  ru: 'Начало имени или имени пользователя.',
});
export const removeFromTeam = msg({ en: 'Remove from team', ru: 'Убрать из команды' });
export const removePersonLabel = msg<{ name: string }>({
  en: ({ name }) => `Remove ${name} from the team`,
  ru: ({ name }) => `Убрать «${name}» из команды`,
});
export const removePersonTitle = msg<{ name: string }>({
  en: ({ name }) => `Remove ${name} from the team?`,
  ru: ({ name }) => `Убрать «${name}» из команды?`,
});
export const removePersonText = msg({
  en: 'They lose the team roles in this project, unless they are in the team through a group. Roles given to them directly stay.',
  ru: 'Человек потеряет роли команды в этом проекте, если не входит в команду через группу. Роли, выданные лично, останутся.',
});
export const removeGroupLabel = msg<{ group: string }>({
  en: ({ group }) => `Remove “${group}” from the team`,
  ru: ({ group }) => `Убрать группу «${group}» из команды`,
});
export const removeGroupTitle = msg<{ group: string }>({
  en: ({ group }) => `Remove “${group}” from the team?`,
  ru: ({ group }) => `Убрать группу «${group}» из команды?`,
});
export const removeGroupText = msg<{ group: string }>({
  en: ({ group }) =>
    `Members of “${group}” leave the team, unless they are in it another way. Members of a group cannot leave the team one by one: remove them from the group instead.`,
  ru: ({ group }) =>
    `Участники группы «${group}» покинут команду, если не входят в неё другим путём. Убрать из команды отдельного участника группы нельзя — уберите его из самой группы.`,
});
export const leftTeam = msg<{ name: string }>({
  en: ({ name }) => `${name} left the team`,
  ru: ({ name }) => `«${name}» больше не в команде`,
});

// ---- Other roles (PRJ-052) ----

export const otherTitle = msg({ en: 'Other roles in the project', ru: 'Другие роли в проекте' });
export const otherIntro = msg({
  en: 'People and groups that hold roles here without being in the team.',
  ru: 'Люди и группы, у которых есть роли в проекте без участия в команде.',
});
export const noOther = msg({
  en: 'Nobody else holds roles in this project.',
  ru: 'Больше ни у кого нет ролей в этом проекте.',
});
export const otherList = msg({ en: 'Roles outside the team', ru: 'Роли вне команды' });
export const assignRole = msg({ en: 'Assign role', ru: 'Выдать роль' });
export const revoke = msg({ en: 'Revoke', ru: 'Отозвать' });
export const revokeLabel = msg<{ role: string; holder: string }>({
  en: ({ role, holder }) => `Revoke “${role}” from ${holder}`,
  ru: ({ role, holder }) => `Отозвать роль «${role}» у «${holder}»`,
});
export const revokeTitle = msg<{ role: string }>({
  en: ({ role }) => `Revoke “${role}”?`,
  ru: ({ role }) => `Отозвать роль «${role}»?`,
});
export const revokeText = msg<{ holder: string }>({
  en: ({ holder }) =>
    `${holder} loses the permissions of this role in this project, unless they hold it another way.`,
  ru: ({ holder }) =>
    `«${holder}» потеряет права этой роли в проекте, если не получает её другим путём.`,
});
export const roleRevoked = msg({ en: 'Role revoked', ru: 'Роль отозвана' });

// ---- Global roles (PRJ-052, ACC-059) ----

export const globalTitle = msg({ en: 'Global roles', ru: 'Глобальные роли' });
export const globalIntro = msg({
  en: 'Assigned globally, these roles apply in every project, this one included. They change on the page of whoever holds them.',
  ru: 'Эти роли выданы глобально и действуют во всех проектах, в том числе в этом. Изменить их можно на странице того, у кого они есть.',
});
export const noGlobal = msg({
  en: 'No global roles reach this project.',
  ru: 'Глобальных ролей, действующих в проекте, нет.',
});
export const globalList = msg({ en: 'Global roles here', ru: 'Глобальные роли в проекте' });

// ---- Adding people (PRJ-048, PRJ-052, PRJ-056) ----

export const addPeopleTitle = msg<{ project: string }>({
  en: ({ project }) => `Add people to “${project}”`,
  ru: ({ project }) => `Добавить людей в проект «${project}»`,
});
export const whoTabs = msg({ en: 'Whom to add', ru: 'Кого добавить' });
export const peopleTab = msg({ en: 'People', ru: 'Люди' });
export const groupsTab = msg({ en: 'Groups', ru: 'Группы' });
export const groupBadge = msg({ en: 'Group', ru: 'Группа' });
export const choosePeople = msg({ en: 'People to add', ru: 'Кого добавить' });
export const chooseGroups = msg({ en: 'Groups to add', ru: 'Какие группы добавить' });
export const noDirectory = msg({
  en: 'Finding people needs the right to see people. Groups can still be added.',
  ru: 'Чтобы искать людей, нужно право на просмотр людей. Группы добавить можно.',
});
export const findGroups = msg({ en: 'Find groups', ru: 'Найти группы' });
export const findGroupsHint = msg({ en: 'Part of a name.', ru: 'Часть названия.' });
export const noGroupsFound = msg({ en: 'No groups found.', ru: 'Группы не найдены.' });
export const howTitle = msg({ en: 'What they get', ru: 'Что они получат' });
export const joinTeam = msg({ en: 'Join the team', ru: 'Войти в команду' });
export const joinTeamHint = msg<{ roles: string }>({
  en: ({ roles }) => `They get the team roles: ${roles}.`,
  ru: ({ roles }) => `Получат роли команды: ${roles}.`,
});
export const joinTeamNoRoles = msg({
  en: 'The team has no roles yet: they get nothing but a place in the team.',
  ru: 'У команды пока нет ролей: кроме места в команде, они ничего не получат.',
});
export const onlyRole = msg({ en: 'Only a role in this project', ru: 'Только роль в проекте' });
export const onlyRoleHint = msg({
  en: 'They get the chosen role here without joining the team.',
  ru: 'Получат выбранную роль в проекте, не входя в команду.',
});
export const chosenSummary = msg<{ people: number; groups: number }>({
  en: ({ people, groups }) =>
    `Chosen: ${plural('en', people, { one: '# person', other: '# people' })}, ${plural('en', groups, { one: '# group', other: '# groups' })}`,
  ru: ({ people, groups }) =>
    `Выбрано: ${plural('ru', people, { one: '# человек', few: '# человека', many: '# человек', other: '# человека' })}, ${plural('ru', groups, { one: '# группа', few: '# группы', many: '# групп', other: '# группы' })}`,
});
export const chooseSomeone = msg({
  en: 'Choose people or groups.',
  ru: 'Выберите людей или группы.',
});
export const addToTeam = msg({ en: 'Add to team', ru: 'Добавить в команду' });
export const broadGroup = msg<{ group: string }>({
  en: ({ group }) => `“${group}” includes every account: everyone gets this access to the project.`,
  ru: ({ group }) =>
    `В группу «${group}» входят все учётные записи: этот доступ к проекту получат все.`,
});
export const addedToTeam = msg<Count>({
  en: ({ count }) =>
    plural('en', count, { one: '# new member of the team', other: '# new members of the team' }),
  ru: ({ count }) =>
    plural('ru', count, {
      one: '# новый участник команды',
      few: '# новых участника команды',
      many: '# новых участников команды',
      other: '# нового участника команды',
    }),
});
export const nothingNew = msg({ en: 'They are in the team already', ru: 'Они уже в команде' });
export const roleAssigned = msg({ en: 'Role assigned', ru: 'Роль выдана' });

// ---- Roles and refusals ----

export const role = msg({ en: 'Role', ru: 'Роль' });
export const chooseRoleOption = msg({ en: 'Choose a role…', ru: 'Выберите роль…' });
export const chooseRole = msg({ en: 'Choose a role.', ru: 'Выберите роль.' });
export const inProject = msg({
  en: 'What it allows in this project',
  ru: 'Что она разрешает в этом проекте',
});
export const lackingToJoin = msg({
  en: 'To add people to the team, you need every permission its roles give. You lack:',
  ru: 'Чтобы добавить людей в команду, нужны все права, которые дают её роли. Вам не хватает:',
});
export const lackingToAssign = msg({
  en: 'You can pass on only permissions you hold yourself. You lack:',
  ru: 'Передать можно только права, которые есть у вас самих. Вам не хватает:',
});
export const archivedConflict = msg({
  en: 'The project is archived: restore it to change its people.',
  ru: 'Проект в архиве: чтобы менять его людей, восстановите проект.',
});

// ---- Projects as others show them ----

export const hiddenProject = msg({ en: 'Hidden project', ru: 'Скрытый проект' });
export const findProjects = msg({ en: 'Find projects', ru: 'Найти проекты' });
export const findProjectsHint = msg({
  en: 'Part of the name or the start of the key.',
  ru: 'Часть названия или начало ключа.',
});
export const noProjectsFound = msg({
  en: 'No projects you manage were found.',
  ru: 'Проектов, которыми вы управляете, не найдено.',
});
export const chosenCount = msg<{ count: string }>({
  en: ({ count }) => `Chosen: ${count}`,
  ru: ({ count }) => `Выбрано: ${count}`,
});
export const showMore = msg({ en: 'Show more', ru: 'Показать ещё' });
