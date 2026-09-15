import { keyId, rowKey, type ChangeReviewData, type PrimaryKey, type SemanticChange } from "./types.ts";

export type ReviewTone = "added" | "modified" | "deleted";
export type ReviewCell = {
  before: string | null;
  after: string | null;
  tone: ReviewTone | null;
  changes: SemanticChange[];
};
export type ReviewRow = {
  id: string;
  key: PrimaryKey;
  cells: Map<string, ReviewCell>;
  tones: ReviewTone[];
  changed: boolean;
};

// Keep the current order and place removed items beside their former neighbours.
function withDeleted(current: string[], previous: string[]): string[] {
  const present = new Set(current);
  const before = new Map<string, string[]>();
  let pending: string[] = [];
  for (const value of previous) {
    if (present.has(value)) {
      before.set(value, pending);
      pending = [];
    } else pending.push(value);
  }
  return [...current.flatMap(value => [...(before.get(value) ?? []), value]), ...pending];
}

export function changeTone(change: SemanticChange): ReviewTone {
  if (change.kind.startsWith("added")) return "added";
  if (change.kind.startsWith("deleted")) return "deleted";
  if (change.kind === "comment") return change.before === null ? "added" : change.after === null ? "deleted" : "modified";
  return "modified";
}

export function buildReviewTable(data: ChangeReviewData, masterId: string) {
  const changes = data.changes.filter(change => change.masterId === masterId);
  const previous = data.before?.masters[masterId]?.data;
  const current = data.after.masters[masterId]?.data;
  const oldDef = data.before?.config.masters[masterId];
  const def = data.after.config.masters[masterId] ?? oldDef;
  if (!def || (!current && !previous)) return null;
  const rekeyed = !!oldDef && keyId(oldDef.primaryKey) !== keyId(def.primaryKey);
  const oldRows = new Map(previous && oldDef && !rekeyed
    ? previous.table.rows.map(values => [keyId(rowKey(values, previous, oldDef)), values]) : []);
  const newRows = new Map(current
    ? current.table.rows.map(values => [keyId(rowKey(values, current, def)), values]) : []);
  const oldColumns = new Map(previous?.table.columns.map((name, i) => [name, i]));
  const newColumns = new Map(current?.table.columns.map((name, i) => [name, i]));
  const tableChanges: SemanticChange[] = [];
  const byRow = new Map<string, SemanticChange[]>();
  const byColumn = new Map<string, SemanticChange[]>();
  const byCell = new Map<string, SemanticChange[]>();
  const push = (map: Map<string, SemanticChange[]>, key: string, change: SemanticChange) => map.set(key, [...(map.get(key) ?? []), change]);
  for (const change of changes) {
    if (change.kind === "cell") push(byCell, JSON.stringify([change.primaryKey, change.column]), change);
    else if (change.kind === "addedRow" || change.kind === "deletedRow") push(byRow, keyId(change.primaryKey), change);
    else if (change.kind === "addedColumn" || change.kind === "deletedColumn") push(byColumn, change.column, change);
    else if (change.kind === "comment" && change.target.kind !== "table") {
      if (change.target.kind === "cell") push(byCell, JSON.stringify([change.target.primaryKey, change.target.column]), change);
      else push(byRow, keyId(change.target.primaryKey), change);
    } else tableChanges.push(change);
  }
  const structural = tableChanges.filter(c => c.kind === "addedMaster" || c.kind === "deletedMaster" || (rekeyed && c.kind === "masterDefinition"));
  const names = withDeleted(current?.table.columns ?? [], rekeyed ? [] : previous?.table.columns ?? []);
  const columns = [...def.primaryKey, ...names.filter(name => !def.primaryKey.includes(name))].map(name => {
    const related = [...structural, ...(byColumn.get(name) ?? [])];
    return { name, changes: related, tone: related.length ? changeTone(related[0]) : null };
  });
  const rows: ReviewRow[] = withDeleted([...newRows.keys()], [...oldRows.keys()]).map(id => {
    const key = JSON.parse(id) as PrimaryKey;
    const oldRow = oldRows.get(id);
    const newRow = newRows.get(id);
    const rowChanges = byRow.get(id) ?? [];
    const cells = new Map<string, ReviewCell>();
    const tones = new Set<ReviewTone>();
    for (const column of columns) {
      const oldIndex = oldColumns.get(column.name);
      const newIndex = newColumns.get(column.name);
      const before = oldRow && oldIndex !== undefined ? oldRow[oldIndex] : null;
      const after = newRow && newIndex !== undefined ? newRow[newIndex] : null;
      const related = [...structural, ...rowChanges, ...(byColumn.get(column.name) ?? []), ...(byCell.get(JSON.stringify([key, column.name])) ?? [])];
      let tone: ReviewTone | null = null;
      if (related.length && (before !== null || after !== null)) {
        tone = rekeyed ? "modified" : before === null ? "added" : after === null ? "deleted" : before !== after ? "modified" : null;
        if (!tone && related.some(c => c.kind === "comment")) tone = "modified";
      }
      if (tone) tones.add(tone);
      cells.set(column.name, { before, after, tone, changes: related });
    }
    return { id, key, cells, tones: [...tones], changed: tones.size > 0 || rowChanges.length > 0 };
  });
  return { def, columns, rows, tableChanges, changes };
}
