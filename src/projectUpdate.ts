import type { ProjectUpdate, RepositoryState, Snapshot } from "./types";

/** Keep untouched masters and rows shared with the cached project. */
export function applyProjectUpdate(current: Snapshot, update: ProjectUpdate): Snapshot {
  if (current.root !== update.root || current.revision !== update.data.baseRevision) {
    throw new Error("編集状態が更新されました。もう一度操作してください。");
  }
  const masters = { ...current.data.masters };
  for (const [id, change] of Object.entries(update.data.masters)) {
    if (change === null) {
      delete masters[id];
      continue;
    }
    let entry;
    if (change.kind === "replace") {
      entry = change.entry;
    } else {
      const previous = Object.hasOwn(masters, id) ? masters[id].data : null;
      if (!previous) throw new Error(`Master がありません: ${id}`);
      const rows = previous.table.rows.slice(0, change.rowCount);
      for (const [index, row] of change.rows) rows[index] = row;
      entry = {
        error: change.error,
        data: {
          table: { columns: previous.table.columns, rows },
          comments: change.comments,
          scripts: change.scripts,
          scriptError: change.scriptError,
        },
      };
    }
    Object.defineProperty(masters, id, { value: entry, enumerable: true, configurable: true, writable: true });
  }
  return {
    ...current,
    revision: update.revision,
    canUndo: update.canUndo,
    canRedo: update.canRedo,
    git: { ...current.git, branch: update.branch, protected: update.protected },
    gitStale: current.gitStale || current.revision !== update.revision || current.git.branch !== update.branch,
    data: { config: update.data.config, masters },
  };
}

/** Ignore late reads from a project/revision which is no longer displayed. */
export function applyRepositoryState(current: Snapshot | null | undefined, state: RepositoryState): Snapshot | null | undefined {
  if (!current || current.root !== state.root || current.revision !== state.revision) return current;
  return { ...current, git: state.git, changes: state.changes, scriptChanges: state.scriptChanges, changesError: state.changesError, gitStale: false };
}
