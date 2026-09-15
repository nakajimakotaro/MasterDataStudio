import { open } from "@tauri-apps/plugin-dialog";
import {
  ArrowLeft,
  ArrowRight,
  Check,
  Database,
  FolderOpen,
  Grid2X2,
  GitBranch,
  GitCommit,
  Layers3,
  History as HistoryIcon,
  LoaderCircle,
  MessageSquare,
  Plus,
  Settings2,
  RefreshCw,
  X,
} from "lucide-react";
import {
  desktop,
  useBusy,
  useProject,
  useRecent,
  useRepositoryAction,
} from "./api";
import { useUI } from "./store";
import type { Snapshot } from "./types";
import { ConflictResolver } from "./ConflictResolver";
import { Editor } from "./Editor";
import { ProjectDialogs } from "./Dialogs";

export default function App() {
  const { data: project } = useProject();
  const { error, set, dialog } = useUI();
  return (
    <>
      {project ? <Workspace project={project} /> : <Launcher />}
      {error && !dialog && (
        <div className="error-toast" role="alert">
          <div>
            <strong>操作を完了できませんでした</strong>
            <p>{error}</p>
          </div>
          <button
            aria-label="エラーを閉じる"
            onClick={() => set({ error: null })}
          >
            <X size={18} />
          </button>
        </div>
      )}
    </>
  );
}

function Brand() {
  return (
    <div className="brand">
      <span className="brand-mark">
        <Layers3 size={21} />
      </span>
      <span>
        GameMaster<span className="brand-light">Studio</span>
      </span>
    </div>
  );
}

function Launcher() {
  const recent = useRecent();
  const action = useRepositoryAction();
  const busy = useBusy();
  const choose = async (initialize: boolean) => {
    try {
      const path = await open({
        directory: true,
        multiple: false,
        title: initialize
          ? "Project を初期化するフォルダ"
          : "Git Repository を開く",
      });
      if (typeof path === "string")
        action.mutate({ command: "open_project", path, initialize });
    } catch (error) {
      useUI.getState().set({ error: String(error) });
    }
  };
  return (
    <div className="launcher">
      <header>
        <Brand />
        <span className="version">
          DESKTOP <span>•</span> LOCAL WORKSPACE
        </span>
      </header>
      <main className="launcher-main">
        <div className="eyebrow">YOUR GAME STARTS WITH DATA</div>
        <h1>
          世界をつくる、
          <br />
          <span>データを整える。</span>
        </h1>
        <p className="lead">
          ゲームのマスターデータを、ひとつの作業場所で。
          <br />
          Repository を開いて、編集をはじめましょう。
        </p>
        {!desktop && (
          <div className="notice">
            Repository の操作はデスクトップアプリで利用できます。
            <br />
            <code>pnpm tauri dev</code> で起動してください。
          </div>
        )}
        <div className="launcher-actions">
          <button
            className="primary"
            disabled={busy || !desktop}
            onClick={() => void choose(false)}
          >
            <FolderOpen size={18} /> Repository を開く <ArrowRight size={17} />
          </button>
          <button disabled={busy || !desktop} onClick={() => void choose(true)}>
            <Plus size={18} /> Project を初期化
          </button>
        </div>
        <p className="hint">
          初期化は選択したフォルダに Project Config を作成します。
          <br />
          Git Repository でない場合は、新しい Repository も作成します。
        </p>
        <section className="recent-section">
          <div className="section-heading">
            <h2>最近開いた Project</h2>
            <span>{recent.data?.length ?? 0} PROJECTS</span>
          </div>
          {recent.error && (
            <p className="inline-error">
              最近の Project を読み込めません: {String(recent.error)}
            </p>
          )}
          {recent.data?.length ? (
            <div className="recent-list">
              {recent.data.map((path) => (
                <button
                  key={path}
                  disabled={busy}
                  onClick={() =>
                    action.mutate({
                      command: "open_project",
                      path,
                      initialize: false,
                    })
                  }
                >
                  <span className="project-icon">
                    <FolderOpen size={21} />
                  </span>
                  <span>
                    <strong>{path.split(/[\\/]/).pop()}</strong>
                    <small>{path}</small>
                  </span>
                  <ArrowRight size={17} />
                </button>
              ))}
            </div>
          ) : (
            <div className="empty-recent">
              <FolderOpen size={26} />
              <p>まだ Project がありません</p>
              <span>開いた Project がここに表示されます。</span>
            </div>
          )}
        </section>
      </main>
      <footer>
        <span>
          <span className="status-dot" />
          ローカルで管理・編集ごとに自動保存
        </span>
        <span>GameMasterStudio / 0.1.0</span>
      </footer>
    </div>
  );
}

