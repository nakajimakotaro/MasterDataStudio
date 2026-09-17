import type { CalculatedCell, PreparedEdit, Operation } from "./types";

/** Runs only the targets approved by Rust, against snapshots of ordinary cells. */
export function evaluateScripts(prepared: PreparedEdit, safeMode = false, operation?: Operation): CalculatedCell[] {
  // This guard precedes even compilation: recovery must work with infinite loops
  // and syntactically invalid JavaScript in metadata.
  if (safeMode) return [];
  const compiled = new Map<string, (row: Record<string, string>) => unknown>();
  // A definition must parse even when the master contains no rows (or all
  // cells are overridden). This is deliberately bypassed in Safe Mode.
  if (operation?.type === "setScript" && operation.script !== null) {
    try { compiled.set(JSON.stringify([operation.masterId, operation.column]), new Function("row", operation.script) as (row: Record<string, string>) => unknown); }
    catch (error) { throw new Error(`Script Error\nMaster: ${operation.masterId}\nColumn: ${operation.column}\nPrimary Key: [] (Script definition)\n\n${String(error)}`); }
  }
  const rows = new Map<string, Map<string, string[]>>();
  return prepared.targets.map(({ masterId, primaryKey, column }) => {
    try {
      const master = prepared.data.masters[masterId]?.data;
      const def = prepared.data.config.masters[masterId];
      if (!master || !def) throw new Error("Master がありません。");
      if (master.scriptError) throw new Error(master.scriptError);
      const scripts = master.scripts?.columns ?? [];
      const entry = scripts.find(s => s.column === column);
      if (!entry) throw new Error("Script がありません。");
      let byKey = rows.get(masterId);
      if (!byKey) {
        const indices = def.primaryKey.map(c => master.table.columns.indexOf(c));
        byKey = new Map(master.table.rows.map(row => [JSON.stringify(indices.map(i => row[i])), row]));
        rows.set(masterId, byKey);
      }
      const values = byKey.get(JSON.stringify(primaryKey));
      if (!values) throw new Error("Row がありません。");
      const scriptColumns = new Set(scripts.map(s => s.column));
      // Null prototype preserves column names such as '__proto__' and prevents
      // prototype properties from appearing as additional row inputs.
      const row: Record<string, string> = Object.create(null);
      master.table.columns.forEach((c, i) => {
        if (!scriptColumns.has(c)) row[c] = values[i];
      });
      const id = JSON.stringify([masterId, column]);
      let fn = compiled.get(id);
      if (!fn) {
        fn = new Function("row", entry.script) as (row: Record<string, string>) => unknown;
        compiled.set(id, fn);
      }
      const result = fn(row);
      if (!["string", "number", "boolean"].includes(typeof result)) {
        throw new Error("戻り値は string / number / boolean にしてください。空文字は return \"\"; を使用します。");
      }
      return { masterId, primaryKey, column, value: String(result) };
    } catch (error) {
      throw new Error(`Script Error\nMaster: ${masterId}\nColumn: ${column}\nPrimary Key: ${JSON.stringify(primaryKey)}\n\n${String(error)}`);
    }
  });
}
