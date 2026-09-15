export type PrimaryKey = string[];
export type Identity = { name: string; email: string };
export type Comment = {
  body: string;
  createdBy: Identity;
  createdAt: string;
  updatedBy: Identity;
  updatedAt: string;
};
export type Comments = {
  version: number;
  table: Comment | null;
  rows: { primaryKey: PrimaryKey; comment: Comment }[];
  cells: { primaryKey: PrimaryKey; column: string; comment: Comment }[];
};
export type Master = {
  table: { columns: string[]; rows: string[][] };
  comments: Comments;
};
export type Definition = { path: string; primaryKey: string[] };
export type Snapshot = {
  root: string;
  name: string;
  identity: Identity;
  revision: number;
  canUndo: boolean;
  canRedo: boolean;
  git: GitStatus;
  changes: SemanticChange[];
  changesError: string | null;
  merge: MergeView | null;
  data: {
    config: {
      version: number;
      git: { protectedBranches: string[] };
      masters: Record<string, Definition>;
    };
    masters: Record<string, { data: Master | null; error: string | null }>;
  };
};
export type GitStatus = { branch: string; upstream: string | null; ahead: number; behind: number; protected: boolean; trackedDirty: boolean; mergeInProgress: boolean; remotes: string[]; branches: string[] };
export type SemanticChange =
  | { kind: "projectConfig"; masterId: string; before: { protectedBranches: string[] } | null; after: { protectedBranches: string[] } }
  | { kind: "masterDefinition"; masterId: string; before: Definition; after: Definition }
  | { kind: "addedMaster" | "deletedMaster"; masterId: string }
  | { kind: "addedColumn" | "deletedColumn"; masterId: string; column: string }
  | { kind: "addedRow" | "deletedRow"; masterId: string; primaryKey: PrimaryKey }
  | { kind: "cell"; masterId: string; primaryKey: PrimaryKey; column: string; before: string; after: string }
  | { kind: "comment"; masterId: string; target: CommentTarget; before: string | null; after: string | null };
export type CommentTarget =
  | { kind: "table" }
  | { kind: "row"; primaryKey: PrimaryKey }
  | { kind: "cell"; primaryKey: PrimaryKey; column: string };
export type CellEdit = {
  primaryKey: PrimaryKey;
  column: string;
  value: string;
};
export type Operation =
  | { type: "setProtectedBranches"; patterns: string[] }
  | { type: "editCells"; masterId: string; edits: CellEdit[] }
  | {
      type: "addRow";
      masterId: string;
      primaryKey: PrimaryKey;
      duplicateFrom?: PrimaryKey;
    }
  | { type: "deleteRows"; masterId: string; primaryKeys: PrimaryKey[] }
  | { type: "addColumn" | "deleteColumn"; masterId: string; name: string }
  | {
      type: "setComment";
      masterId: string;
      target: CommentTarget;
      body: string;
    }
  | {
      type: "createMaster";
      masterId: string;
      path: string;
      primaryKey: string[];
      columns: string[];
    }
  | {
      type: "configureMaster";
      masterId: string;
      path: string;
      primaryKey: string[];
    };

export const keyId = (key: PrimaryKey) => JSON.stringify(key);
export const rowKey = (row: string[], master: Master, def: Definition) =>
  def.primaryKey.map((c) => row[master.table.columns.indexOf(c)]);

export type Resolution = { kind: "comment"; value: Comment | null } | { kind: "ours" | "theirs" } | { kind: "custom"; value: string };
export type Conflict = {
  id: string;
  kind: "projectConfig" | "master" | "column" | "row" | "cell" | "comment";
  masterId: string;
  primaryKey: PrimaryKey | null;
  column: string | null;
  base: unknown;
  ours: unknown;
  theirs: unknown;
  resolution: Resolution | null;
};
export type MergeView = {
  automaticallyMerged: number;
  conflicts: Conflict[];
  remaining: number;
  error: string | null;
};

export type ChangeReviewData = { before: Snapshot["data"] | null; after: Snapshot["data"]; changes: SemanticChange[] };

export type HistoryCommit = { oid: string; parents: string[]; author: Identity; authoredAt: string; subject: string };
export type HistoryPage = { head: string | null; commits: HistoryCommit[]; hasMore: boolean };
export type HistoryDetail = { oid: string; parent: string | null; message: string; changes: SemanticChange[] };
