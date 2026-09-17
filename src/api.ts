import { evaluateScripts } from "./columnScripts";
import { invoke, isTauri } from "@tauri-apps/api/core";
import {
  useIsMutating,
  useMutation,
  useQuery,
  useQueryClient,
} from "@tanstack/react-query";
import { useUI } from "./store";
import type { ChangeFilter, ChangeReviewData, HistoryChangesPage, HistoryPage, Operation, Resolution, SemanticChange, Snapshot, PreparedEdit } from "./types";

export const desktop = isTauri();
type Request =
  | { command: "open_project"; path: string; initialize: boolean; safeMode?: boolean }
  | { command: "edit_project"; operation: Operation }
  | { command: "undo" | "redo" | "close_project" }
  | { command: "set_identity"; name: string; email: string }
  | { command: "git_fetch" | "git_update" | "git_push" }
  | { command: "switch_branch"; branch: string; create: boolean }
  | { command: "commit"; message: string; push: boolean }
  | { command: "revert_change"; change: SemanticChange }
  | { command: "merge_branch"; branch: string }
  | { command: "resolve_conflict"; id: string; resolution: Resolution }
  | { command: "resolve_conflicts"; ids: string[]; resolution: Resolution; revision: number }
  | { command: "complete_merge"; message: string }
  | { command: "abort_merge" };

export function useProject() {
  return useQuery<Snapshot | null>({
    queryKey: ["project"],
    queryFn: () => invoke<Snapshot>("get_project"),
    enabled: false,
    initialData: null,
  });
}

export function useRepositoryAction() {
  const client = useQueryClient();
  return useMutation({
    mutationKey: ["repository"],
    scope: { id: "repository" },
    mutationFn: async (request: Request) => {
      const { command, ...args } = request;
      const current = client.getQueryData<Snapshot | null>(["project"]);
      const revision = "revision" in request ? request.revision : current?.revision ?? 0;
      const edit = async (operation: Operation, snapshot: Snapshot) => {
        const prepared = await invoke<PreparedEdit>("preview_edit", { operation, revision: snapshot.revision });
        const calculated = evaluateScripts(prepared, snapshot.safeMode, operation);
        return invoke<Snapshot>("edit_project", { operation, calculated, revision: snapshot.revision });
      };
      let snapshot = command === "edit_project" && current
        ? await edit(request.operation, current)
        : command === "revert_change" && current
        ? await edit({ type: "revertChange", change: request.change }, current)
        : await invoke<Snapshot | null>(command, { ...args, revision });
      const errors: string[] = [];
      // Undo and redo deliberately never take this path.
      if (snapshot && !snapshot.safeMode && !snapshot.merge && [
        "open_project", "switch_branch", "git_update", "merge_branch", "complete_merge", "abort_merge",
      ].includes(command)) {
        for (const [masterId, entry] of Object.entries(snapshot.data.masters)) {
          if (entry.data?.scriptError) {
            errors.push(`Master: ${masterId}\nScript metadata: ${entry.data.scriptError}\nSafe Mode で修正してください。`);
          } else if (entry.data?.scripts?.columns.length) {
            try { snapshot = await edit({ type: "recalculateScripts", masterId }, snapshot); }
            catch (error) { errors.push(String(error)); }
          }
        }
      }
      return { snapshot, error: errors.length ? errors.join("\n\n") : null };
    },
    onSuccess: ({ snapshot, error }, request) => {
      client.setQueryData(["project"], snapshot);
      void client.invalidateQueries({ queryKey: ["changeReview"] });
      void client.invalidateQueries({ queryKey: ["history"] });
      if (snapshot?.merge || request.command === "abort_merge" || request.command === "complete_merge" || request.command === "switch_branch" || request.command === "git_update" || request.command === "merge_branch") {
        useUI.getState().selectMaster(null);
      }
      useUI.getState().set({ error });
      if (
        request.command === "open_project" ||
        request.command === "close_project"
      ) {
        useUI.getState().selectMaster(null);
        void client.invalidateQueries({ queryKey: ["recent"] });
      }
    },
    onError: async (error, request) => {
      useUI.getState().set({ error: String(error) });
      // Git can change state before reporting an error (e.g. commit succeeds,
      // push fails, or a merge commit hook rejects). Refresh the Rust snapshot.
      if (request.command !== "open_project" && request.command !== "close_project") {
        try { client.setQueryData(["project"], await invoke<Snapshot>("get_project")); } catch { /* Preserve the original error. */ }
      }
    },
  });
}

export function useBusy() {
  return useIsMutating({ mutationKey: ["repository"] }) > 0;
}
export function useRecent() {
  return useQuery({
    queryKey: ["recent"],
    queryFn: () => invoke<string[]>("recent_projects"),
    enabled: desktop,
  });
}

export function useHistory(project: Snapshot, cursor: string | null, query: string, author: string) {
  return useQuery({
    queryKey: ["history", project.root, project.git.branch, cursor, query, author],
    queryFn: () => invoke<HistoryPage>("project_history", { root: project.root, cursor, query, author }),
    gcTime: 0,
  });
}
export function useHistoryDetail(root: string, oid: string | null, filter: ChangeFilter) {
  return useQuery({
    queryKey: ["historyDetail", root, oid, filter],
    queryFn: () => invoke<HistoryChangesPage>("history_detail", { root, oid, filter }),
    enabled: !!oid,
    gcTime: 0,
    staleTime: Infinity,
  });
}

export function useChangeReview(project: Snapshot) {
  return useQuery({
    queryKey: ["changeReview", project.root, project.revision, project.git.branch],
    queryFn: () => invoke<ChangeReviewData>("change_review", { root: project.root, revision: project.revision }),
    placeholderData: (previous, query) => query?.queryKey[1] === project.root ? previous : undefined,
    gcTime: 0,
  });
}
