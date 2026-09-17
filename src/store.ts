import { create } from "zustand";
import type { PrimaryKey } from "./types";

export type Dialog =
  | "columnScript"
  | "createMaster"
  | "deleteRows"
  | "addColumn"
  | "deleteColumn"
  | "settings"
  | "masterSettings"
  | "fill"
  | "changes"
  | "commit"
  | "merge"
  | "branch"
  | "history"
  | null;
export type DraftRow = { key: PrimaryKey; cells: Record<string, string> };
type UIState = {
  scriptColumn: string | null;
  drafts: Record<string, DraftRow[]>;
  masterId: string | null;
  selectedKeys: PrimaryKey[];
  cell: { primaryKey: PrimaryKey; column: string } | null;
  search: string;
  inspector: boolean;
  dialog: Dialog;
  error: string | null;
  selectMaster: (id: string | null) => void;
  set: (state: Partial<UIState>) => void;
};

export const useUI = create<UIState>((set) => ({
  scriptColumn: null,
  drafts: {},
  masterId: null,
  selectedKeys: [],
  cell: null,
  search: "",
  inspector: true,
  dialog: null,
  error: null,
  selectMaster: (masterId) =>
    set({ masterId, selectedKeys: [], cell: null, search: "", dialog: null }),
  set: (state) => set(state),
}));
