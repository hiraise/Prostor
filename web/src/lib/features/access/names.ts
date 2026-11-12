// Names and explanations of permissions, predefined roles, built-in groups and account states
// (ACC-034…ACC-052), in the viewer's language. Predefined roles and built-in groups are shown by
// their key, never by the English name the server stores. Voice: docs/design.md §12.
import type {
  BuiltinGroupKey,
  DisabledLabel,
  PermissionEntity,
  PermissionKey,
  PermissionOperation,
  PredefinedRoleKey,
} from '#lib/api/schema.gen.ts';
import { msg } from '#lib/i18n/message.ts';

import * as m from './messages.ts';

interface Described {
  name: () => string;
  description: () => string;
}

function described(
  name: { en: string; ru: string },
  description: { en: string; ru: string },
): Described {
  return { name: msg(name), description: msg(description) };
}

export const permissions: Record<PermissionKey, Described> = {
  'system.read': described(
    { en: 'Read system settings', ru: 'Просмотр системных настроек' },
    {
      en: 'See administration settings, integrations, metrics, and every group and role.',
      ru: 'Видеть настройки администрирования, интеграции, метрики, все группы и роли.',
    },
  ),
  'system.write': described(
    { en: 'Change system settings', ru: 'Изменение системных настроек' },
    {
      en: 'Change administration settings and integrations, back up the database, create and edit groups and roles, switch capabilities.',
      ru: 'Менять настройки администрирования и интеграции, делать резервные копии, создавать и менять группы и роли, включать возможности.',
    },
  ),
  'user.directory': described(
    { en: 'See people', ru: 'Просмотр людей' },
    {
      en: 'List accounts and see their username, name and avatar.',
      ru: 'Видеть список учётных записей: имя пользователя, имя и аватар.',
    },
  ),
  'user.details': described(
    { en: 'See details about people', ru: 'Просмотр сведений о людях' },
    {
      en: 'Also see the email, groups and roles of others. Never personal settings or security.',
      ru: 'Видеть также почту, группы и роли других. Личные настройки и безопасность — никогда.',
    },
  ),
  'user.self': described(
    { en: 'Edit own profile', ru: 'Изменение своего профиля' },
    {
      en: 'Change one’s own name, username, email and password.',
      ru: 'Менять своё имя, имя пользователя, почту и пароль.',
    },
  ),
  'user.update': described(
    { en: 'Edit accounts', ru: 'Изменение учётных записей' },
    {
      en: 'Edit any profile, disable and enable accounts, ask for a new password.',
      ru: 'Менять любой профиль, отключать и включать учётные записи, требовать новый пароль.',
    },
  ),
  'user.create': described(
    { en: 'Create accounts', ru: 'Создание учётных записей' },
    {
      en: 'Create accounts and invite people by email.',
      ru: 'Создавать учётные записи и приглашать людей по почте.',
    },
  ),
  'user.delete': described(
    { en: 'Delete accounts', ru: 'Удаление учётных записей' },
    {
      en: 'Delete accounts, handing their place to a replacement.',
      ru: 'Удалять учётные записи, передавая их место другой.',
    },
  ),
  'project.create': described(
    { en: 'Create projects', ru: 'Создание проектов' },
    { en: 'Create new projects.', ru: 'Создавать новые проекты.' },
  ),
  'project.basic': described(
    { en: 'See projects', ru: 'Просмотр проектов' },
    {
      en: 'See the name, description and team of a project.',
      ru: 'Видеть название, описание и команду проекта.',
    },
  ),
  'project.full': described(
    { en: 'See project settings', ru: 'Просмотр настроек проекта' },
    {
      en: 'See every property of a project and the roles people hold in it.',
      ru: 'Видеть все свойства проекта и роли людей в нём.',
    },
  ),
  'project.update': described(
    { en: 'Manage projects', ru: 'Управление проектами' },
    {
      en: 'Change project settings, fields and access, and assign roles in the project.',
      ru: 'Менять настройки, поля и доступ проекта, выдавать роли в нём.',
    },
  ),
  'project.delete': described(
    { en: 'Delete projects', ru: 'Удаление проектов' },
    {
      en: 'Delete projects with everything in them.',
      ru: 'Удалять проекты со всем содержимым.',
    },
  ),
  'issue.read': described(
    { en: 'Read issues', ru: 'Просмотр задач' },
    { en: 'See issues and their public fields.', ru: 'Видеть задачи и их общие поля.' },
  ),
  'issue.create': described(
    { en: 'Create issues', ru: 'Создание задач' },
    {
      en: 'Create issues. Reporters also see and edit their own issues.',
      ru: 'Создавать задачи. Автор также видит и меняет свои задачи.',
    },
  ),
  'issue.update': described(
    { en: 'Edit issues', ru: 'Изменение задач' },
    { en: 'Change the public fields of any issue.', ru: 'Менять общие поля любой задачи.' },
  ),
  'issue.delete': described(
    { en: 'Delete issues', ru: 'Удаление задач' },
    { en: 'Delete issues, one’s own included.', ru: 'Удалять задачи, в том числе свои.' },
  ),
  'issue.link': described(
    { en: 'Link issues', ru: 'Связывание задач' },
    {
      en: 'Add and remove links between issues.',
      ru: 'Добавлять и убирать связи между задачами.',
    },
  ),
  'issue.private.read': described(
    { en: 'Read private fields', ru: 'Просмотр скрытых полей' },
    { en: 'See fields hidden from most people.', ru: 'Видеть поля, скрытые от большинства.' },
  ),
  'issue.private.update': described(
    { en: 'Edit private fields', ru: 'Изменение скрытых полей' },
    { en: 'Change fields hidden from most people.', ru: 'Менять поля, скрытые от большинства.' },
  ),
  'issue.watchers.read': described(
    { en: 'See watchers', ru: 'Просмотр наблюдателей' },
    { en: 'See who watches an issue.', ru: 'Видеть, кто следит за задачей.' },
  ),
  'issue.watchers.update': described(
    { en: 'Manage watchers', ru: 'Управление наблюдателями' },
    { en: 'Add other people as watchers.', ru: 'Добавлять других людей в наблюдатели.' },
  ),
  'issue.voters.read': described(
    { en: 'See voters', ru: 'Просмотр голосов' },
    { en: 'See who voted for an issue.', ru: 'Видеть, кто голосовал за задачу.' },
  ),
  'issue.silent': described(
    { en: 'Change silently', ru: 'Изменения без уведомлений' },
    {
      en: 'Apply changes without notifying watchers.',
      ru: 'Вносить изменения, не уведомляя наблюдателей.',
    },
  ),
  'visibility.bypass': described(
    { en: 'See restricted items', ru: 'Просмотр ограниченного' },
    {
      en: 'See issues, comments and attachments limited to other people.',
      ru: 'Видеть задачи, комментарии и вложения, доступные только другим.',
    },
  ),
  'comment.create': described(
    { en: 'Comment', ru: 'Комментирование' },
    {
      en: 'Add comments. Authors see their own comments.',
      ru: 'Добавлять комментарии. Автор видит свои комментарии.',
    },
  ),
  'comment.read': described(
    { en: 'Read comments', ru: 'Просмотр комментариев' },
    { en: 'See comments on issues.', ru: 'Видеть комментарии к задачам.' },
  ),
  'comment.update.own': described(
    { en: 'Edit own comments', ru: 'Изменение своих комментариев' },
    { en: 'Change comments one wrote.', ru: 'Менять свои комментарии.' },
  ),
  'comment.delete.own': described(
    { en: 'Delete own comments', ru: 'Удаление своих комментариев' },
    { en: 'Delete comments one wrote.', ru: 'Удалять свои комментарии.' },
  ),
  'comment.update.any': described(
    { en: 'Edit any comment', ru: 'Изменение любых комментариев' },
    { en: 'Change comments of other people.', ru: 'Менять комментарии других людей.' },
  ),
  'comment.delete.any': described(
    { en: 'Delete any comment', ru: 'Удаление любых комментариев' },
    {
      en: 'Delete comments of other people, and delete comments for good.',
      ru: 'Удалять комментарии других людей и удалять комментарии навсегда.',
    },
  ),
  'attachment.create': described(
    { en: 'Attach files', ru: 'Добавление вложений' },
    {
      en: 'Attach files. Authors change their own attachments.',
      ru: 'Прикреплять файлы. Автор меняет свои вложения.',
    },
  ),
  'attachment.update': described(
    { en: 'Edit attachments', ru: 'Изменение вложений' },
    {
      en: 'Change any attachment and who sees it.',
      ru: 'Менять любые вложения и то, кто их видит.',
    },
  ),
  'attachment.delete': described(
    { en: 'Delete attachments', ru: 'Удаление вложений' },
    {
      en: 'Delete any attachment. Everyone deletes their own.',
      ru: 'Удалять любые вложения. Свои может удалить каждый.',
    },
  ),
};

