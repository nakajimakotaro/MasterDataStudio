import { test } from "node:test";
import assert from "node:assert/strict";
import { emptyConflictFilter, filterConflicts, indexConflicts, unresolvedIds } from "../src/conflictModel.ts";
import type { Conflict } from "../src/types.ts";

const conflicts: Conflict[] = Array.from({ length: 50_000 }, (_, i) => ({
  id: `conflict-${i}`, kind: "cell", masterId: i % 2 ? "item" : "enemy", primaryKey: [String(i), "日本語"],
  column: i % 4 ? "name" : "hp", base: "100", ours: "120", theirs: i === 49_999 ? "Unique Value" : "150",
  resolution: i % 5 === 0 ? { kind: "ours" } : null,
}));
const index = indexConflicts(conflicts);
test("50,000 conflicts can be filtered across every page without selecting resolved entries", () => {
  const all = filterConflicts(index, emptyConflictFilter);
  assert.equal(all.length, 40_000);
  assert.equal(unresolvedIds(all).size, 40_000);
  const selected = filterConflicts(index, { ...emptyConflictFilter, master: "enemy", column: "hp" });
  assert.equal(selected.length, 10_000);
  assert.equal(unresolvedIds(selected.slice(0, 100)).size, 100);
  assert.equal(unresolvedIds(selected).has("conflict-0"), false);
  assert.equal(new Set(selected.map(c => c.id)).size, selected.length);
});
test("search finds values beyond the first page and preserves compound keys", () => {
  assert.equal(filterConflicts(index, { ...emptyConflictFilter, query: "unique VALUE" })[0].id, "conflict-49999");
  assert.equal(filterConflicts(index, { ...emptyConflictFilter, query: '["49999","日本語"]' }).length, 1);
  assert.equal(filterConflicts(index, { ...emptyConflictFilter, query: "not found" }).length, 0);
});
test("resolved and all views retain decisions, while select unresolved never overwrites them", () => {
  assert.equal(filterConflicts(index, { ...emptyConflictFilter, status: "resolved" }).length, 10_000);
  const all = filterConflicts(index, { ...emptyConflictFilter, status: "all" });
  assert.equal(all.length, 50_000);
  assert.equal(unresolvedIds(all).size, 40_000);
});
