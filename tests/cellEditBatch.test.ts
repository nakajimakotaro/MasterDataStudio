import { test } from "node:test";
import assert from "node:assert/strict";
import { CellEditBatch } from "../src/cellEditBatch.ts";
import type { CellEdit } from "../src/types.ts";

const cell = (key: string[], column: string, value: string): CellEdit => ({
  primaryKey: key, column, value,
});

test("ordinary cell edits are immediately available for saving", () => {
  const batch = new CellEditBatch();
  const edit = cell(["01"], "hp", "100");
  assert.deepEqual(batch.request(edit), [edit]);
  assert.deepEqual(batch.finish(), []);
});

test("a multi-column fill is saved as one operation, preserving keys and string values", () => {
  const batch = new CellEditBatch();
  const edits = [
    cell(["01", "a,b"], "hp", "100"),
    cell(["02", "a,b"], "hp", "200"),
    cell(["01", "a,b"], "name", "001"),
    cell(["draft", "uuid"], "name", ""),
  ];
  batch.start();
  for (const edit of edits) assert.deepEqual(batch.request(edit), []);
  assert.deepEqual(batch.finish(), edits);
  assert.deepEqual(batch.finish(), []);
});

test("empty and consecutive fills do not leak edits into later operations", () => {
  const batch = new CellEditBatch();
  batch.start();
  assert.deepEqual(batch.finish(), []);
  const first = cell(["1"], "hp", "10");
  batch.start();
  batch.request(first);
  assert.deepEqual(batch.finish(), [first]);
  const second = cell(["2"], "hp", "20");
  batch.start();
  batch.request(second);
  assert.deepEqual(batch.finish(), [second]);
  assert.deepEqual(batch.request(first), [first]);
});
