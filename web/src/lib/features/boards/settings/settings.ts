// Saving board settings (BRD-086): each change on its own, against the version the panel shows.
// A stale version reloads the board and asks to repeat the change.
import { ApiError } from '#lib/api/client.ts';
import type { Board, BoardPatch } from '#lib/api/schema.gen.ts';
import { refusal } from '#lib/features/fields/index.ts';

import { reloadBoard, updateBoard } from '../boards.ts';
import * as m from '../messages.ts';

/** The tabs of the settings panel. */
export type SettingsTab = 'general' | 'columns' | 'lanes' | 'card' | 'backlog';

/** What went wrong with a change: texts by field, and one for the whole change. */
export interface SaveProblem {
  fields: Record<string, string>;
  message: string | null;
}

/** Saves a change of the board; `null` when it went through. */
export async function saveBoard(
  board: Board,
  patch: Omit<BoardPatch, 'version'>,
  paths: readonly string[] = [],
): Promise<SaveProblem | null> {
  try {
    await updateBoard(board.id, { ...patch, version: board.version });
    return null;
  } catch (error) {
    if (error instanceof ApiError && error.code === 'conflict') {
      await reloadBoard(board.id).catch(() => undefined);
      return { fields: {}, message: m.settingsChanged() };
    }
    const found = refusal(error, { paths });
    return { fields: found.fields, message: found.message };
  }
}