function Workspace({ project }: { project: Snapshot }) {
  const { masterId, selectMaster, set, dialog } = useUI();
  const action = useRepositoryAction();
  const busy = useBusy();
  const ids = Object.keys(project.data.config.masters);
  const editable = !!project.identity.name && !!project.identity.email && !project.git.protected && !project.git.mergeInProgress;
  const entry = masterId ? project.data.masters[masterId] : null;
  return (
    <div className="workspace">
      <header className="app-header">
        <Brand />
        <div className="header-project">
          <span>/</span>
          <FolderOpen size={15} />
          <strong>{project.name}</strong>
        </div>
        <div className="save-state" aria-live="polite">
          {busy ? (
            <>
              <LoaderCircle className="spin" size={15} />
              処理中…
            </>
          ) : (
            <>
              <Check size={15} />
              {project.merge ? "Merge 解決中" : useUI.getState().error ? "直前の操作に失敗" : "すべて保存済み"}
            </>
          )}
        </div>
        <button className="git-pill" disabled={busy || project.git.mergeInProgress} onClick={() => set({ dialog: "branch" })} title="Branch を切替・作成">
          <GitBranch size={15} /> {project.git.branch || "detached"}
          {project.git.protected && <small>PROTECTED</small>}
        </button>
        <button disabled={busy} onClick={() => action.mutate({ command: "git_fetch" })} title="Fetch all / prune"><RefreshCw size={15} /> Fetch</button>
        <button disabled={busy || !project.git.upstream || project.git.trackedDirty || project.git.mergeInProgress} onClick={() => action.mutate({ command: "git_update" })}>Update{project.git.behind ? ` (${project.git.behind})` : ""}</button>
        <button disabled={busy || project.git.trackedDirty || project.git.mergeInProgress || project.git.protected} onClick={() => set({ dialog: "merge" })}>Merge</button>
        <button disabled={busy || project.git.mergeInProgress} onClick={() => set({ dialog: "changes" })}><GitCommit size={15} /> Changes <strong>{project.changes.length}</strong></button>
        <span className="remote-status" title={project.git.upstream ?? "upstream 未設定"}>{project.git.upstream ?? "upstream 未設定"} · ↑ {project.git.ahead} ↓ {project.git.behind}</span>
        <button disabled={busy || project.git.mergeInProgress || (!project.git.upstream && !project.git.remotes.includes("origin"))} onClick={() => action.mutate({ command: "git_push" })}>Push</button>
        <button
          className="icon-button"
          title="Project Settings"
          aria-label="Project Settings"
          disabled={busy}
          onClick={() => set({ dialog: "settings" })}
        >
          <Settings2 size={18} />
        </button>
      </header>
      <div className="workspace-body">
        <aside className="sidebar">
          <div className="workspace-label">WORKSPACE</div>
          <button
            className={`nav-item ${!masterId ? "active" : ""}`}
            onClick={() => selectMaster(null)}
          >
            <Grid2X2 size={17} />
            Overview
          </button>
          <button className="nav-item" disabled={busy} onClick={() => set({ dialog: "history" })}><HistoryIcon size={17} /> History</button>
          <div className="sidebar-section">
            <span>
              MASTERS <small>{ids.length}</small>
            </span>
            <button
              title="Master を作成"
              aria-label="Master を作成"
              disabled={!editable || busy}
              onClick={() => set({ dialog: "createMaster" })}
            >
              <Plus size={16} />
            </button>
          </div>
          <nav>
            {ids.map((id) => (
              <button
                className={`nav-item ${masterId === id ? "active" : ""}`}
                key={id}
                onClick={() => selectMaster(id)}
              >
                <Database size={16} />
                <span>{id}</span>
                {project.data.masters[id].error ? (
                  <span className="error-dot" />
                ) : (
                  <small>
                    {project.data.masters[
                      id
                    ].data?.table.rows.length.toLocaleString()}
                  </small>
                )}
              </button>
            ))}
          </nav>
          <div className="sidebar-bottom">
            <div className="identity-avatar">
              {(project.identity.name || "?").slice(0, 1).toUpperCase()}
            </div>
            <div>
              <strong>{project.identity.name || "Identity 未設定"}</strong>
              <small>
                {project.identity.email || "Settings から設定できます"}
              </small>
            </div>
          </div>
          <button
            className="back-button"
            disabled={busy}
            onClick={() => action.mutate({ command: "close_project" })}
          >
            <ArrowLeft size={15} />
            Project を閉じる
          </button>
        </aside>
        <main className="main-panel">
          {!editable && !project.merge && (
            <div className="identity-notice">
              {project.git.protected ? "Protected Branch は閲覧専用です。Working Branch を作成してください。" : "閲覧モードです。編集するには Git の名前とメールアドレスを設定してください。"}
              <button onClick={() => set({ dialog: project.git.protected ? "branch" : "settings" })}>
                {project.git.protected ? "Branch を作成" : "Identity を設定"} <ArrowRight size={14} />
              </button>
            </div>
          )}
          {project.merge ? <ConflictResolver project={project} /> : masterId && entry ? (
            entry.data ? (
              <Editor
                key={masterId}
                project={project}
                masterId={masterId}
                master={entry.data}
              />
            ) : (
              <div className="master-error">
                <Database size={38} />
                <h1>{masterId}</h1>
                <h2>この Master を読み込めません</h2>
                <p>{entry.error}</p>
                <code>{project.data.config.masters[masterId].path}</code>
                <p className="hint">
                  不正な Master は編集できません。設定・データを確認し、Project
                  を開き直してください。
                </p>
              </div>
            )
          ) : (
            <Dashboard project={project} />
          )}
        </main>
      </div>
      {dialog && <ProjectDialogs key={dialog} project={project} />}
    </div>
  );
}

