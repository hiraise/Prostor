// Texts of the command dialog (step 2.1 §6, CMD-027…CMD-030) and of the `cmd_*` problems the
// assist answers. Voice and glossary: docs/design.md §12.
import type { CommandProblem, PlannedOperation, SuggestionGroup } from '#lib/api/schema.gen.ts';
import { getLocale } from '#lib/i18n/locale.svelte.ts';
import { msg, plural } from '#lib/i18n/message.ts';

/** A message without parameters, as the label tables type it. */
type Text = () => string;

export const dialogTitle = msg({ en: 'Commands', ru: 'Команды' });
export const inputLabel = msg({ en: 'Command', ru: 'Команда' });
export const placeholder = msg({ en: 'For example: tag bug', ru: 'Например: тег баг' });
export const completionLabel = msg({ en: 'Suggestions', ru: 'Подсказки' });
export const completionOpen = msg({ en: 'Show suggestions', ru: 'Показать подсказки' });
export const presentMark = msg({ en: 'already on the issue', ru: 'уже на задаче' });
export const ownMark = msg({ en: 'you', ru: 'вы' });

// The preview (CMD-028).
export const previewTitle = msg({ en: 'Preview', ru: 'Предпросмотр' });
export const previewEmpty = msg({
  en: 'What the line does appears here',
  ru: 'Здесь появится, что делает команда',
});
export const previewCount = msg<{ count: number }>({
  en: ({ count }) => `${String(count)} change(s)`,
  ru: ({ count }) => `изменений: ${String(count)}`,
});
export const previewExpand = msg({ en: 'Details', ru: 'Подробнее' });
export const previewCollapse = msg({ en: 'Hide details', ru: 'Свернуть' });
export const commentTitle = msg({ en: 'Comment', ru: 'Комментарий' });
export const commentPlaceholder = msg({
  en: 'The text the comment command attaches',
  ru: 'Текст, который прикрепит команда «комментарий»',
});
export const commentRow = msg({ en: 'Comment', ru: 'Комментарий' });
export const commentMissing = msg({
  en: 'The comment command needs text in the comment field',
  ru: 'Команде «комментарий» нужен текст в поле комментария',
});
export const moveNote = msg({
  en: 'Values move to the same-named fields of the new project',
  ru: 'Значения перейдут в одноимённые поля нового проекта',
});
export const deleteNote = msg({
  en: 'This deletes every target issue',
  ru: 'Это удалит все задачи, к которым применяется команда',
});
export const createValue = msg<{ name: string }>({
  en: ({ name }) => `Create “${name}”`,
  ru: ({ name }) => `Создать «${name}»`,
});
export const createValueHint = msg({
  en: 'and apply the command',
  ru: 'и применить команду',
});

// The kinds of the preview's rows.
export const kinds: Record<string, Text> = {
  set_field: msg({ en: 'Field', ru: 'Поле' }),
  tag: msg({ en: 'Tag', ru: 'Тег' }),
  move: msg({ en: 'Move to', ru: 'Перенос в' }),
  link: msg({ en: 'Link', ru: 'Связь' }),
  duplicate: msg({ en: 'Duplicate of', ru: 'Дубликат' }),
  visibility: msg({ en: 'Visibility', ru: 'Видимость' }),
  board: msg({ en: 'Board', ru: 'Доска' }),
  star: msg({ en: 'Favorites', ru: 'Избранное' }),
  vote: msg({ en: 'Vote', ru: 'Голос' }),
  comment: commentRow,
  delete: msg({ en: 'Deletion', ru: 'Удаление' }),
  clone: msg({ en: 'Clone', ru: 'Клонирование' }),
};

