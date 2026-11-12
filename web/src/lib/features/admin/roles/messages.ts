// Texts of the roles pages (ACC-034, ACC-044, ACC-046, ACC-050, ACC-052…ACC-057).
// Voice and wording: docs/design.md §12.
import { formatNumber, msg, plural } from '#lib/i18n/message.ts';

// ---- Roles list ----

export const rolesTitle = msg({ en: 'Roles', ru: 'Роли' });
export const rolesIntro = msg({
  en: 'A role is a set of permissions. People and groups get permissions only through roles, assigned globally or in a project.',
  ru: 'Роль — это набор прав. Люди и группы получают права только через роли, выданные глобально или в проекте.',
});
export const createRole = msg({ en: 'Create role', ru: 'Создать роль' });
export const findRoles = msg({ en: 'Find roles', ru: 'Найти роли' });
export const findRolesHint = msg({ en: 'Part of the name.', ru: 'Часть названия.' });
export const withPermission = msg({ en: 'With permission', ru: 'С правом' });
export const anyPermission = msg({ en: 'Any permission', ru: 'Любое право' });
export const predefined = msg({ en: 'Predefined', ru: 'Встроенная' });
export const permissionCount = msg<{ count: number }>({
  en: ({ count }) => plural('en', count, { one: '# permission', other: '# permissions' }),
  ru: ({ count }) =>
    plural('ru', count, { one: '# право', few: '# права', many: '# прав', other: '# права' }),
});
export const assignmentCount = msg<{ count: number }>({
  en: ({ count }) => plural('en', count, { one: '# assignment', other: '# assignments' }),
  ru: ({ count }) =>
    plural('ru', count, {
      one: '# назначение',
      few: '# назначения',
      many: '# назначений',
      other: '# назначения',
    }),
});
export const noRolesFound = msg({ en: 'No roles found', ru: 'Роли не найдены' });
export const noRolesFoundText = msg({
  en: 'Try another name or permission.',
  ru: 'Попробуйте другое название или право.',
});
export const showMore = msg({ en: 'Show more', ru: 'Показать ещё' });

// ---- Creating a role ----

export const newRoleTitle = msg({ en: 'New role', ru: 'Новая роль' });
export const name = msg({ en: 'Name', ru: 'Название' });
export const nameHint = msg({
  en: 'What the role is for, such as “Support agent”.',
  ru: 'Для чего роль, например «Поддержка».',
});
export const description = msg({ en: 'Description', ru: 'Описание' });
export const descriptionHint = msg({
  en: 'Optional. Who should get this role and why.',
  ru: 'Необязательно. Кому и зачем давать эту роль.',
});
export const nameTaken = msg({
  en: 'Another role has this name. Predefined roles keep their names in every language.',
  ru: 'Это название уже занято. Названия встроенных ролей заняты на всех языках.',
});
export const roleCreated = msg({
  en: 'Role created. Choose its permissions.',
  ru: 'Роль создана. Выберите её права.',
});

// ---- One role ----

export const predefinedNotice = msg({
  en: 'This role comes with trackflow and cannot be changed or deleted. For a different set of permissions, create your own role.',
  ru: 'Эта роль встроена в trackflow: её нельзя изменить или удалить. Для другого набора прав создайте свою роль.',
});
export const readOnlyNotice = msg<{ permission: string }>({
  en: ({ permission }) => `Roles can be changed only with the permission “${permission}”.`,
  ru: ({ permission }) => `Менять роли можно только с правом «${permission}».`,
});
export const details = msg({ en: 'Details', ru: 'Основное' });
export const saveChanges = msg({ en: 'Save changes', ru: 'Сохранить изменения' });
export const roleSaved = msg({ en: 'Role saved', ru: 'Роль сохранена' });
export const editConflict = msg({
  en: 'Someone else changed the name or description meanwhile. Save again to keep your version.',
  ru: 'Кто-то изменил название или описание одновременно с вами. Сохраните ещё раз, чтобы оставить свой вариант.',
});