export const entities: Record<PermissionEntity, () => string> = {
  system: msg({ en: 'System', ru: 'Система' }),
  user: msg({ en: 'People', ru: 'Люди' }),
  project: msg({ en: 'Projects', ru: 'Проекты' }),
  issue: msg({ en: 'Issues', ru: 'Задачи' }),
  comment: msg({ en: 'Comments', ru: 'Комментарии' }),
  attachment: msg({ en: 'Attachments', ru: 'Вложения' }),
  visibility: msg({ en: 'Visibility', ru: 'Видимость' }),
};

export const operations: Record<PermissionOperation, () => string> = {
  read: msg({ en: 'Read', ru: 'Просмотр' }),
  create: msg({ en: 'Create', ru: 'Создание' }),
  update: msg({ en: 'Change', ru: 'Изменение' }),
  delete: msg({ en: 'Delete', ru: 'Удаление' }),
  link: msg({ en: 'Link', ru: 'Связи' }),
};

export const predefinedRoles: Record<PredefinedRoleKey, Described> = {
  system_admin: described(
    { en: 'System administrator', ru: 'Системный администратор' },
    { en: 'Everything, everywhere.', ru: 'Всё и везде.' },
  ),
  project_admin: described(
    { en: 'Project administrator', ru: 'Администратор проекта' },
    {
      en: 'Runs projects: settings, fields and access, and everything a member does.',
      ru: 'Ведёт проекты: настройки, поля и доступ, и всё, что делает участник.',
    },
  ),
  project_member: described(
    { en: 'Project member', ru: 'Участник проекта' },
    {
      en: 'Works with issues, comments and attachments. The usual role of a project team.',
      ru: 'Работает с задачами, комментариями и вложениями. Обычная роль команды проекта.',
    },
  ),
  directory_viewer: described(
    { en: 'Directory viewer', ru: 'Просмотр справочника' },
    {
      en: 'Sees other people and edits one’s own profile. All members hold it.',
      ru: 'Видит других людей и меняет свой профиль. Есть у всех участников.',
    },
  ),
  project_creator: described(
    { en: 'Project creator', ru: 'Создатель проектов' },
    { en: 'Creates projects, and nothing else.', ru: 'Создаёт проекты, и только.' },
  ),
  account_creator: described(
    { en: 'Account creator', ru: 'Создатель учётных записей' },
    {
      en: 'Creates accounts and invites people.',
      ru: 'Создаёт учётные записи и приглашает людей.',
    },
  ),
};

