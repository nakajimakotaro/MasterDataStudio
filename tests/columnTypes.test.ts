import { test } from "node:test";
import assert from "node:assert/strict";
import { columnType, columnTypeError, scriptValue, columnTypeLabels } from "../src/columnTypes.ts";
import type { ColumnType } from "../src/types.ts";

test("types preserve strings and allow empty cells independently of conversion", () => {
  for (const type of Object.keys(columnTypeLabels) as ColumnType[]) {
    assert.equal(columnTypeError("", type), null);
  }
  assert.equal(scriptValue("", "integer"), 0);
  assert.equal(scriptValue("001", "string"), "001");
  assert.equal(scriptValue("2026-09-28", "date"), "2026-09-28");
  assert.equal(scriptValue("09:30:00", "time"), "09:30:00");
  assert.equal(scriptValue("2026-09-28T09:30:00", "datetime"), "2026-09-28T09:30:00");
  assert.equal(scriptValue("0", "boolean"), false);
  assert.equal(scriptValue("1", "boolean"), true);
  assert.equal(scriptValue("wrong", "boolean"), "wrong");
  assert.equal(scriptValue("", "boolean"), "");
});

test("integer validation checks notation without imposing Script precision or range limits", () => {
  for (const value of ["0", "001", "-23", "+5", "9007199254740991", "-9007199254740991", "9007199254740992", "9007199254740993", (2n ** 100n).toString(), (2n ** 100n + 1n).toString(), (-(2n ** 100n)).toString(), "9".repeat(400)]) {
    assert.equal(columnTypeError(value, "integer"), null, value);
  }
  for (const value of ["1.5", "0x10", "1e2", "NaN", "Infinity", "abc", " 1", " "]) {
    assert.ok(columnTypeError(value, "integer"), value);
  }
  assert.equal(scriptValue("1.5", "integer"), 1.5);
  assert.equal(scriptValue("0x10", "integer"), 16);
  assert.ok(Number.isNaN(scriptValue("abc", "integer")));
});

test("decimal validation accepts large numbers and exponents beyond the Script range", () => {
  for (const value of ["1", "1.5", "-.25", "+2.", "1e-7", "2E+20", "1e999", "-1e999", "1e-999", "1267650600228229401496703205376.125", `${"9".repeat(400)}.125`]) assert.equal(columnTypeError(value, "decimal"), null, value);
  for (const value of ["NaN", "Infinity", "-Infinity", "1e", "1,234", "0x10", " 2 "]) assert.ok(columnTypeError(value, "decimal"), value);
});

test("date validation checks calendar dates including century leap years without timezone conversion", () => {
  for (const value of ["0001-01-01", "2000-02-29", "2024-02-29", "2026-09-28", "9999-12-31"]) assert.equal(columnTypeError(value, "date"), null, value);
  for (const value of ["0000-01-01", "1900-02-29", "2026-02-29", "2026-04-31", "2026-00-01", "2026-13-01", "2026-01-00", "2026-1-1", "2026-01-01T00:00:00Z"]) assert.ok(columnTypeError(value, "date"), value);
});

test("boolean validation does not use JavaScript truthiness and arbitrary column names are safe", () => {
  for (const value of ["1", "0"]) assert.equal(columnTypeError(value, "boolean"), null);
  for (const value of ["true", "false", "True", "2", "-1", "01", "1.0", " 1", "0 "]) {
    assert.ok(columnTypeError(value, "boolean"));
    assert.equal(scriptValue(value, "boolean"), value);
  }
  const def = { primaryKey: ["id"], columnTypes: JSON.parse('{"__proto__":"integer"}') };
  assert.equal(columnType(def, "__proto__"), "integer");
  assert.equal(columnType(def, "constructor"), "integer");
  assert.equal(columnType({ primaryKey: ["id"] }, "hp"), "integer");
  assert.equal(columnType({ primaryKey: ["id"], columnTypes: { name: "string" } }, "name"), "string");
});

test("time validation requires seconds and a valid 24-hour clock with an optional timezone", () => {
  for (const value of ["00:00:00", "09:30:00", "23:59:59", "09:30:00Z", "09:30:00+09:00", "09:30:00-05:30", "09:30:00+23:59"]) assert.equal(columnTypeError(value, "time"), null, value);
  for (const value of ["24:00:00", "12:60:00", "12:00:60", "9:30:00", "09:30", "09:30:00.123", "09:30:00z", "09:30:00+0900", "09:30:00+09", "09:30:00+24:00", "09:30:00+09:60", "09:30:00 +09:00", "-1:00:00", " 09:30:00", "2026-09-28T09:30:00"]) assert.ok(columnTypeError(value, "time"), value);
});

test("datetime validation checks both calendar and time without parsing or normalizing strings", () => {
  for (const value of ["0001-01-01T00:00:00", "2000-02-29T12:30:45", "2026-09-28T09:30:00", "9999-12-31T23:59:59", "2026-09-28T09:30:00Z", "2026-09-28T09:30:00+09:00", "2026-09-28T09:30:00-05:30"]) assert.equal(columnTypeError(value, "datetime"), null, value);
  for (const value of ["0000-01-01T00:00:00", "1900-02-29T12:30:45", "2026-04-31T12:30:45", "2026-09-28T24:00:00", "2026-09-28T12:60:00", "2026-09-28T12:00:60", "2026-09-28", "09:30:00", "2026-09-28 09:30:00", "2026-09-28T09:30", "2026-09-28T09:30:00.123", "2026-09-28T09:30:00.123Z", "2026-09-28T09:30:00z", "2026-09-28T09:30:00+0900", "2026-09-28T09:30:00+24:00", "2026-09-28T09:30:00\n"]) assert.ok(columnTypeError(value, "datetime"), value);
  assert.equal(scriptValue("invalid datetime", "datetime"), "invalid datetime");
  assert.equal(scriptValue("2026-09-28T09:30:00+09:00", "datetime"), "2026-09-28T09:30:00+09:00");
});
