// Texts of the jobs screen (ADM-031, ADM-032; plan step 1.11 §6.2).
// Voice and glossary: docs/design.md §12.
import { msg } from '#lib/i18n/message.ts';

export const jobsTitle = msg({ en: 'Jobs', ru: 'Задания' });
export const jobsIntro = msg({
  en: 'Long work of the instance runs here in the background: deletions, letters, re-rendering. It survives a restart; a finished or failed job stays until its row is no longer needed (cleanup is not built yet).',
  ru: 'Долгая работа инстанса идёт здесь в фоне: удаления, письма, перерисовка текстов. Она переживает перезапуск; законченное и упавшее задание остаётся в списке (очистки пока нет).',
});
export const readOnlyNote = msg({
  en: 'Only system administrators can act on jobs.',
  ru: 'Действовать с заданиями могут только системные администраторы.',
});

// The filter.
export const filterLabel = msg({ en: 'Filter jobs', ru: 'Фильтр заданий' });
export const filterHint = msg({
  en: 'Words of the kind or the initiator; a word with a leading “-” leaves them out.',
  ru: 'Слова о виде задания или том, кто его запустил; слово с минусом «-» впереди исключает.',
});
export const allStates = msg({ en: 'All', ru: 'Все' });
export const stateNames = {
  queued: msg({ en: 'Queued', ru: 'В очереди' }),
  running: msg({ en: 'Running', ru: 'Выполняется' }),
  finished: msg({ en: 'Finished', ru: 'Завершено' }),
  failed: msg({ en: 'Failed', ru: 'Ошибка' }),
  cancelled: msg({ en: 'Cancelled', ru: 'Отменено' }),
} as const;

// One row.
export const initiatorColumn = msg({ en: 'Started by', ru: 'Запустил' });
export const startedColumn = msg({ en: 'Started', ru: 'Начато' });
export const byServer = msg({ en: 'The server itself', ru: 'Сам сервер' });
export const progressOf = msg<{ done: number; total: number | null }>({
  en: ({ done, total }) =>
    total === null ? `${String(done)} done` : `${String(done)} of ${String(total)}`,
  ru: ({ done, total }) =>
    total === null ? `готово ${String(done)}` : `${String(done)} из ${String(total)}`,
});
export const attemptsOf = msg<{ attempts: number; max: number }>({
  en: ({ attempts, max }) => `try ${String(attempts + 1)} of ${String(max)}`,
  ru: ({ attempts, max }) => `попытка ${String(attempts + 1)} из ${String(max)}`,
});
export const cancelJob = msg({ en: 'Cancel', ru: 'Отменить' });
export const retryJob = msg({ en: 'Retry', ru: 'Повторить' });
export const cancelTitle = msg({ en: 'Cancel the job?', ru: 'Отменить задание?' });
export const cancelConfirm = msg({ en: 'Cancel job', ru: 'Отменить задание' });
export const cancelQueuedText = msg({
  en: 'The job has not started yet; it will not run.',
  ru: 'Задание ещё не началось — оно не выполнится.',
});
export const cancelRunningText = msg({
  en: 'The job will stop where it is; what it has finished stays finished.',
  ru: 'Задание остановится, где стоит; сделанное останется сделанным.',
});
export const stoppedAsk = msg({
  en: 'Stop asked; waiting for the job to notice.',
  ru: 'Просим остановиться; ждём, когда задание заметит.',
});
export const retryQueued = msg({
  en: 'A fresh job is queued',
  ru: 'Новое задание встало в очередь',
});
export const cancelledToast = msg({ en: 'The job is cancelled', ru: 'Задание отменено' });
export const noJobs = msg({
  en: 'No jobs yet. Long work — deleting a project, sending letters — will appear here.',
  ru: 'Заданий пока нет. Долгая работа — удаление проекта, письма — появится здесь.',
});
export const nothingFound = msg({
  en: 'Nothing matches the filter.',
  ru: 'Под фильтр ничего не подошло.',
});
export const showMore = msg({ en: 'Show more', ru: 'Показать ещё' });
export const refresh = msg({ en: 'Refresh', ru: 'Обновить' });