function Dashboard({ project }: { project: Snapshot }) {
  const { selectMaster, set } = useUI();
  const busy = useBusy();
  const entries = Object.entries(project.data.masters);
  const rows = entries.reduce(
    (sum, [, e]) => sum + (e.data?.table.rows.length ?? 0),
    0,
  );
  const comments = entries.reduce(
    (sum, [, e]) =>
      sum +
      (e.data
        ? Number(!!e.data.comments.table) +
          e.data.comments.rows.length +
          e.data.comments.cells.length
        : 0),
    0,
  );
  return (
    <div className="dashboard">
      <div className="breadcrumb">
        Workspace <span>/</span> Overview
      </div>
      <div className="page-heading">
        <div>
          <div className="eyebrow">PROJECT OVERVIEW</div>
          <h1>{project.name}</h1>
          <p className="muted">
            マスターデータを選んで、編集をはじめましょう。
          </p>
        </div>
        <button
          className="primary"
          disabled={busy || !project.identity.name || !project.identity.email || project.git.protected || project.git.mergeInProgress}
          onClick={() => set({ dialog: "createMaster" })}
        >
          <Plus size={17} />
          Master を作成
        </button>
      </div>
      <div className="stats">
        <div>
          <Database size={20} />
          <span>Masters</span>
          <strong>{entries.length.toLocaleString()}</strong>
        </div>
        <div>
          <Grid2X2 size={20} />
          <span>Total rows</span>
          <strong>{rows.toLocaleString()}</strong>
        </div>
        <div>
          <MessageSquare size={20} />
          <span>Comments</span>
          <strong>{comments.toLocaleString()}</strong>
        </div>
      </div>
      <div className="section-heading">
        <h2>Masters</h2>
        <span>PRIMARY KEY で管理</span>
      </div>
      {entries.length ? (
        <div className="master-list">
          <div className="master-list-head">
            <span>MASTER / PATH</span>
            <span>PRIMARY KEY</span>
            <span>ROWS</span>
            <span>STATUS</span>
          </div>
          {entries.map(([id, entry]) => (
            <button
              className="master-list-row"
              key={id}
              onClick={() => selectMaster(id)}
            >
              <span className="master-name">
                <span className="project-icon">
                  <Database size={20} />
                </span>
                <span>
                  <strong>{id}</strong>
                  <small>{project.data.config.masters[id].path}</small>
                </span>
              </span>
              <span className="pk-tags">
                {project.data.config.masters[id].primaryKey.map((k) => (
                  <code key={k}>{k}</code>
                ))}
              </span>
              <span>
                {entry.data?.table.rows.length.toLocaleString() ?? "—"}
              </span>
              <span className={`badge ${entry.error ? "danger" : ""}`}>
                {entry.error ? "読み込みエラー" : project.changes.some(c => c.masterId === id) ? "Modified" : "Ready"}
                <ArrowRight size={14} />
              </span>
            </button>
          ))}
        </div>
      ) : (
        <div className="empty-state">
          <Database size={35} />
          <h2>最初の Master を作成</h2>
          <p>
            Column と Primary Key を定義すると、
            <br />
            ここからデータを編集できます。
          </p>
        </div>
      )}
      <div className="repository-path">
        <FolderOpen size={15} />
        <span>{project.root}</span>
      </div>
    </div>
  );
}
