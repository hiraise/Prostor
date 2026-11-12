// Texts of the notification bell and the inbox (NTF, plan step 1.10 §4.1).
import { msg, plural } from '#lib/i18n/message.ts';

export const bell = msg({ en: 'Notifications', ru: 'Уведомления' });
export const bellUnread = msg<{ count: number }>({
  en: ({ count }) =>
    `Notifications: ${plural('en', count, { one: '# unread item', other: '# unread items' })}`,
  ru: ({ count }) =>
    `Уведомления: ${plural('ru', count, {
      one: '# непрочитанное',
      few: '# непрочитанных',
      many: '# непрочитанных',
      other: '# непрочитанных',
    })}`,
});

export const folderAll = msg({ en: 'All', ru: 'Все' });
export const folderDirect = msg({ en: 'Mentions', ru: 'Упоминания' });
export const folderSubscriptions = msg({ en: 'Subscriptions', ru: 'Подписки' });
export const folderSystem = msg({ en: 'System', ru: 'Система' });

export const unreadOnly = msg({ en: 'Unread only', ru: 'Только непрочитанные' });
export const unreadRow = msg({ en: 'unread', ru: 'непрочитано' });

export const markRead = msg({ en: 'Mark as read', ru: 'Отметить прочитанным' });
export const markUnread = msg({ en: 'Mark as unread', ru: 'Отметить непрочитанным' });
export const markAllRead = msg({ en: 'Mark all as read', ru: 'Прочитать все' });
export const markThreadRead = msg({ en: 'Mark thread as read', ru: 'Прочитать тред' });

export const showMore = msg({ en: 'Show more', ru: 'Показать ещё' });

export const inboxEmpty = msg({
  en: 'No notifications yet',
  ru: 'Уведомлений пока нет',
});
export const inboxEmptyUnread = msg({
  en: 'No unread notifications',
  ru: 'Непрочитанных уведомлений нет',
});
export const inboxFailed = msg({
  en: 'The inbox could not be loaded',
  ru: 'Не удалось загрузить уведомления',
});
export const retry = msg({ en: 'Try again', ru: 'Повторить' });

// What a row says happened (the kind words of NTF-062, the row forms).
export const kindCreated = msg({ en: 'created', ru: 'создана' });
export const kindUpdated = msg({ en: 'changed', ru: 'изменена' });
export const kindResolved = msg({ en: 'resolved', ru: 'решена' });
export const kindComment = msg({ en: 'comment', ru: 'комментарий' });
export const kindVote = msg({ en: 'vote', ru: 'голос' });
export const kindTagAdded = msg({ en: 'tag added', ru: 'тег добавлен' });
export const kindTagRemoved = msg({ en: 'tag removed', ru: 'тег убран' });
export const kindSystem = msg({ en: 'system', ru: 'система' });

export const kindWords: Readonly<Record<string, () => string>> = {
  created: kindCreated,
  updated: kindUpdated,
  resolved: kindResolved,
  comment: kindComment,
  vote: kindVote,
  tag_added: kindTagAdded,
  tag_removed: kindTagRemoved,
  system: kindSystem,
};

// The states of a job report (`system` rows, ADM-031).
export const jobFinished = msg({ en: 'finished', ru: 'завершено' });
export const jobFailed = msg({ en: 'failed', ru: 'не удалось' });
export const jobStates: Readonly<Record<string, () => string>> = {
  finished: jobFinished,
  failed: jobFailed,
};

// ---- The «Уведомления» tab of the account page and subscriptions (plan step 1.10 §4.2) ----

export const settingsTitle = msg({ en: 'Notifications', ru: 'Уведомления' });

export const channelsLegend = msg({ en: 'Letters', ru: 'Письма' });
export const emailChannel = msg({
  en: 'Send letters to the profile address',
  ru: 'Присылать письма на адрес профиля',
});
export const emailFormat = msg({ en: 'Letter format', ru: 'Формат писем' });
export const formatHtml = msg({ en: 'HTML', ru: 'HTML' });
export const formatPlain = msg({ en: 'Plain text', ru: 'Простой текст' });
export const addressGoesTo = msg<{ email: string }>({
  en: ({ email }) => `Letters go to ${email}.`,
  ru: ({ email }) => `Письма уходят на ${email}.`,
});
export const noAddress = msg({ en: 'No email in the profile.', ru: 'В профиле не указана почта.' });
export const emailOffNotice = msg({
  en: 'Instance email is turned off, so no letters are sent.',
  ru: 'Почта экземпляра выключена — письма не отправляются.',
});
export const testSend = msg({ en: 'Send a test letter', ru: 'Отправить пробное письмо' });
export const testSent = msg<{ email: string }>({
  en: ({ email }) => `A test letter is on its way to ${email}.`,
  ru: ({ email }) => `Пробное письмо отправлено на ${email}.`,
});

