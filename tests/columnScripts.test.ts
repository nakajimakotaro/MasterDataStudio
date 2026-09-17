import { test } from "node:test";
import assert from "node:assert/strict";
import { evaluateScripts } from "../src/columnScripts.ts";
import type { PreparedEdit } from "../src/types.ts";

function prepared(script = "return Number(row.attack) * 2;"): PreparedEdit {
  return {
    data: {
      config: { version: 1, git: { protectedBranches: [] }, masters: { enemy: { path: "enemy.csv", primaryKey: ["id", "wave"] } } },
      masters: { enemy: { error: null, data: {
        table: { columns: ["id", "wave", "attack", "power", "score", "__proto__"], rows: [["1", "a", "20", "999", "888", "literal"]] },
        comments: { version: 1, table: null, rows: [], cells: [] },
        scripts: { version: 1, columns: [
          { column: "power", script, overrides: [] },
          { column: "score", script: 'return Number(row.attack) + 1;', overrides: [] },
        ] },
      } } },
    },
    targets: [
      { masterId: "enemy", primaryKey: ["1", "a"], column: "power" },
      { masterId: "enemy", primaryKey: ["1", "a"], column: "score" },
    ],
  };
}

test("scripts see only same-row ordinary strings, including arbitrary column names", () => {
  const p = prepared(`
    if (typeof row.attack !== "string" || row.__proto__ !== "literal") throw Error("input");
    if ("power" in row || "score" in row || "toString" in row) throw Error("leaked column");
    return Number(row.attack) * 2;
  `);
  const before = structuredClone(p);
  assert.deepEqual(evaluateScripts(p).map(c => c.value), ["40", "21"]);
  assert.deepEqual(p, before);
});

test("each script gets an independent row snapshot", () => {
  const p = prepared('row.attack = "500"; return 7;');
  assert.deepEqual(evaluateScripts(p).map(c => c.value), ["7", "21"]);
});

test("only primitive strings, numbers and booleans are accepted", () => {
  for (const [expression, value] of [['""', ""], ['"001"', "001"], ["123", "123"], ["false", "false"], ["NaN", "NaN"], ["Infinity", "Infinity"]]) {
    assert.equal(evaluateScripts(prepared(`return ${expression};`))[0].value, value);
  }
  for (const expression of ["undefined", "null", "{}", "[]", "(() => 1)", "Symbol()", "1n", "Promise.resolve(1)"]) {
    assert.throws(() => evaluateScripts(prepared(`return ${expression};`)), /戻り値は/);
  }
});

test("syntax, execution and validation errors include master, column and tuple", () => {
  for (const script of ["return (", 'throw new Error("bad row");', "return null;"]) {
    assert.throws(() => evaluateScripts(prepared(script)), error => {
      assert.match(String(error), /Master: enemy/);
      assert.match(String(error), /Column: power/);
      assert.match(String(error), /Primary Key: \["1","a"\]/);
      return true;
    });
  }
});

test("later row errors return no partial patch and do not mutate input", () => {
  const p = prepared('if (row.id === "2") throw Error("second row"); return 42;');
  p.data.masters.enemy.data!.table.rows.push(["2", "b", "30", "old", "old", ""]);
  p.targets.push({ masterId: "enemy", primaryKey: ["2", "b"], column: "power" });
  const before = structuredClone(p);
  assert.throws(() => evaluateScripts(p), /second row/);
  assert.deepEqual(p, before);
});

test("Safe Mode never compiles or executes scripts, including definition edits", () => {
  for (const script of ["while (true) {}", "return (", 'throw Error("executed");']) {
    assert.deepEqual(evaluateScripts(prepared(script), true, { type: "setScript", masterId: "enemy", column: "power", script }), []);
  }
});

test("syntax errors are rejected even with no calculation targets", () => {
  const p = prepared(); p.targets = [];
  assert.throws(() => evaluateScripts(p, false, { type: "setScript", masterId: "enemy", column: "power", script: "return (" }), /Script definition/);
});

test("row lookups preserve composite identities and unrequested overrides", () => {
  const p = prepared('return row.wave;');
  p.data.masters.enemy.data!.table.rows = [
    ["a,b", "c", "1", "manual", "0", ""],
    ["a", "b,c", "2", "0", "0", ""],
  ];
  p.data.masters.enemy.data!.scripts!.columns[0].overrides = [["a,b", "c"]];
  p.targets = [{ masterId: "enemy", primaryKey: ["a", "b,c"], column: "power" }];
  assert.equal(evaluateScripts(p)[0].value, "b,c");
  assert.equal(p.data.masters.enemy.data!.table.rows[0][3], "manual");
});
