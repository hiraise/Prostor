// What the UI says for each API error code (D-016: the server sends codes, the UI translates them)
// and for invalid form fields. The record type makes a new server code a type error here.
import type { ErrorCode, FieldError } from '#lib/api/schema.gen.ts';

import { msg } from '../message.ts';

/** Codes the client adds to the server's: the request did not reach the server. */
export type ClientErrorCode = 'network';

export const errorText: Record<ErrorCode | ClientErrorCode, () => string> = {
  invalid_request: msg({
    en: 'Some values are not valid.',
    ru: 'Некоторые значения указаны неверно.',
  }),
  invalid_query: msg({
    en: 'The search query has mistakes.',
    ru: 'В поисковом запросе есть ошибки.',
  }),
  unauthenticated: msg({
    en: 'Your session has ended. Sign in again.',
    ru: 'Сеанс завершён. Войдите снова.',
  }),
  invalid_credentials: msg({
    en: 'The username or password is not correct.',
    ru: 'Неверное имя пользователя или пароль.',
  }),
  forbidden: msg({
    en: 'You do not have permission to do this.',
    ru: 'У вас нет прав на это действие.',
  }),
  password_change_required: msg({
    en: 'Choose a new password first.',
    ru: 'Сначала задайте новый пароль.',
  }),
  not_found: msg({
    en: 'It does not exist, or you do not have access to it.',
    ru: 'Не найдено, или у вас нет доступа.',
  }),
  method_not_allowed: msg({
    en: 'This action is not supported.',
    ru: 'Это действие не поддерживается.',
  }),
  conflict: msg({
    en: 'Someone changed this at the same time. Reload and try again.',
    ru: 'Кто-то изменил это одновременно с вами. Обновите страницу и попробуйте снова.',
  }),
  last_administrator: msg({
    en: 'This would leave nobody able to administer TaskMaster. Make someone else a system administrator first.',
    ru: 'Тогда в TaskMaster не останется ни одного системного администратора. Сначала назначьте администратором кого-то ещё.',
  }),
  payload_too_large: msg({
    en: 'The file or text is too large.',
    ru: 'Файл или текст слишком большой.',
  }),
  unsupported_media_type: msg({
    en: 'The request format is not supported.',
    ru: 'Формат запроса не поддерживается.',
  }),
  rate_limited: msg({
    en: 'Too many requests. Wait a moment and try again.',
    ru: 'Слишком много запросов. Подождите немного и повторите.',
  }),
  internal: msg({
    en: 'Something went wrong on the server. Try again; if it happens again, tell your administrator.',
    ru: 'На сервере что-то пошло не так. Повторите; если ошибка повторится, сообщите администратору.',
  }),
  unavailable: msg({
    en: 'The server is temporarily unavailable. Try again in a minute.',
    ru: 'Сервер временно недоступен. Повторите через минуту.',
  }),
  link_type_locked: msg({
    en: 'This link type is built in and cannot be changed or deleted.',
    ru: 'Это встроенный тип связи: его нельзя изменить или удалить.',
  }),
  parent_taken: msg({
    en: 'The issue already has a parent of this kind.',
    ru: 'У задачи уже есть родитель такого вида.',
  }),
  link_cycle: msg({
    en: 'This link would make an issue its own ancestor.',
    ru: 'Такая связь замкнёт задачу саму на себя.',
  }),
  hierarchy_conflict: msg({
    en: 'Some issues have several links of this type to a parent, or the links form a loop.',
    ru: 'У некоторых задач несколько родителей по этому типу, или связи образуют петлю.',
  }),
  gravatar_not_found: msg({
    en: 'Gravatar has no picture for your email address.',
    ru: 'В Gravatar нет картинки для вашего адреса почты.',
  }),
  gravatar_unavailable: msg({
    en: 'Gravatar did not answer. Try again later.',
    ru: 'Gravatar не ответил. Повторите позже.',
  }),
  network: msg({
    en: 'No connection to the server. Check your network and try again.',
    ru: 'Нет связи с сервером. Проверьте подключение и повторите.',
  }),
};

/** Field problems (`FieldError.code`); unknown codes fall back to `invalid`. */
export const fieldErrorText = {
  required: msg({ en: 'Fill in this field.', ru: 'Заполните это поле.' }),
  invalid: msg({ en: 'The value is not valid.', ru: 'Неверное значение.' }),
  too_short: msg({ en: 'The value is too short.', ru: 'Слишком короткое значение.' }),
  taken: msg({ en: 'This value is already taken.', ru: 'Это значение уже занято.' }),
  incorrect: msg({ en: 'The value is not correct.', ru: 'Значение указано неверно.' }),
  out_of_range: msg({ en: 'The value is out of range.', ru: 'Значение вне допустимых границ.' }),
  too_long: msg({ en: 'The value is too long.', ru: 'Слишком длинное значение.' }),
  unknown: msg({ en: 'This field is not expected.', ru: 'Это поле не ожидается.' }),
} as const;

/** Texts of a field's own problems, by `FieldError.code`, that replace the generic ones. */
export type OwnFieldTexts = Readonly<Record<string, Readonly<Record<string, () => string>>>>;

/**
 * The text to show under each rejected field, by path: the field's own text for the code when it
 * has one (`own[path][code]`), otherwise the generic text of the code. The first problem of a
 * field wins.
 */
export function fieldErrorTexts(
  fields: readonly FieldError[],
  own: OwnFieldTexts = {},
): Record<string, string> {
  const generic: Readonly<Record<string, (() => string) | undefined>> = fieldErrorText;
  const texts: Record<string, string> = {};
  for (const field of fields) {
    if (field.path in texts) continue;
    const text = own[field.path]?.[field.code] ?? generic[field.code] ?? fieldErrorText.invalid;
    texts[field.path] = text();
  }
  return texts;
}