export const eventsLegend = msg({ en: 'Events', ru: 'События' });
export const mentionsSetting = msg({ en: 'Mentions', ru: 'Упоминания' });
export const mentionsHint = msg({
  en: 'When someone mentions me.',
  ru: 'Когда кто-то упоминает меня.',
});
export const ownChangesSetting = msg({ en: 'My own changes', ru: 'Мои собственные изменения' });
export const ownChangesHint = msg({
  en: 'Also tell me about changes I made myself.',
  ru: 'Сообщать и о том, что я изменил сам.',
});

export const watchLegend = msg({ en: 'Watching', ru: 'Наблюдение' });
export const watchIntro = msg({
  en: 'What to tell me about the issues I watch:',
  ru: 'Что сообщать о задачах, за которыми я слежу:',
});
export const watchCreatedLabel = msg({ en: 'Created', ru: 'Создание' });
export const watchUpdatedLabel = msg({ en: 'Changed', ru: 'Изменения' });
export const watchResolvedLabel = msg({ en: 'Resolved', ru: 'Решение' });
export const watchCommentLabel = msg({ en: 'Comments', ru: 'Комментарии' });
export const watchVoteLabel = msg({ en: 'Votes', ru: 'Голоса' });
export const watchTagLabel = msg({ en: 'Tags', ru: 'Теги' });
export const watchLabels: Readonly<Record<string, () => string>> = {
  watch_created: watchCreatedLabel,
  watch_updated: watchUpdatedLabel,
  watch_resolved: watchResolvedLabel,
  watch_comment: watchCommentLabel,
  watch_vote: watchVoteLabel,
  watch_tag: watchTagLabel,
};

export const autowatchLegend = msg({ en: 'Auto-watch', ru: 'Автонаблюдение' });
export const autowatchIntro = msg({
  en: 'Follow these issues on my own — the star comes on by itself:',
  ru: 'Самим включать звёздочку на таких задачах:',
});
export const autowatchCreated = msg({
  en: 'Issues I publish',
  ru: 'Задачи, которые я публикую',
});
export const autowatchCommented = msg({
  en: 'Issues I comment on (my first comment)',
  ru: 'Задачи, которые я комментирую (первый комментарий)',
});
export const autowatchAssigned = msg({
  en: 'Issues assigned to me',
  ru: 'Задачи, которые мне назначают',
});
export const autowatchVoted = msg({
  en: 'Issues I vote for',
  ru: 'Задачи, за которые я голосую',
});
export const autowatchLabels: Readonly<Record<string, () => string>> = {
  autowatch_created: autowatchCreated,
  autowatch_commented: autowatchCommented,
  autowatch_assigned: autowatchAssigned,
  autowatch_voted: autowatchVoted,
};

// What a subscription may deliver (NTF-022): the ticks of the dialogs and the words of a row.
export const subCreated = msg({ en: 'Created', ru: 'Создание' });
export const subUpdated = msg({ en: 'Changed', ru: 'Изменения' });
export const subResolved = msg({ en: 'Resolved', ru: 'Решение' });
export const subComment = msg({ en: 'Comments', ru: 'Комментарии' });
export const subVote = msg({ en: 'Votes', ru: 'Голоса' });
export const subTagAdded = msg({ en: 'Tag added', ru: 'Тег добавлен' });
export const subTagRemoved = msg({ en: 'Tag removed', ru: 'Тег убран' });
export const subscriptionKindLabels: Readonly<Record<string, () => string>> = {
  created: subCreated,
  updated: subUpdated,
  resolved: subResolved,
  comment: subComment,
  vote: subVote,
  tag_added: subTagAdded,
  tag_removed: subTagRemoved,
};