// The buttons.
export const apply = msg({ en: 'Apply', ru: 'Применить' });
export const applySilent = msg({
  en: 'Apply without notifications',
  ru: 'Применить без уведомлений',
});
export const applyLoading = msg({ en: 'Applying…', ru: 'Применяем…' });
export const confirmDelete = msg({ en: 'Delete', ru: 'Удалить' });
export const back = msg({ en: 'Back', ru: 'Назад' });
export const deleteConfirmTitle = msg({ en: 'Delete issues?', ru: 'Удалить задачи?' });
/** The confirm names the count it deletes — the selection toolbar's «Удалить» (step 2.2 §8.2). */
export const deleteConfirmCountTitle = msg<{ count: number }>({
  en: ({ count }) => plural('en', count, { one: 'Delete # issue?', other: 'Delete # issues?' }),
  ru: ({ count }) =>
    plural('ru', count, {
      one: 'Удалить # задачу?',
      few: 'Удалить # задачи?',
      many: 'Удалить # задач?',
      other: 'Удалить # задачи?',
    }),
});

// The bulk flow (CMD-026).
export const bulkTitle = msg<{ count: number }>({
  en: ({ count }) => `Apply a command to ${String(count)} issues`,
  ru: ({ count }) => `Применить команду к ${String(count)} задачам`,
});
export const bulkQueued = msg({ en: 'The job was queued', ru: 'Задача поставлена в очередь' });
export const bulkRunning = msg<{ done: number; total: number | null }>({
  en: ({ done, total }) => `Applied to ${String(done)} of ${total === null ? '…' : String(total)}`,
  ru: ({ done, total }) => `Применено к ${String(done)} из ${total === null ? '…' : String(total)}`,
});
export const bulkDone = msg<{ done: number; failed: number }>({
  en: ({ done, failed }) =>
    failed > 0
      ? `Done: ${String(done)}, failed: ${String(failed)}`
      : `Done: applied to ${String(done)}`,
  ru: ({ done, failed }) =>
    failed > 0
      ? `Готово: ${String(done)}, не удалось: ${String(failed)}`
      : `Готово: применено к ${String(done)}`,
});
export const bulkFailed = msg({ en: 'The job failed', ru: 'Задача не выполнена' });
export const bulkCancelled = msg({ en: 'The job was cancelled', ru: 'Задача отменена' });
export const bulkFailures = msg({ en: 'Could not apply to', ru: 'Не удалось применить к' });

// The silent prompt after a first bulk apply without silent (§6).
export const silentPromptTitle = msg({
  en: 'Apply commands to lists without notifications?',
  ru: 'Применять команды к спискам без уведомлений?',
});
export const silentPromptAbout = msg({
  en: 'Bulk commands can follow people quietly: subscribers learn of changes only from the issues themselves.',
  ru: 'Массовые команды могут идти тихо: подписчики узнают об изменениях только из самих задач.',
});
export const silentPromptYes = msg({ en: 'Yes, silently', ru: 'Да, без уведомлений' });
export const silentPromptNo = msg({ en: 'No', ru: 'Нет' });
export const silentPromptNever = msg({ en: 'Never ask again', ru: 'Больше не спрашивать' });

export const appliedToast = msg<{ count: number }>({
  en: ({ count }) => `Command applied to ${String(count)}`,
  ru: ({ count }) => `Команда применена к ${String(count)}`,
});
export const notAppliedToast = msg({
  en: 'The command was applied to nothing',
  ru: 'Команда не применилась ни к одной задаче',
});

/** The completion groups, labelled (D-061: both surfaces share the contract). */
export const groups: Record<SuggestionGroup, Text> = {
  command: msg({ en: 'Commands', ru: 'Команды' }),
  attribute: msg({ en: 'Fields', ru: 'Поля' }),
  value: msg({ en: 'Values', ru: 'Значения' }),
  link: msg({ en: 'Links', ru: 'Связи' }),
  tag: msg({ en: 'Tags', ru: 'Теги' }),
  project: msg({ en: 'Projects', ru: 'Проекты' }),
  user: msg({ en: 'People', ru: 'Люди' }),
  group: msg({ en: 'Groups', ru: 'Группы' }),
  board: msg({ en: 'Boards', ru: 'Доски' }),
  sprint: msg({ en: 'Sprints', ru: 'Спринты' }),
  special: msg({ en: 'Special', ru: 'Особые' }),
  empty: msg({ en: 'Empty', ru: 'Пустые' }),
  date: msg({ en: 'Dates', ru: 'Даты' }),
  issue: msg({ en: 'Issues', ru: 'Задачи' }),
  recent: msg({ en: 'Recent', ru: 'Недавние' }),
  keyword: msg({ en: 'Keywords', ru: 'Ключевые слова' }),
  sort_key: msg({ en: 'Sort fields', ru: 'Поля сортировки' }),
  sort_direction: msg({ en: 'Direction', ru: 'Направление' }),
};

