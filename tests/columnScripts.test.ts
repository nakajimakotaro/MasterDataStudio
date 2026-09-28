import { test } from "node:test";
import assert from "node:assert/strict";
import { evaluateScripts, needsScriptPreview } from "../src/columnScripts.ts";
import { columnTypeError } from "../src/columnTypes.ts";
import type { ColumnType, Operation, PreparedEdit, Snapshot } from "../src/types.ts";

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

test("only known script-free cell edits and Safe Mode bypass preview", () => {
  const snapshot = { data: prepared().data } as Snapshot;
  const operation: Operation = { type: "editCells", masterId: "enemy", edits: [{ primaryKey: ["1", "a"], column: "attack", value: "30" }] };
  assert.equal(needsScriptPreview(snapshot, operation), true);
  snapshot.data.masters.enemy.data!.scripts!.columns = [];
  assert.equal(needsScriptPreview(snapshot, operation), false);
  snapshot.data.masters.enemy.data!.scriptError = "invalid metadata";
  assert.equal(needsScriptPreview(snapshot, operation), true);
  delete snapshot.data.masters.enemy;
  assert.equal(needsScriptPreview(snapshot, operation), true);
  assert.equal(needsScriptPreview(snapshot, { type: "setScript", masterId: "enemy", column: "power", script: "return (" }), true);
  assert.equal(needsScriptPreview({ ...snapshot, safeMode: true }, operation), false);
});

test("scripts see only same-row ordinary strings, including arbitrary column names", () => {
  const p = prepared(`
    if (typeof row.attack !== "string" || row.__proto__ !== "literal") throw Error("input");
    if ("power" in row || "score" in row || "toString" in row) throw Error("leaked column");
    return Number(row.attack) * 2;
  `);
  p.data.config.masters.enemy.columnTypes = { attack: "string", ["__proto__"]: "string" };
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
  const p = prepared('if (row.id === 2) throw Error("second row"); return 42;');
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
  p.data.config.masters.enemy.columnTypes = { wave: "string" };
  p.data.masters.enemy.data!.table.rows = [
    ["a,b", "c", "1", "manual", "0", ""],
    ["a", "b,c", "2", "0", "0", ""],
  ];
  p.data.masters.enemy.data!.scripts!.columns[0].overrides = [["a,b", "c"]];
  p.targets = [{ masterId: "enemy", primaryKey: ["a", "b,c"], column: "power" }];
  assert.equal(evaluateScripts(p)[0].value, "b,c");
  assert.equal(p.data.masters.enemy.data!.table.rows[0][3], "manual");
});

test("typed Script inputs add numerically and leave stored values unchanged", () => {
  const p = prepared('return row.attack + row.__proto__;');
  p.data.config.masters.enemy.columnTypes = { attack: "integer", ["__proto__"]: "decimal", wave: "boolean" };
  p.data.masters.enemy.data!.table.rows[0][5] = "2.5";
  p.data.masters.enemy.data!.scripts!.columns[1].script = 'return row.wave;';
  // Keep the primary key as its original string even when the column has a type.
  p.data.masters.enemy.data!.table.rows[0][1] = "0";
  p.targets.forEach(t => { t.primaryKey[1] = "0"; });
  const before = structuredClone(p);
  assert.deepEqual(evaluateScripts(p).map(c => c.value), ["22.5", "false"]);
  assert.deepEqual(p, before);
});

test("columns without type definitions are numeric Script inputs", () => {
  const p = prepared('return row.attack + 2;');
  const before = structuredClone(p);
  assert.equal(evaluateScripts(p)[0].value, "22");
  assert.deepEqual(p, before);
});