export const subscriptionsTitle = msg({ en: 'Subscriptions', ru: 'Подписки' });
export const subscriptionsIntro = msg({
  en: 'Letters about every issue of a tag or a saved search you choose.',
  ru: 'Письма обо всех задачах выбранного тега или сохранённого поиска.',
});
export const noSubscriptions = msg({ en: 'No subscriptions yet', ru: 'Подписок пока нет' });
export const noSubscriptionsText = msg({
  en: 'Subscribe to a tag or a saved search to hear about every issue it covers.',
  ru: 'Подпишитесь на тег или сохранённый поиск, чтобы узнавать обо всех их задачах.',
});
export const addSubscription = msg({ en: 'Subscribe', ru: 'Подписаться' });
export const sourceTag = msg({ en: 'tag', ru: 'тег' });
export const sourceSearch = msg({ en: 'saved search', ru: 'сохранённый поиск' });
export const sourceWords: Readonly<Record<string, () => string>> = {
  tag: sourceTag,
  search: sourceSearch,
};
export const silentSubscription = msg({
  en: 'delivers nothing',
  ru: 'ничего не приходит',
});
export const editSubscriptionLabel = msg<{ name: string }>({
  en: ({ name }) => `Change the subscription on ${name}`,
  ru: ({ name }) => `Изменить подписку «${name}»`,
});
export const removeSubscriptionLabel = msg<{ name: string }>({
  en: ({ name }) => `Remove the subscription on ${name}`,
  ru: ({ name }) => `Удалить подписку «${name}»`,
});
export const removeSubscriptionTitle = msg({
  en: 'Remove the subscription?',
  ru: 'Удалить подписку?',
});
export const removeSubscriptionText = msg<{ name: string }>({
  en: ({ name }) => `Letters on “${name}” will stop coming.`,
  ru: ({ name }) => `Письма по «${name}» больше не будут приходить.`,
});
export const removeSubscription = msg({ en: 'Remove', ru: 'Удалить' });
export const subscriptionRemoved = msg({
  en: 'Subscription removed',
  ru: 'Подписка удалена',
});

export const newSubscriptionTitle = msg({ en: 'Subscribe', ru: 'Подписаться' });
export const editSubscriptionTitle = msg({
  en: 'Change the subscription',
  ru: 'Изменение подписки',
});
export const pickSource = msg({ en: 'Subscribe to', ru: 'На что подписаться' });
export const findSource = msg({ en: 'Find a tag or a search', ru: 'Найти тег или поиск' });
export const kindsLegend = msg({ en: 'What to tell me about', ru: 'О чём сообщать' });
export const alreadySubscribed = msg({ en: 'already subscribed', ru: 'уже есть подписка' });
export const pickFirst = msg({
  en: 'Choose what to subscribe to first.',
  ru: 'Сначала выберите, на что подписаться.',
});
export const pickKind = msg({
  en: 'Tick at least one event.',
  ru: 'Отметьте хотя бы одно событие.',
});
export const emptyKindsHint = msg({
  en: 'With no events ticked, the subscription stays but says nothing.',
  ru: 'Без отмеченных событий подписка останется, но ничего не будет присылать.',
});
export const save = msg({ en: 'Save', ru: 'Сохранить' });
export const subscribedToast = msg<{ name: string }>({
  en: ({ name }) => `Subscribed to ${name}.`,
  ru: ({ name }) => `Подписка на «${name}» добавлена.`,
});
export const subscriptionSaved = msg({
  en: 'Subscription saved',
  ru: 'Подписка сохранена',
});

// The subscription section of the tag and saved-search dialogs (NTF-025).
export const sectionLegend = msg({ en: 'Notifications', ru: 'Уведомления' });
export const sectionAboutTag = msg({
  en: 'Ticks apply at once; letters will come about the issues with this tag.',
  ru: 'Галочки применяются сразу; письма будут приходить о задачах с этим тегом.',
});
export const sectionAboutSearch = msg({
  en: 'Ticks apply at once; letters will come about the issues this search finds.',
  ru: 'Галочки применяются сразу; письма будут приходить о задачах, которые находит этот поиск.',
});
export const loadingFailed = msg({
  en: 'The subscription could not be loaded',
  ru: 'Не удалось загрузить подписку',
});

// The watcher popup of the issue page (NTF-015, NTF-016, plan step 1.10 §4.3).
export const watchersLoading = msg({ en: 'Loading watchers', ru: 'Загружаем наблюдателей' });
export const watchersFailed = msg({
  en: 'The watchers could not be loaded',
  ru: 'Не удалось загрузить наблюдателей',
});
export const noWatchers = msg({
  en: 'Nobody watches this issue yet',
  ru: 'Эту задачу пока никто не наблюдает',
});
export const you = msg({ en: 'you', ru: 'вы' });
export const addWatcher = msg({ en: 'Add a watcher', ru: 'Добавить наблюдателя' });
export const findWatcher = msg({
  en: 'Find a person who can read this issue',
  ru: 'Найти человека, который видит эту задачу',
});
export const removeWatcher = msg<{ name: string }>({
  en: ({ name }) => `Stop ${name} watching`,
  ru: ({ name }) => `Убрать ${name} из наблюдателей`,
});
export const watchersTitle = msg({ en: 'Watchers', ru: 'Наблюдатели' });
