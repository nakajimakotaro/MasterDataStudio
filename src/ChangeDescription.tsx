import type { SemanticChange } from "./types";

export function describeChange(change: SemanticChange): string {
  switch (change.kind) {
    case "cell":
      return `${JSON.stringify(change.primaryKey)} / ${change.column}: “${change.before}” → “${change.after}”`;
    case "comment": {
      const target = change.target;
      const location = target.kind === "table" ? "Table" : `${JSON.stringify(target.primaryKey)}${target.kind === "cell" ? ` / ${target.column}` : ""}`;
      return `${location} Comment\n${change.before === null ? "（なし）" : change.before}\n↓\n${change.after === null ? "（削除）" : change.after}`;
    }
    case "projectConfig":
      return `Protected Branches: ${change.before?.protectedBranches.join(", ") ?? "（未設定）"} → ${change.after.protectedBranches.join(", ") || "（なし）"}`;
    case "masterDefinition":
      return `Master 定義: ${change.before.path} / ${JSON.stringify(change.before.primaryKey)} → ${change.after.path} / ${JSON.stringify(change.after.primaryKey)}`;
    case "addedRow": case "deletedRow":
      return `${change.kind === "addedRow" ? "+ Added" : "− Deleted"} Row ${JSON.stringify(change.primaryKey)}`;
    case "addedColumn": case "deletedColumn":
      return `${change.kind === "addedColumn" ? "+ Added" : "− Deleted"} Column ${change.column}`;
    default:
      return change.kind === "addedMaster" ? "+ Added Master" : "− Deleted Master";
  }
}
