// The board's zoom on phones (BRD-088, D-044): one column at a time, swiped, or all
// columns narrow with the smallest cards. It only exists below `NARROW`, so it is kept in this
// browser per board rather than in the person's board profile.
import { readStored, writeStored } from '#lib/storage.ts';

export const ZOOMS = ['column', 'all'] as const;
export type Zoom = (typeof ZOOMS)[number];

const key = (board: number) => `taskmaster.board-zoom.${String(board)}`;

export class BoardZoom {
  value = $state<Zoom>('column');
  readonly #board: number;

  constructor(board: number) {
    this.#board = board;
    const stored = readStored(key(board));
    if (stored === 'all') this.value = 'all';
  }

  set(zoom: Zoom): void {
    this.value = zoom;
    writeStored(key(this.#board), zoom === 'column' ? null : zoom);
  }
}
