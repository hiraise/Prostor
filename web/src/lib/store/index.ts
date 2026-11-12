// The app's one data store (architecture.md §15). A feature adds its entity kinds where it
// defines them, by declaration merging, and registers how to load them:
//
//   declare module '#lib/store/index.ts' {
//     interface EntityKinds { issue: Issue }
//   }
//   store.defineKind('issue', { load: (ids, form) => … });
import { LiveConnection } from '#lib/live/connection.ts';
import { LiveSync } from '#lib/live/sync.ts';
import { sessionEndedOnServer } from '#lib/session/index.ts';

import { DataStore } from './data.ts';

/** Entity kinds of the app: kind name → entity type. Each feature adds its own. */
// eslint-disable-next-line @typescript-eslint/no-empty-object-type -- filled by declaration merging
export interface EntityKinds {}

export const store = new DataStore<EntityKinds>();

/** Applies live events of followed topics to `store`: `liveSync.follow(liveConnection(), topic)`. */
export const liveSync = new LiveSync({
  store: store.entities,
  refetch: (kind, ids) => store.refetch(kind, ids),
});

let shared: LiveConnection | null = null;

/** The tab's live connection, created on first use. Closed for an ended session, it ends it here. */
export function liveConnection(): LiveConnection {
  shared ??= new LiveConnection({ onSessionEnded: sessionEndedOnServer });
  return shared;
}

export { ConflictError, EntityHandle, type Changes, type EditRequest } from './data.ts';
export type { Entity, Form } from './entities.ts';
export {
  keyText,
  LIVE_BATCH_MS,
  type ListHandle,
  type ListSpec,
  type LiveSource,
  type Page,
} from './queries.ts';
