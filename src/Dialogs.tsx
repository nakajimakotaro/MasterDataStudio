import {
  useEffect,
  useRef,
  useState,
  type FormEvent,
  type ReactNode,
} from "react";
import { KeyRound, X } from "lucide-react";
import { useBusy, useRepositoryAction } from "./api";
import { History } from "./History";
import { ChangeReview } from "./ChangeReview";
import { useUI } from "./store";
import type { Operation, Snapshot } from "./types";

function Modal({ title, children, className }: { title: string; children: ReactNode; className?: string }) {
  const ref = useRef<HTMLDialogElement>(null);
  const busy = useBusy();
  const error = useUI((state) => state.error);
  useEffect(() => {
    ref.current?.showModal();
  }, []);
  return (
    <dialog
      ref={ref}
      className={className}
      onCancel={(e) => {
        e.preventDefault();
        if (!busy) useUI.getState().set({ dialog: null });
      }}
    >
      <div className="modal-heading">
        <h2>{title}</h2>
        <button
          type="button"
          className="icon-button"
          disabled={busy}
          aria-label="閉じる"
          onClick={() => useUI.getState().set({ dialog: null })}
        >
          <X size={19} />
        </button>
      </div>
      {children}
      {error && (
        <div className="modal-error" role="alert">
          <strong>操作を完了できませんでした</strong>
          <p>{error}</p>
        </div>
      )}
    </dialog>
  );
}