test("invalid numeric inputs do not stop calculation and scripts may handle NaN", () => {
  const p = prepared('return row.attack + 2;');
  p.data.config.masters.enemy.columnTypes = { attack: "integer" };
  p.data.masters.enemy.data!.table.rows[0][2] = "abc";
  p.data.masters.enemy.data!.scripts!.columns[1].script = 'return Number.isNaN(row.attack) ? 7 : row.attack;';
  assert.deepEqual(evaluateScripts(p).map(c => c.value), ["NaN", "7"]);
  p.data.masters.enemy.data!.table.rows[0][2] = "";
  assert.deepEqual(evaluateScripts(p).map(c => c.value), ["2", "0"]);
});

test("large numeric Script inputs may round or overflow without changing the stored input", () => {
  const twoTo100 = 2n ** 100n;
  for (const type of ["integer", "decimal"] as const) {
    for (const [input, expected] of [
      [twoTo100.toString(), "1.2676506002282294e+30"],
      [(twoTo100 + 1n).toString(), "1.2676506002282294e+30"],
      [(-twoTo100).toString(), "-1.2676506002282294e+30"],
      ["9007199254740993", "9007199254740992"],
      ["9".repeat(400), "Infinity"],
    ]) {
      const p = prepared('return row.attack;');
      p.data.config.masters.enemy.columnTypes = { attack: type };
      p.data.masters.enemy.data!.table.rows[0][2] = input;
      p.data.masters.enemy.data!.scripts!.columns[1].script = 'return row.attack + 1;';
      const before = structuredClone(p);
      assert.deepEqual(evaluateScripts(p).map(c => c.value), [expected, expected]);
      assert.deepEqual(p, before);
    }
  }
});

test("string inputs allow exact large-integer calculation with BigInt and string output", () => {
  const p = prepared('return (BigInt(row.attack) + 1n).toString();');
  p.data.config.masters.enemy.columnTypes = { attack: "string" };
  p.data.masters.enemy.data!.table.rows[0][2] = (2n ** 100n).toString();
  const before = structuredClone(p);
  assert.equal(evaluateScripts(p)[0].value, "1267650600228229401496703205377");
  assert.deepEqual(p, before);
});

test("boolean columns convert 1/0 to booleans and save boolean Script results as 1/0", () => {
  for (const [input, expected] of [["1", "0"], ["0", "1"]]) {
    const p = prepared('if (typeof row.attack !== "boolean") throw Error("input"); return !row.attack;');
    p.data.config.masters.enemy.columnTypes = { attack: "boolean", power: "boolean" };
    p.data.masters.enemy.data!.table.rows[0][2] = input;
    const before = structuredClone(p);
    const result = evaluateScripts(p)[0].value;
    assert.equal(result, expected);
    assert.equal(columnTypeError(result, "boolean"), null);
    assert.deepEqual(p, before);
  }
});

test("boolean output leaves explicit strings and invalid results intact for advisory validation", () => {
  for (const [expression, expected] of [['"true"', "true"], ['"false"', "false"], ['""', ""], ["1", "1"], ["0", "0"], ["2", "2"], ["NaN", "NaN"]]) {
    const p = prepared(`return ${expression};`);
    p.data.config.masters.enemy.columnTypes = { power: "boolean" };
    assert.equal(evaluateScripts(p)[0].value, expected);
    assert.equal(!!columnTypeError(expected, "boolean"), !["", "1", "0"].includes(expected));
  }
});

test("date, time and datetime columns reach scripts as their original strings, including invalid values", () => {
  const inputs: [ColumnType, string][] = [
    ["date", "2026-09-28"], ["time", "09:30:00"], ["datetime", "2026-09-28T09:30:00"],
    ["time", "25:00:00"], ["datetime", "invalid"], ["datetime", ""],
  ];
  for (const [type, value] of inputs) {
    const p = prepared('return typeof row.attack + ":" + row.attack;');
    p.data.config.masters.enemy.columnTypes = { attack: type };
    p.data.masters.enemy.data!.table.rows[0][2] = value;
    const before = structuredClone(p);
    assert.equal(evaluateScripts(p)[0].value, `string:${value}`);
    assert.deepEqual(p, before);
  }
});
