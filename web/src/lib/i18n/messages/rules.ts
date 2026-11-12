// Texts of the built-in rules' messages (AUT-023, §3.11): each code of a mutation answer's
// `messages` has its text here; the params arrive already resolved to names by the store.
// Voice and glossary: docs/design.md §12.
import { msg } from '#lib/i18n/message.ts';

/** AUT-063: the assignee is outside the issue's audience. */
export const assigneeNotVisible = msg<{ assignee: string }>({
  en: ({ assignee }) => `The assignee ${assignee} cannot see this issue`,
  ru: ({ assignee }) => `Исполнитель ${assignee} не видит эту задачу`,
});

/** AUT-079: the comment counted as a vote. */
export const voteAdded = msg({
  en: '+1 counts as a vote',
  ru: '+1 — это голос',
});

/** AUT-090: the value's owner became the assignee. */
export const assignedToOwner = msg<{ assignee: string; value: string }>({
  en: ({ assignee, value }) => `The issue is assigned to the owner of “${value}”: ${assignee}`,
  ru: ({ assignee, value }) => `Задача назначена владельцу значения «${value}» — ${assignee}`,
});

/** AUT-090: the value's owner is not among the people the assignee field can take. */
export const ownerNotAssignable = msg<{ owner: string; value: string }>({
  en: ({ owner, value }) => `The owner of “${value}”, ${owner}, is not among the assignees`,
  ru: ({ owner, value }) =>
    `Владелец значения «${value}» — ${owner} — не входит в набор исполнителей`,
});

/** AUT-094: the parent took a value of its state field. */
export const parentSet = msg<{ issue: string; value: string }>({
  en: ({ issue, value }) => `${issue} moved to “${value}”`,
  ru: ({ issue, value }) => `${issue} переведена в «${value}»`,
});

/** AUT-094: the parent is open again. */
export const parentReopened = msg<{ issue: string }>({
  en: ({ issue }) => `${issue} is open again`,
  ru: ({ issue }) => `${issue} снова открыта`,
});

/** AUT-071: the duplicates of a linked issue moved to the chain's root. */
export const linkReattached = msg<{ issue: string }>({
  en: ({ issue }) => `Duplicates now point to ${issue}`,
  ru: ({ issue }) => `Дубликаты теперь указывают на ${issue}`,
});

/** AUT-071: the original took the duplicate's higher priority. */
export const priorityRaised = msg<{ value: string }>({
  en: ({ value }) => `Priority raised to “${value}”`,
  ru: ({ value }) => `Приоритет поднят до «${value}»`,
});

/** AUT-066: the copy, published; `issue` is its readable ID. */
export const cloneCreated = msg<{ issue: string }>({
  en: ({ issue }) => `Copy created: ${issue}`,
  ru: ({ issue }) => `Копия создана: ${issue}`,
});

/** AUT-066: the copy, kept as the actor's draft; `issue` is the source's readable ID. */
export const draftCopied = msg<{ issue: string }>({
  en: ({ issue }) => `Draft copied from ${issue}`,
  ru: ({ issue }) => `Черновик скопирован с ${issue}`,
});

/** A code this build does not know; the rule still did something. */
export const ruleRan = msg({
  en: 'A rule ran',
  ru: 'Сработало правило',
});