export function ProjectDialogs({ project }: { project: Snapshot }) {
  const ui = useUI();
  const action = useRepositoryAction();
  const busy = useBusy();
  const masterId = ui.masterId ?? "";
  const master = project.data.masters[masterId]?.data;
  const def = project.data.config.masters[masterId];
  const [id, setId] = useState("");
  const [path, setPath] = useState(def?.path ?? "");
  const [columns, setColumns] = useState("id\nname");
  const [primaryKeys, setPrimaryKeys] = useState(
    def?.primaryKey.join("\n") ?? "id",
  );
  const [keys, setKeys] = useState<string[]>(
    def?.primaryKey.map(() => "") ?? [],
  );
  const [column, setColumn] = useState(
    ui.dialog === "deleteColumn"
      ? (master?.table.columns.find((c) => !def.primaryKey.includes(c)) ?? "")
      : "",
  );
  const [name, setName] = useState(project.identity.name);
  const [email, setEmail] = useState(project.identity.email);
  const [message, setMessage] = useState("");
  const [branch, setBranch] = useState("");
  const [branchQuery, setBranchQuery] = useState("");
  const [createBranch, setCreateBranch] = useState(project.git.protected);
  const branchTerms = branchQuery.toLowerCase().trim().split(/\s+/).filter(Boolean);
  const filteredBranches = project.git.branches.filter((candidate) =>
    branchTerms.every((term) => candidate.toLowerCase().includes(term)),
  );
  const [patterns, setPatterns] = useState(project.data.config.git.protectedBranches.join("\n"));
  const editable = !!project.identity.name && !!project.identity.email && !project.git.protected && !project.git.mergeInProgress;
  const lines = (text: string) => text.split(/\r?\n/).filter(Boolean);
  const edit = async (operation: Operation) => {
    try {
      await action.mutateAsync({ command: "edit_project", operation });
      ui.set({ dialog: null });
      if (operation.type === "createMaster")
        ui.selectMaster(operation.masterId);
    } catch {
      /* The shared error banner retains the command error. */
    }
  };
  const submit = (fn: () => void) => (event: FormEvent) => {
    event.preventDefault();
    if (!busy) fn();
  };
  const footer = (label: string, disabled = false, danger = false) => (
    <div className="modal-footer">
      <button
        type="button"
        disabled={busy}
        onClick={() => ui.set({ dialog: null })}
      >
        キャンセル
      </button>
      <button
        className={danger ? "danger-button" : "primary"}
        disabled={busy || disabled}
      >
        {busy ? "処理中…" : label}
      </button>
    </div>
  );

  if (ui.dialog === "history")
    return <Modal title="History"><History project={project} /></Modal>;

  if (ui.dialog === "settings")
    return (
      <Modal title="Project Settings">
        <form
          onSubmit={submit(() => {
            void action
              .mutateAsync({ command: "set_identity", name, email })
              .then(() => ui.set({ dialog: null }))
              .catch(() => {});
          })}
        >
          <div className="modal-body">
            <label>
              Repository
              <input readOnly value={project.root} />
            </label>
            <h3>Git Identity</h3>
            <p className="hint">
              この Repository の Git config
              に保存します。コメントの作者として使われます。
            </p>
            <label>
              名前
              <input
                required
                autoFocus
                value={name}
                onChange={(e) => setName(e.target.value)}
                placeholder="Taro Yamada"
              />
            </label>
            <label>
              メールアドレス
              <input
                required
                type="email"
                value={email}
                onChange={(e) => setEmail(e.target.value)}
                placeholder="taro@example.com"
              />
            </label>
          </div>
          {footer("Identity を設定", !name.trim() || !email.trim())}
        </form>
        <form onSubmit={submit(() => void edit({ type: "setProtectedBranches", patterns: lines(patterns) }))}>
          <div className="modal-body">
            <h3>Protected Branches</h3>
            <p className="hint">1 行に 1 pattern。大文字・小文字を区別します。例: main、develop、release/*</p>
            <label>保護する Branch<textarea rows={4} value={patterns} readOnly={!editable} onChange={e => setPatterns(e.target.value)} /></label>
            <p className="hint">設定は Project Config に自動保存し、Commit で共有します。変更は保護対象に含まれない Working Branch で行ってください。</p>
            {project.git.protected && <button type="button" disabled={busy || project.git.trackedDirty || project.git.mergeInProgress} onClick={() => ui.set({ dialog: "branch" })}>Working Branch を作成</button>}
          </div>
          {footer("保護設定を適用", !editable || patterns === project.data.config.git.protectedBranches.join("\n"))}
        </form>
      </Modal>
    );

  if (ui.dialog === "merge")
    return <Modal title="Branch を Merge"><form onSubmit={submit(() => {
      void action.mutateAsync({ command: "merge_branch", branch }).then(() => ui.set({ dialog: null })).catch(() => {});
    })}>
      <div className="modal-body">
        <p className="hint">{project.git.branch} に別の Branch を取り込みます。競合した場合は Conflict Resolver で解決します。</p>
        <label>Incoming Branch<input required autoFocus list="merge-branches" value={branch} onChange={e => setBranch(e.target.value)} placeholder="feature/enemy-balance" /></label>
        <datalist id="merge-branches">{project.git.branches.filter(b => b !== project.git.branch).map(b => <option key={b} value={b} />)}</datalist>
      </div>
      {footer("Merge", !branch.trim() || project.git.trackedDirty || project.git.mergeInProgress || project.git.protected)}
    </form></Modal>;

  if (ui.dialog === "branch")
    return <Modal title="Branch"><form onSubmit={submit(() => {
      if (!branch.trim() || (!createBranch && !filteredBranches.includes(branch)) || project.git.trackedDirty || project.git.mergeInProgress) return;
      void action.mutateAsync({command:"switch_branch", branch, create:createBranch}).then(()=>ui.set({dialog:null})).catch(()=>{});
    })}>
      <div className="modal-body"><p className="hint">切替・作成は Repository 全体の tracked changes と Index が clean な場合だけ実行できます。</p>
      {!createBranch && <>
        <label>Branch を検索
          <input
            type="search"
            autoFocus
            value={branchQuery}
            disabled={busy}
            placeholder="例: feature enemy"
            aria-describedby="branch-search-hint"
            onChange={(e) => { setBranchQuery(e.target.value); setBranch(""); }}
            onKeyDown={(e) => { if (e.key === "Enter") e.preventDefault(); }}
          />
        </label>
        <p id="branch-search-hint" className="hint">名前の一部で検索できます。大文字・小文字は区別せず、空白で複数のキーワードを指定できます。</p>
        <label>Branch
          <select
            className="branch-results"
            required
            size={8}
            value={branch}
            disabled={busy || !filteredBranches.length}
            onChange={(e) => setBranch(e.target.value)}
          >
            <option value="" disabled>選択してください</option>
            {filteredBranches.map((candidate) => <option key={candidate} value={candidate} title={candidate}>
              {candidate}{candidate === project.git.branch ? "（現在）" : ""}
            </option>)}
          </select>
        </label>
        <p className="hint" role="status">
          {filteredBranches.length} / {project.git.branches.length} 件
          {!filteredBranches.length && (project.git.branches.length ? " — 一致する Branch がありません。検索条件を変えてください。" : " — Branch がありません。新規作成してください。")}
        </p>
        {branch && <p className="branch-selection">選択中: <code>{branch}</code></p>}
      </>}
      {createBranch && <label>新しい Branch<input required autoFocus value={branch} onChange={e=>setBranch(e.target.value)} placeholder="feature/master-update" /></label>}
      <label className="check-row"><input type="checkbox" checked={createBranch} disabled={busy} onChange={e=>{setCreateBranch(e.target.checked);setBranch("");setBranchQuery("")}} />新規作成して切り替える</label></div>
      {footer(createBranch ? "作成して切替" : "切替", !branch.trim() || (!createBranch && !filteredBranches.includes(branch)) || project.git.trackedDirty || project.git.mergeInProgress)}
    </form></Modal>;

  if (ui.dialog === "changes")
    return <Modal title={`Change Review · ${project.changes.length}`} className="review-modal"><ChangeReview project={project} /></Modal>;

  if (ui.dialog === "commit")
    return <Modal title="Commit"><form onSubmit={submit(()=>{void action.mutateAsync({command:"commit",message,push:false}).then(()=>ui.set({dialog:null})).catch(()=>{});})}>
      <div className="modal-body"><p>{project.changes.length} semantic changes を一括 Commit します。</p><label>Commit message<textarea required autoFocus rows={4} value={message} onChange={e=>setMessage(e.target.value)} /></label></div>
      <div className="modal-footer"><button type="button" disabled={busy} onClick={()=>ui.set({dialog:"changes"})}>戻る</button><button type="button" disabled={busy||!editable||!message.trim()} onClick={()=>{void action.mutateAsync({command:"commit",message,push:true}).then(()=>ui.set({dialog:null})).catch(()=>{})}}>Commit & Push</button><button className="primary" disabled={busy||!editable||!message.trim()}>Commit</button></div>
    </form></Modal>;

  if (ui.dialog === "createMaster")
    return (
      <Modal title="Master を作成">
        <form
          onSubmit={submit(
            () =>
              void edit({
                type: "createMaster",
                masterId: id,
                path: path || `masters/${id}.csv`,
                columns: lines(columns),
                primaryKey: lines(primaryKeys),
              }),
          )}
        >
          <div className="modal-body">
            <p className="hint">
              すべての Cell は文字列です。Primary Key は複数の Column
              を組み合わせられます。
            </p>
            <label>
              Master ID
              <input
                required
                autoFocus
                pattern="[A-Za-z0-9][A-Za-z0-9_-]*"
                value={id}
                onChange={(e) => setId(e.target.value)}
                placeholder="enemy"
              />
            </label>
            <label>
              CSV path
              <input
                value={path}
                onChange={(e) => setPath(e.target.value)}
                placeholder={`masters/${id || "enemy"}.csv`}
              />
            </label>
            <div className="form-columns">
              <label>
                Columns <small>1 行に 1 Column・上から保存順</small>
                <textarea
                  required
                  rows={5}
                  value={columns}
                  onChange={(e) => setColumns(e.target.value)}
                />
              </label>
              <label>
                Primary Key <small>1 行に 1 Column・上から tuple 順</small>
                <textarea
                  required
                  rows={5}
                  value={primaryKeys}
                  onChange={(e) => setPrimaryKeys(e.target.value)}
                />
              </label>
            </div>
          </div>
          {footer("Master を作成", !editable)}
        </form>
      </Modal>
    );

  if (!master || !def) return null;
  if (ui.dialog === "masterSettings")
    return (
      <Modal title={`${masterId} / Settings`}>
        <form
          onSubmit={submit(
            () =>
              void edit({
                type: "configureMaster",
                masterId,
                path,
                primaryKey: lines(primaryKeys),
              }),
          )}
        >
          <div className="modal-body">
            <p className="hint">
              Row が 1 件以上ある Master の path / Primary Key
              は読み取り専用です。
            </p>
            <label>
              CSV path
              <input
                required
                readOnly={!!master.table.rows.length}
                value={path}
                onChange={(e) => setPath(e.target.value)}
              />
            </label>
            <label>
              Primary Key <small>1 行に 1 Column・上から tuple 順</small>
              <textarea
                required
                readOnly={!!master.table.rows.length}
                value={primaryKeys}
                onChange={(e) => setPrimaryKeys(e.target.value)}
              />
            </label>
            <div className="column-list">
              {master.table.columns.map((c) => (
                <div key={c}>
                  <code>{c}</code>
                  {def.primaryKey.includes(c) && (
                    <span>
                      <KeyRound size={13} />
                      Primary Key
                    </span>
                  )}
                </div>
              ))}
            </div>
          </div>
          {footer("設定を適用", !editable || !!master.table.rows.length)}
        </form>
      </Modal>
    );
  if (ui.dialog === "addRow" || ui.dialog === "duplicateRow")
    return (
      <Modal title={ui.dialog === "addRow" ? "Row を追加" : "Row を複製"}>
        <form
          onSubmit={submit(
            () =>
              void edit({
                type: "addRow",
                masterId,
                primaryKey: keys,
                duplicateFrom:
                  ui.dialog === "duplicateRow" ? ui.selectedKeys[0] : undefined,
              }),
          )}
        >
          <div className="modal-body">
            <p className="hint">
              新しい Primary Key を入力してください。作成後は変更できません。
              {ui.dialog === "duplicateRow"
                ? "値をコピーし、コメントはコピーしません。"
                : "その他の Cell は空文字で作成します。"}
            </p>
            {def.primaryKey.map((c, i) => (
              <label key={c}>
                <span>
                  <KeyRound size={13} /> {c}
                </span>
                <input
                  required
                  autoFocus={i === 0}
                  value={keys[i]}
                  onChange={(e) =>
                    setKeys(keys.map((v, n) => (n === i ? e.target.value : v)))
                  }
                />
              </label>
            ))}
          </div>
          {footer("Row を作成", !editable || keys.some((k) => !k))}
        </form>
      </Modal>
    );
  if (ui.dialog === "deleteRows")
    return (
      <Modal title="選択した Row を削除">
        <form
          onSubmit={submit(
            () =>
              void edit({
                type: "deleteRows",
                masterId,
                primaryKeys: ui.selectedKeys,
              }),
          )}
        >
          <div className="modal-body">
            <p>
              {ui.selectedKeys.length} 件の Row と、その Row / Cell Comment
              を削除します。
            </p>
            <div className="delete-keys">
              {ui.selectedKeys.slice(0, 10).map((k) => (
                <code key={JSON.stringify(k)}>{JSON.stringify(k)}</code>
              ))}
            </div>
            <p className="hint">Undo で元に戻せます。</p>
          </div>
          {footer("Row を削除", !editable || !ui.selectedKeys.length, true)}
        </form>
      </Modal>
    );
  if (ui.dialog === "addColumn")
    return (
      <Modal title="Column を追加">
        <form
          onSubmit={submit(
            () => void edit({ type: "addColumn", masterId, name: column }),
          )}
        >
          <div className="modal-body">
            <label>
              Column 名
              <input
                required
                autoFocus
                value={column}
                onChange={(e) => setColumn(e.target.value)}
                placeholder="defense"
              />
            </label>
            <p className="hint">
              末尾に追加します。既存 Row の値はすべて空文字です。
            </p>
          </div>
          {footer("Column を追加", !editable)}
        </form>
      </Modal>
    );
  if (ui.dialog === "deleteColumn")
    return (
      <Modal title="Column を削除">
        <form
          onSubmit={submit(
            () => void edit({ type: "deleteColumn", masterId, name: column }),
          )}
        >
          <div className="modal-body">
            <label>
              Column
              <select
                value={column}
                onChange={(e) => setColumn(e.target.value)}
              >
                {master.table.columns
                  .filter((c) => !def.primaryKey.includes(c))
                  .map((c) => (
                    <option key={c}>{c}</option>
                  ))}
              </select>
            </label>
            <p>この Column のすべての値と Cell Comment を削除します。</p>
            <p className="hint">
              Primary Key Column は削除できません。Undo で元に戻せます。
            </p>
          </div>
          {footer("Column を削除", !editable || !column, true)}
        </form>
      </Modal>
    );
  return null;
}
