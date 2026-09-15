import { test } from "node:test";
import assert from "node:assert/strict";
import { buildReviewTable } from "../src/reviewModel.ts";
import type { ChangeReviewData, Master, SemanticChange } from "../src/types.ts";

function master(columns: string[], rows: string[][]): Master {
  return { table: { columns, rows }, comments: { version: 1, table: null, rows: [], cells: [] } };
}
function data(before: Master | null, after: Master | null, changes: SemanticChange[], primaryKey = ["id"]): ChangeReviewData {
  const project = (value: Master | null) => ({
    config: { version: 1, git: { protectedBranches: [] }, masters: value ? { enemy: { path: "enemy.csv", primaryKey } } : {} },
    masters: value ? { enemy: { data: value, error: null } } : {},
  });
  return { before: before ? project(before) : null, after: project(after), changes } as ChangeReviewData;
}

test("retains removed rows and columns with original values, including intersecting deletions", () => {
  const review = buildReviewTable(data(
    master(["id", "name", "hp"], [["1", "Slime", "100"], ["2", "Bat", "50"], ["3", "Wolf", "80"]]),
    master(["id", "hp"], [["1", "100"], ["3", "80"]]),
    [{ kind: "deletedRow", masterId: "enemy", primaryKey: ["2"] }, { kind: "deletedColumn", masterId: "enemy", column: "name" }],
  ), "enemy")!;
  assert.deepEqual(review.columns.map(c => c.name), ["id", "name", "hp"]);
  assert.deepEqual(review.rows.map(r => r.key), [["1"], ["2"], ["3"]]);
  assert.equal(review.rows[1].cells.get("hp")?.before, "50");
  assert.equal(review.rows[1].cells.get("hp")?.after, null);
  assert.equal(review.rows[1].cells.get("name")?.before, "Bat");
  assert.equal(review.rows[1].cells.get("name")?.tone, "deleted");
  assert.equal(review.rows[0].cells.get("hp")?.tone, null);
  assert.equal(review.rows[0].changed, true);
});

test("distinguishes empty strings and absent values while preserving raw composite keys", () => {
  const before = master(["id", "wave", "hp"], [["a,b", "c", "100"], ["a", "b,c", "100"]]);
  const after = master(["id", "wave", "hp"], [["a,b", "c", ""], ["a", "b,c", "100"], ["01", "2", ""]]);
  const review = buildReviewTable(data(before, after, [
    { kind: "cell", masterId: "enemy", primaryKey: ["a,b", "c"], column: "hp", before: "100", after: "" },
    { kind: "addedRow", masterId: "enemy", primaryKey: ["01", "2"] },
  ], ["id", "wave"]), "enemy")!;
  assert.equal(review.rows[0].cells.get("hp")?.after, "");
  assert.equal(review.rows[0].cells.get("hp")?.tone, "modified");
  assert.equal(review.rows[1].changed, false);
  assert.equal(review.rows[2].cells.get("hp")?.before, null);
  assert.equal(review.rows[2].cells.get("hp")?.after, "");
  assert.equal(review.rows[2].cells.get("hp")?.tone, "added");
  assert.deepEqual(review.rows.filter(r => r.changed).map(r => r.key), [["a,b", "c"], ["01", "2"]]);
});

test("added columns and deleted rows never invent values at their intersection", () => {
  const review = buildReviewTable(data(
    master(["id", "hp"], [["1", "100"], ["2", "200"]]),
    master(["id", "hp", "name"], [["1", "100", ""]]),
    [{ kind: "addedColumn", masterId: "enemy", column: "name" }, { kind: "deletedRow", masterId: "enemy", primaryKey: ["2"] }],
  ), "enemy")!;
  assert.equal(review.rows[0].cells.get("name")?.tone, "added");
  assert.equal(review.rows[1].cells.get("name")?.before, null);
  assert.equal(review.rows[1].cells.get("name")?.after, null);
  assert.equal(review.rows[1].cells.get("name")?.tone, null);
});

test("comment-only changes remain discoverable without falsely changing cell values", () => {
  const m = master(["id", "hp"], [["1", "100"], ["2", "200"]]);
  const changes: SemanticChange[] = [
    { kind: "comment", masterId: "enemy", target: { kind: "cell", primaryKey: ["1"], column: "hp" }, before: null, after: "Check balance" },
    { kind: "comment", masterId: "enemy", target: { kind: "table" }, before: "old", after: "new" },
  ];
  const review = buildReviewTable(data(m, m, changes), "enemy")!;
  assert.equal(review.rows[0].changed, true);
  assert.equal(review.rows[1].changed, false);
  assert.equal(review.rows[0].cells.get("hp")?.before, "100");
  assert.equal(review.rows[0].cells.get("hp")?.after, "100");
  assert.deepEqual(review.rows[0].cells.get("hp")?.changes, [changes[0]]);
  assert.deepEqual(review.tableChanges, [changes[1]]);
});

test("whole-master additions and deletions color the grid and remain revertible as one change", () => {
  const m = master(["id", "hp"], [["1", "100"]]);
  for (const kind of ["addedMaster", "deletedMaster"] as const) {
    const change: SemanticChange = { kind, masterId: "enemy" };
    const review = buildReviewTable(data(kind === "addedMaster" ? null : m, kind === "addedMaster" ? m : null, [change]), "enemy")!;
    assert.equal(review.rows[0].cells.get("hp")?.tone, kind === "addedMaster" ? "added" : "deleted");
    assert.deepEqual(review.tableChanges, [change]);
    assert.deepEqual(review.rows[0].cells.get("hp")?.changes, [change]);
  }
});

test("empty masters expose column changes and changed definitions without fabricating row matches", () => {
  const review = buildReviewTable(data(master(["id", "hp"], []), master(["id"], []), [
    { kind: "deletedColumn", masterId: "enemy", column: "hp" },
  ]), "enemy")!;
  assert.equal(review.rows.length, 0);
  assert.equal(review.columns[1].tone, "deleted");
  assert.equal(review.columns[1].changes[0].kind, "deletedColumn");
  const fixture = data(master(["id", "name"], [["1", "A"]]), master(["id", "name"], [["1", "A"]]), [
    { kind: "masterDefinition", masterId: "enemy", before: { path: "enemy.csv", primaryKey: ["id"] }, after: { path: "enemy.csv", primaryKey: ["name"] } },
  ]);
  fixture.after.config.masters.enemy.primaryKey = ["name"];
  const rekeyed = buildReviewTable(fixture, "enemy")!;
  assert.equal(rekeyed.rows.length, 1);
  assert.deepEqual(rekeyed.rows[0].key, ["A"]);
  assert.equal(rekeyed.rows[0].cells.get("id")?.tone, "modified");
  assert.equal(rekeyed.rows[0].cells.get("id")?.changes[0].kind, "masterDefinition");
});
