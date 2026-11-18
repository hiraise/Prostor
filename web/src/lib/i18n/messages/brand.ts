// The product name in the interface languages (D-066): Latin in English, Russian in Russian.
// The wordmark splits the same name into the regular lead and the bold tail (design.md §2,
// "Name").
import { msg } from '../message.ts';

export const brand = msg({ en: 'Prostor', ru: 'Простор' });
export const brandLead = msg({ en: 'Pros', ru: 'Про' });
export const brandTail = msg({ en: 'tor', ru: 'стор' });