export const builtinGroups: Record<BuiltinGroupKey, Described> = {
  everyone: described(
    { en: 'Everyone', ru: 'Все' },
    {
      en: 'Every account, the guest included. Every other group is part of it.',
      ru: 'Все учётные записи, включая гостя. Любая другая группа входит в неё.',
    },
  ),
  all_members: described(
    { en: 'All members', ru: 'Все участники' },
    { en: 'Every account except the guest.', ru: 'Все учётные записи, кроме гостя.' },
  ),
};

export const disabledLabels: Record<DisabledLabel, () => string> = {
  disabled: msg({ en: 'Disabled', ru: 'Отключена' }),
  suspended: msg({ en: 'Suspended', ru: 'Приостановлена' }),
  left: msg({ en: 'Left the organization', ru: 'Больше не в организации' }),
  blocked: msg({ en: 'Blocked', ru: 'Заблокирована' }),
};

/** The name of a role as the viewer reads it. */
export function roleName(role: { name: string; builtin: PredefinedRoleKey | null }): string {
  return role.builtin ? predefinedRoles[role.builtin].name() : role.name;
}

/** The name of a group as the viewer reads it; a project team is named after its project. */
export function groupName(group: {
  name: string;
  builtin: BuiltinGroupKey | null;
  team_of?: number | null;
}): string {
  if (group.builtin) return builtinGroups[group.builtin].name();
  if (group.team_of != null) return m.teamName({ project: group.name });
  return group.name;
}
