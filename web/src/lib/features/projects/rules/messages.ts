// Texts of the project's rules tab (AUT-023). Voice and glossary: docs/design.md §12.
import { msg } from '#lib/i18n/message.ts';

export const title = msg({ en: 'Rules', ru: 'Правила' });
export const lead = msg({
  en: 'The project’s built-in rules. A rule runs inside the action that triggers it.',
  ru: 'Встроенные правила проекта. Правило срабатывает внутри действия, которое его запускает.',
});
/** The gray note of a rule the project cannot run yet. */
export const needSetup = msg({ en: 'Needs setup:', ru: 'Нужна настройка:' });
/** A field the project has not attached. */
export const needField = msg<{ name: string }>({
  en: ({ name }) => `missing the field “${name}”`,
  ru: ({ name }) => `не хватает поля «${name}»`,
});
/** A value the field’s set lacks; `value` is null when any value does. */
export const needValue = msg<{ field: string; value: string | null }>({
  en: ({ field, value }) =>
    value === null
      ? `the set of “${field}” lacks values`
      : `the set of “${field}” lacks the value “${value}”`,
  ru: ({ field, value }) =>
    value === null
      ? `набору поля «${field}» не хватает значений`
      : `набору поля «${field}» не хватает значения «${value}»`,
});
export const openFields = msg({ en: 'Open “Fields”', ru: 'Открыть «Поля»' });
/** Someone else changed the settings between the load and the toggle. */
export const changedRetry = msg({
  en: 'The settings changed, try again',
  ru: 'Настройки изменились, повторите',
});
/** The tooltip of a switch the person has no right to move. */
export const noRights = msg({
  en: 'You cannot change this project’s settings',
  ru: 'Нет права менять настройки этого проекта',
});
