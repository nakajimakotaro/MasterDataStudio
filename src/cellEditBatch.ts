import type { CellEdit } from "./types.ts";

// A fill emits one edit request per cell. Hold them until fillEnd so saving
// and repository Undo treat the drag as a single operation.
export class CellEditBatch {
  private pending: CellEdit[] | null = null;

  start() {
    this.pending = [];
  }

  request(edit: CellEdit): CellEdit[] {
    if (this.pending === null) return [edit];
    this.pending.push(edit);
    return [];
  }

  finish(): CellEdit[] {
    const edits = this.pending ?? [];
    this.pending = null;
    return edits;
  }
}
