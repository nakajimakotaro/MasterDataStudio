import { evaluateScripts } from "./columnScripts";
import { applyProjectUpdate, applyRepositoryState } from "./projectUpdate";
import { invoke, isTauri } from "@tauri-apps/api/core";
import {
  useIsMutating,
  useMutation,
  useQuery,
  useQueryClient,
} from "@tanstack/react-query";
import { useUI } from "./store";
import type { ChangeFilter, ChangeReviewData, HistoryChangesPage, HistoryPage, Operation, Resolution, SemanticChange, Snapshot, PreparedEdit, ProjectUpdate, RepositoryState, ReviewSummary } from "./types";

export const desktop = isTauri();
type Request =
  | { command: "select_master"; masterId: string | null }
  | { command: "clone_project"; url: string; path: string; safeMode?: boolean }
  | { command: "open_project"; path: string; initialize: boolean; safeMode?: boolean }
  | { command: "edit_project"; operation: Operation }
  | { command: "close_project" }
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
    gcTime: 0,
    onMutate: async () => {
      // A read started before a Git operation must not overwrite its result,
      // even when that operation (for example Fetch) keeps the same revision.
      await Promise.all([
        client.cancelQueries({ queryKey: ["changeReview"] }),
        client.cancelQueries({ queryKey: ["repositoryState"] }),
        client.cancelQueries({ queryKey: ["reviewSummary"] }),
      ]);
    },
    mutationFn: async (request: Request) => {
      const { command, ...args } = request;
      const current = client.getQueryData<Snapshot | null>(["project"]);
      if (["edit_project", "revert_change"].includes(command) && !current) {
        throw new Error("Repository を開いてください。");
      }
      const revision = "revision" in request ? request.revision : current?.revision ?? 0;
      const edit = async (operation: Operation, snapshot: Snapshot) => {
        const prepared = await invoke<PreparedEdit>("preview_edit", { operation, revision: snapshot.revision });
        const calculated = evaluateScripts(prepared, snapshot.safeMode, operation);
        const update = await invoke<ProjectUpdate>("edit_project", { operation, calculated, revision: snapshot.revision });
        return applyProjectUpdate(snapshot, update);
      };
      let snapshot: Snapshot | null;
      if (command === "select_master") {
        if (!current) throw new Error("Repository を開いてください。");
        const masters: Snapshot["data"]["masters"] = {};
        if (request.masterId) masters[request.masterId] = await invoke("read_master", { root: current.root, masterId: request.masterId, revision });
        snapshot = { ...current, data: { config: current.data.config, masters } };
      } else {
        snapshot = command === "edit_project" && current
          ? await edit(request.operation, current)
          : command === "revert_change" && current
          ? await edit({ type: "revertChange", change: request.change }, current)
          : await invoke<Snapshot | null>(command, { ...args, revision });
      }
      const errors: string[] = [];
      // Preserve startup/branch recalculation, reading only masters with Script metadata.
      // Each preview/result is released before processing the next master.
      if (snapshot && !snapshot.safeMode && !snapshot.merge && [
        "open_project", "clone_project", "switch_branch", "git_update", "merge_branch", "complete_merge", "abort_merge",
      ].includes(command)) {
        const masterIds = await invoke<string[]>("script_masters").catch(error => { errors.push(String(error)); return []; });
        for (const masterId of masterIds) {
          try { snapshot = await edit({ type: "recalculateScripts", masterId }, snapshot); }
          catch (error) { errors.push(String(error)); }
        }
      }
      if (snapshot && current && ["git_fetch", "git_push", "set_identity"].includes(command)) {
        snapshot = { ...snapshot, data: { ...snapshot.data, masters: current.data.masters } };
      }
      // A review can revert a Master that is not currently open in the editor.
      if (snapshot) {
        const active = useUI.getState().masterId;
        snapshot = { ...snapshot, data: { ...snapshot.data, masters: Object.fromEntries(Object.entries(snapshot.data.masters).filter(([id]) => id === active)) } };
      }
      // Keep table data in one query, not in every completed mutation's result.
      client.setQueryData(["project"], snapshot);
      return { error: errors.length ? errors.join("\n\n") : null };
    },
    onSuccess: ({ error }, request) => {
      const snapshot = client.getQueryData<Snapshot | null>(["project"]);
      const active = useUI.getState().masterId;
      if (active && snapshot && !Object.hasOwn(snapshot.data.config.masters, active)) {
        useUI.getState().set({ masterId: null, selectedKeys: [], cell: null });
      }
      if (!["edit_project", "revert_change", "select_master"].includes(request.command)) {
        void client.invalidateQueries({ queryKey: ["history"] });
      }
      if (snapshot?.merge || request.command === "abort_merge" || request.command === "complete_merge" || request.command === "switch_branch" || request.command === "git_update" || request.command === "merge_branch") {
        useUI.getState().selectMaster(null);
      }
      useUI.getState().set({ error });
      if (
        request.command === "open_project" ||
        request.command === "clone_project" ||
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
      if (request.command !== "open_project" && request.command !== "clone_project" && request.command !== "close_project") {
        try {
          const fresh = await invoke<Snapshot>("get_project");
          const active = useUI.getState().masterId;
          if (active && !fresh.merge && Object.hasOwn(fresh.data.config.masters, active)) {
            fresh.data.masters[active] = await invoke("read_master", { root: fresh.root, masterId: active, revision: fresh.revision });
          }
          client.setQueryData(["project"], fresh);
        } catch { /* Preserve the original error. */ }
      }
    },
    onSettled: () => {
      // Only mounted, enabled review/Git dialogs refetch; cell editors do not.
      void client.invalidateQueries({ queryKey: ["changeReview"] });
      void client.invalidateQueries({ queryKey: ["repositoryState"] });
      void client.invalidateQueries({ queryKey: ["reviewSummary"] });
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

export function useReviewSummary(project: Snapshot) {
  return useQuery({
    queryKey: ["reviewSummary", project.root, project.revision],
    queryFn: () => invoke<ReviewSummary>("review_summary", { root: project.root, revision: project.revision }),
    gcTime: 0,
  });
}

export function useChangeReview(project: Snapshot, masterId: string) {
  return useQuery({
    queryKey: ["changeReview", project.root, project.revision, masterId],
    queryFn: () => invoke<ChangeReviewData>("change_review", { root: project.root, revision: project.revision, masterId }),
    gcTime: 0,
  });
}

export function useRepositoryState(project: Snapshot, enabled: boolean) {
  const client = useQueryClient();
  return useQuery({
    queryKey: ["repositoryState", project.root, project.revision],
    queryFn: async ({ signal }) => {
      const state = await invoke<RepositoryState>("repository_state", { root: project.root, revision: project.revision });
      if (!signal.aborted && !client.isMutating({ mutationKey: ["repository"] })) {
        client.setQueryData<Snapshot | null>(["project"], current => applyRepositoryState(current, state));
      }
      return state;
    },
    enabled,
    gcTime: 0,
    refetchOnMount: "always",
  });
}
