// Texts of page-level states: not found, forbidden, failed to load (UI-036).
import { msg } from '../message.ts';

// One text for missing and inaccessible objects, so a page never reveals that something exists.
export const notFoundTitle = msg({ en: 'Nothing here', ru: 'Здесь ничего нет' });
export const notFoundText = msg({
  en: 'This page does not exist, or you do not have access to it.',
  ru: 'Такой страницы нет, или у вас нет к ней доступа.',
});
export const toHomePage = msg({ en: 'Go to the home page', ru: 'На главную' });

export const forbiddenTitle = msg({
  en: 'Only administrators can open this',
  ru: 'Это могут открыть только администраторы',
});
export const forbiddenText = msg({
  en: 'If you need access, ask an administrator of your Prostor.',
  ru: 'Если вам нужен доступ, обратитесь к администратору Простора.',
});

export const loadFailedTitle = msg({
  en: 'Could not load the page',
  ru: 'Не удалось загрузить страницу',
});
