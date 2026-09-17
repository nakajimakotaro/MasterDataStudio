import { test } from "node:test";
import assert from "node:assert/strict";
import { applyProjectUpdate, applyRepositoryState } from "../src/projectUpdate.ts";
import type { Master, ProjectUpdate, RepositoryState, Snapshot } from "../src/types.ts";

function fixture(): Snapshot {
  const master: Master = {
    table: { columns: ["id", "value"], rows: [["1", "a"], ["2", "b"], ["3", "c"]] },
    comments: { version: 1, table: null, rows: [], cells: [] },
    scripts: { version: 1, columns: [] },
  };
  return {
    root: "/project", name: "project", identity: { name: "Test", email: "test@example.com" },
    revision: 3, canUndo: false, canRedo: false, changes: [], changesError: null, merge: null,
    git: { branch: "work", upstream: null, ahead: 0, behind: 0, protected: false, trackedDirty: false, mergeInProgress: false, remotes: [], branches: ["work"] },
    data: {
      config: { version: 1, git: { protectedBranches: [] }, masters: { a: { path: "a.csv", primaryKey: ["id"] }, b: { path: "b.csv", primaryKey: ["id"] } } },
      masters: { a: { data: master, error: null }, b: { data: structuredClone(master), error: null } },
    },
  };
}

function update(current: Snapshot, masters: ProjectUpdate["data"]["masters"]): ProjectUpdate {
  return { root: current.root, branch: current.git.branch, protected: current.git.protected, revision: current.revision + 1, canUndo: true, canRedo: false, data: { baseRevision: current.revision, config: current.data.config, masters } };
}

test("cell updates preserve untouched row and master references without mutating the cache", () => {
  const current = fixture();
  const master = current.data.masters.a.data!;
  const before = structuredClone(current);
  const next = applyProjectUpdate(current, update(current, {
    a: { kind: "rows", rows: [[1, ["2", "edited"]]], rowCount: 3, comments: master.comments, scripts: master.scripts!, scriptError: null, error: null },
  }));
  assert.equal(next.data.masters.b, current.data.masters.b);
  assert.equal(next.data.masters.a.data!.table.rows[0], master.table.rows[0]);
  assert.equal(next.data.masters.a.data!.table.rows[2], master.table.rows[2]);
  assert.deepEqual(next.data.masters.a.data!.table.rows[1], ["2", "edited"]);
  assert.equal(next.revision, 4);
  assert.equal(next.canUndo, true);
  assert.deepEqual(current, before);
});

test("row patches handle canonical reordering, insertion and truncation", () => {
  let current = fixture();
  for (const rows of [
    [["0", "new"], ["1", "a"], ["2", "b"], ["3", "c"]],
    [["1", "a"], ["3", "c"]],
    [],
  ]) {
    const master = current.data.masters.a.data!;
    current = applyProjectUpdate(current, update(current, {
      a: { kind: "rows", rows: rows.map((row, i) => [i, row]), rowCount: rows.length, comments: master.comments, scripts: master.scripts!, scriptError: null, error: null },
    }));
    assert.deepEqual(current.data.masters.a.data!.table.rows, rows);
  }
});

test("master replacements, removal and config-only changes apply together", () => {
  const current = fixture();
  const replacement = { data: null, error: "CSV error" };
  const patch = update(current, { a: null, b: { kind: "replace", entry: replacement }, c: { kind: "replace", entry: current.data.masters.a } });
  patch.data.config = { ...current.data.config, masters: { c: { path: "c.csv", primaryKey: ["id"] } } };
  const next = applyProjectUpdate(current, patch);
  assert.equal(Object.hasOwn(next.data.masters, "a"), false);
  assert.equal(next.data.masters.b, replacement);
  assert.equal(next.data.masters.c, current.data.masters.a);
  assert.equal(next.data.config, patch.data.config);
  const configOnly = applyProjectUpdate(current, update(current, {}));
  assert.equal(configOnly.data.masters.a, current.data.masters.a);
});

test("stale updates and updates from another project are rejected", () => {
  const current = fixture();
  const patch = update(current, {});
  assert.throws(() => applyProjectUpdate({ ...current, revision: 9 }, patch), /編集状態/);
  assert.throws(() => applyProjectUpdate({ ...current, root: "/other" }, patch), /編集状態/);
});

test("editing keeps last fetched Git metadata but marks it stale until a matching refresh", () => {
  const current = fixture();
  current.changes = [{ kind: "addedMaster", masterId: "a" }];
  const patch = update(current, {});
  patch.protected = true;
  const edited = applyProjectUpdate(current, patch);
  assert.equal(edited.gitStale, true);
  assert.equal(edited.git.protected, true);
  assert.equal(edited.changes, current.changes);
  const state: RepositoryState = { root: edited.root, revision: edited.revision, git: { ...edited.git, trackedDirty: true }, changes: [], scriptChanges: ["gamemasterstudio/scripts/a.json"], changesError: null };
  const fresh = applyRepositoryState(edited, state)!;
  assert.equal(fresh.gitStale, false);
  assert.equal(fresh.git.trackedDirty, true);
  assert.equal(fresh.data, edited.data);
  assert.deepEqual(fresh.scriptChanges, state.scriptChanges);
  assert.equal(applyRepositoryState(edited, { ...state, revision: state.revision - 1 }), edited);
  assert.equal(applyRepositoryState(edited, { ...state, root: "/other" }), edited);
  assert.equal(applyRepositoryState(null, state), null);
});

test("a no-op edit does not invalidate fresh Git metadata or clear a pending refresh", () => {
  const current = fixture();
  const patch = update(current, {});
  patch.revision = current.revision;
  assert.equal(applyProjectUpdate(current, patch).gitStale, false);
  assert.equal(applyProjectUpdate({ ...current, gitStale: true }, patch).gitStale, true);
});
