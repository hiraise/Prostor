// Texts of the admin mail settings screen (NTF-002, NTF-004, NTF-006, NTF-007). Voice and
// glossary: docs/design.md §12.
import { msg } from '#lib/i18n/message.ts';

export const mailTitle = msg({ en: 'Mail', ru: 'Почта' });
export const mailIntro = msg({
  en: 'Where the outgoing email of this instance comes from: the mail server, its credentials, the sender of every letter. Secrets are stored encrypted and never shown again — only that one is stored.',
  ru: 'Откуда приходит исходящая почта этого инстанса: почтовый сервер, его учётные данные, отправитель всех писем. Секреты хранятся зашифрованными и больше не показываются — только отметка, что они сохранены.',
});
export const transportLegend = msg({ en: 'Mail server', ru: 'Почтовый сервер' });
export const smtpUrlLabel = msg({ en: 'Server', ru: 'Сервер' });
export const smtpUrlHint = msg({
  en: 'smtp://host:587 (STARTTLS when offered) or smtps://host:465 (TLS from the start); user:password@ can live in the address.',
  ru: 'smtp://host:587 (STARTTLS, если сервер предлагает) или smtps://host:465 (TLS сразу); user:password@ можно указать прямо в адресе.',
});
export const usernameLabel = msg({ en: 'User name', ru: 'Имя пользователя' });
export const usernameHint = msg({
  en: 'When the server asks for one the address does not carry.',
  ru: 'Если сервер требует имя, которого нет в адресе.',
});
export const passwordLabel = msg({ en: 'Password', ru: 'Пароль' });
export const passwordHint = msg({
  en: 'With 2-step verification on, the provider’s app password.',
  ru: 'При двухфакторной аутентификации — пароль приложения от провайдера.',
});
export const senderLegend = msg({ en: 'Sender', ru: 'Отправитель' });
export const fromLabel = msg({ en: 'From', ru: 'От кого' });
export const fromHint = msg({
  en: 'A real mailbox of your domain, with SPF in place, so letters do not land in spam.',
  ru: 'Настоящий ящик на вашем домене с настроенным SPF, чтобы письма не попадали в спам.',
});
export const replyToLabel = msg({ en: 'Reply-To', ru: 'Адрес для ответов' });
export const replyToHint = msg({
  en: 'Empty: replies go to the sender.',
  ru: 'Пусто — ответы уходят отправителю.',
});
export const switchLegend = msg({ en: 'The switch', ru: 'Переключатель' });
export const emailEnabledLabel = msg({ en: 'Email is on', ru: 'Почта включена' });
export const emailEnabledHint = msg({
  en: 'While off, no letter leaves the instance: notifications, invitations, verification and password resets.',
  ru: 'Пока выключена, инстанс не отправляет ни одного письма: ни уведомления, ни приглашения, ни подтверждения, ни сбросы пароля.',
});
export const storedMark = msg({ en: 'Stored', ru: 'Сохранено' });
export const clearSecret = msg({
  en: 'Remove the stored value (an empty field keeps it)',
  ru: 'Убрать сохранённое значение (пустое поле не меняет его)',
});
export const saveSettings = msg({ en: 'Save', ru: 'Сохранить' });
export const savedToast = msg({ en: 'Mail settings saved', ru: 'Настройки почты сохранены' });
export const testLegend = msg({ en: 'Test', ru: 'Проверка' });
export const testIntro = msg({
  en: 'Send a letter to any address through the saved transport; if the server complains, its answer is shown in full.',
  ru: 'Отправьте письмо на любой адрес через сохранённый транспорт; если сервер откажет, его ответ показывается целиком.',
});
export const testToLabel = msg({ en: 'To', ru: 'Кому' });
export const testSend = msg({ en: 'Send a test letter', ru: 'Отправить проверочное письмо' });
export const testSent = msg({ en: 'The letter went out', ru: 'Письмо отправлено' });
export const offerOn = msg({
  en: 'The transport works. Switch email on so letters can leave.',
  ru: 'Транспорт работает. Включите почту, чтобы письма уходили.',
});
export const switchOn = msg({ en: 'Switch email on', ru: 'Включить почту' });
export const envNotice = msg({
  en: 'Until a server is saved here, the transport comes from the environment (PROSTOR_SMTP_URL).',
  ru: 'Пока здесь не сохранён сервер, транспорт берётся из переменных окружения (PROSTOR_SMTP_URL).',
});
export const offNotice = msg({
  en: 'Email is off: no letter leaves the instance.',
  ru: 'Почта выключена: инстанс не отправляет писем.',
});
export const readOnlyNote = msg({
  en: 'Only system administrators can change the mail settings.',
  ru: 'Менять настройки почты могут только системные администраторы.',
});
