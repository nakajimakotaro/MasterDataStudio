import type { Conflict } from "./types.ts";
export type ConflictFilter = { master: string; kind: string; column: string; status: "unresolved" | "resolved" | "all"; query: string };
export const emptyConflictFilter: ConflictFilter = { master: "", kind: "", column: "", status: "unresolved", query: "" };
export function indexConflicts(conflicts: Conflict[]) {
  return conflicts.map(conflict => ({ conflict, search: [conflict.masterId, conflict.kind, JSON.stringify(conflict.primaryKey), conflict.column,
    ...[conflict.base, conflict.ours, conflict.theirs].map(value => typeof value === "string" ? value : JSON.stringify(value)),
  ].join("\n").toLocaleLowerCase() }));
}
export function filterConflicts(index: ReturnType<typeof indexConflicts>, filter: ConflictFilter) {
  const query = filter.query.trim().toLocaleLowerCase();
  return index.filter(({ conflict: c, search }) => (!filter.master || c.masterId === filter.master)
    && (!filter.kind || c.kind === filter.kind) && (!filter.column || c.column === filter.column)
    && (filter.status === "all" || (filter.status === "resolved" ? !!c.resolution : !c.resolution))
    && (!query || search.includes(query))).map(({ conflict }) => conflict);
}
export function unresolvedIds(conflicts: Conflict[]): Set<string> {
  return new Set(conflicts.filter(c => !c.resolution).map(c => c.id));
}