/** The class names the server's `matches` param carries, in the person's language. */
const MATCHES: Record<string, { en: string; ru: string }> = {
  projects: { en: 'projects', ru: 'проектам' },
  tags: { en: 'tags', ru: 'тегам' },
  accounts: { en: 'accounts', ru: 'людям' },
  boards: { en: 'boards', ru: 'доскам' },
  'link names': { en: 'link names', ru: 'именам связей' },
  'a project and a tag': { en: 'a project and a tag', ru: 'проекту и тегу' },
};

/** One problem's text, by its `cmd_*` code; the params say what the code names. */
export function problemText(problem: CommandProblem): string {
  const field = problem.params.field;
  const matches = problem.params.matches;
  const fields = problem.params.fields;
  const permission = problem.params.permission;
  switch (problem.code) {
    case 'cmd_unknown_word':
      return msg({ en: 'An unknown word', ru: 'Неизвестное слово' })();
    case 'cmd_value_expected':
      return msg({ en: 'A value is expected here', ru: 'Здесь ожидается значение' })();
    case 'cmd_unknown_value':
      if (field)
        return msg<{ field: string }>({
          en: ({ field }) => `No such value in “${field}”`,
          ru: ({ field }) => `В поле «${field}» нет такого значения`,
        })({ field });
      return msg({ en: 'No such value', ru: 'Нет такого значения' })();
    case 'cmd_ambiguous':
      if (fields)
        return msg<{ fields: string }>({
          en: ({ fields }) => `The word fits several fields: ${fields}`,
          ru: ({ fields }) => `Слово подходит нескольким полям: ${fields}`,
        })({ fields });
      if (matches) {
        const names = MATCHES[matches] ?? { en: matches, ru: matches };
        return msg<{ names: string }>({
          en: ({ names }) => `Ambiguous: fits ${names}`,
          ru: ({ names }) => `Неоднозначно: подходит ${names}`,
        })({ names: getLocale() === 'ru' ? names.ru : names.en });
      }
      return msg({ en: 'Ambiguous', ru: 'Неоднозначно' })();
    case 'cmd_not_empty':
      return msg({ en: 'The field cannot be empty', ru: 'Поле нельзя очистить' })();
    case 'cmd_wrong_kind':
      return msg({
        en: 'A value of this kind does not fit the field',
        ru: 'Такое значение полю не подходит',
      })();
    case 'cmd_text_field':
      return msg({
        en: 'A text field is edited on the page, not by a command',
        ru: 'Текстовое поле меняют на странице, не командой',
      })();
    case 'cmd_own_issue_vote':
      return msg({ en: 'One cannot vote for one’s own issue', ru: 'За свою задачу не голосуют' })();
    case 'cmd_no_permission':
      return msg<{ permission: string }>({
        en: ({ permission }) => `This needs the ${permission} right`,
        ru: ({ permission }) => `Для этого нужно право ${permission}`,
      })({ permission: permission ?? '' });
    case 'cmd_target_not_found':
      return msg({ en: 'No such issue', ru: 'Задача не найдена' })();
    case 'cmd_board_no_sprints':
      return msg({ en: 'The board has no sprints', ru: 'У доски нет спринтов' })();
    default:
      return problem.code;
  }
}

/** What one preview row is about, when the kind itself does not say it. */
export function rowTitle(operation: PlannedOperation): string {
  const first = operation.names?.[0];
  const kind = kinds[operation.kind];
  // A set names its field — the row's own title; every other kind says itself.
  return operation.kind === 'set_field'
    ? (first ?? (kind ? kind() : operation.kind))
    : kind
      ? kind()
      : operation.kind;
}