export const permissionsTitle = msg({ en: 'Permissions', ru: 'Права' });
export const switchedOnCount = msg<{ on: number; total: number }>({
  en: ({ on, total }) => `${formatNumber('en', on)} of ${formatNumber('en', total)} switched on`,
  ru: ({ on, total }) => `Включено ${formatNumber('ru', on)} из ${formatNumber('ru', total)}`,
});
export const findPermission = msg({ en: 'Find a permission', ru: 'Найти право' });
export const grouping = msg({ en: 'Group', ru: 'Группировка' });
export const byEntity = msg({ en: 'By area', ru: 'По области' });
export const byOperation = msg({ en: 'By action', ru: 'По действию' });
export const globalOnly = msg({ en: 'Global only', ru: 'Только глобально' });
export const globalOnlyExplained = msg({
  en: 'Permissions marked “Global only” work only when the role is assigned globally, not in a project.',
  ru: 'Права с пометкой «Только глобально» действуют, только когда роль выдана глобально, а не в проекте.',
});
export const alsoOn = msg<{ names: string }>({
  en: ({ names }) => `Also switches on: ${names}`,
  ru: ({ names }) => `Вместе с ним включаются: ${names}`,
});
export const alsoOff = msg<{ names: string }>({
  en: ({ names }) => `Also switches off: ${names}`,
  ru: ({ names }) => `Вместе с ним выключаются: ${names}`,
});
export const noPermissionsMatch = msg<{ text: string }>({
  en: ({ text }) => `No permissions match “${text}”.`,
  ru: ({ text }) => `Нет прав, подходящих под «${text}».`,
});
export const saving = msg({ en: 'Saving…', ru: 'Сохранение…' });
export const missingPermissions = msg<{ names: string }>({
  en: ({ names }) => `You can give only permissions you hold yourself. Missing: ${names}.`,
  ru: ({ names }) =>
    `Выдавать можно только те права, которые есть у вас самих. Не хватает: ${names}.`,
});
export const switchedOn = msg<{ names: string }>({
  en: ({ names }) => `Switched on: ${names}`,
  ru: ({ names }) => `Включено: ${names}`,
});
export const switchedOff = msg<{ names: string }>({
  en: ({ names }) => `Switched off: ${names}`,
  ru: ({ names }) => `Выключено: ${names}`,
});

export const holdersTitle = msg({ en: 'Assigned to', ru: 'Кому выдана' });
export const noHolders = msg({
  en: 'Nobody has this role yet.',
  ru: 'Эта роль пока никому не выдана.',
});
export const group = msg({ en: 'Group', ru: 'Группа' });
export const globalScope = msg({ en: 'Global', ru: 'Глобально' });
export const projectScope = msg<{ id: number }>({
  en: ({ id }) => `Project ${String(id)}`,
  ru: ({ id }) => `Проект ${String(id)}`,
});
export const assigned = msg({ en: 'assigned', ru: 'выдана' });

// ---- Deleting a role ----

export const deleteRole = msg({ en: 'Delete role', ru: 'Удалить роль' });
export const deleteRoleTitle = msg<{ name: string }>({
  en: ({ name }) => `Delete role “${name}”?`,
  ru: ({ name }) => `Удалить роль «${name}»?`,
});
export const deleteUnused = msg({
  en: 'Nobody has this role.',
  ru: 'Эта роль никому не выдана.',
});
export const deleteRevokes = msg({
  en: 'These people and groups lose the role and the permissions it gives:',
  ru: 'Эти люди и группы потеряют роль и права, которые она даёт:',
});
export const groupHolder = msg<{ name: string }>({
  en: ({ name }) => `${name} (group)`,
  ru: ({ name }) => `${name} (группа)`,
});
export const andMore = msg<{ count: number }>({
  en: ({ count }) => `and ${formatNumber('en', count)} more`,
  ru: ({ count }) => `и ещё ${formatNumber('ru', count)}`,
});
export const cannotUndo = msg({ en: 'This cannot be undone.', ru: 'Отменить это нельзя.' });
export const lastAdministrator = msg<{ role: string }>({
  en: ({ role }) =>
    `This would leave nobody able to administer trackflow. Give someone the role “${role}” first.`,
  ru: ({ role }) =>
    `Тогда trackflow останется без администратора. Сначала выдайте кому-нибудь роль «${role}».`,
});
export const roleDeleted = msg({ en: 'Role deleted', ru: 'Роль удалена' });
