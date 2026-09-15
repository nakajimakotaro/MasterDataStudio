import { useState } from "react";
import { Check, GitMerge, RotateCcw } from "lucide-react";
import { useBusy, useRepositoryAction } from "./api";
import type { Comment, Conflict, Resolution, Snapshot } from "./types";

const labels: Record<Conflict["kind"], string> = {
  projectConfig: "Project Config Conflict", master: "Master Conflict",
  column: "Column Conflict", row: "Row Conflict", cell: "Cell Conflict",
  comment: "Comment Conflict",
};
function commentValue(value: unknown): Comment | null {
  return value && typeof value === "object" && "body" in value ? value as Comment : null;
}
function display(value: unknown): string {
  if (value === null) return "ABSENT（存在しません）";
  if (value === "") return '""（空文字）';
  return typeof value === "string" ? value : JSON.stringify(value, null, 2);
}
export function ConflictResolver({ project }: { project: Snapshot }) {
  const merge = project.merge!;
  const action = useRepositoryAction();
  const busy = useBusy();
  const [selected, setSelected] = useState<string | null>(null);
  const [message, setMessage] = useState("");
  const [confirmAbort, setConfirmAbort] = useState(false);
  const current = merge.conflicts.find(c => c.id === selected)
    ?? merge.conflicts.find(c => !c.resolution) ?? merge.conflicts[0];
  const editable = !busy && !project.git.protected && !!project.identity.name && !!project.identity.email;
  return <section className="merge-page">
    <div className="page-heading"><div><div className="eyebrow">MERGE</div><h1><GitMerge size={26} /> Conflict Resolver</h1>
      <p className="muted">自動マージ: {merge.automaticallyMerged} 件 · 残りの判断: {merge.remaining} 件</p></div>
      <button disabled={busy} onClick={() => setConfirmAbort(true)}><RotateCcw size={15} /> Merge を中止</button>
    </div>
    {confirmAbort && <div className="notice" role="alert"><p>Merge を中止して開始前の状態へ戻します。途中の解決内容は破棄されます。</p>
      <button disabled={busy} onClick={() => setConfirmAbort(false)}>戻る</button>{" "}
      <button className="danger-button" disabled={busy} onClick={() => action.mutate({ command: "abort_merge" })}>中止して元に戻す</button></div>}
    {merge.error && <div className="inline-error" role="alert">{merge.error}</div>}
    {project.git.protected && <p className="notice">Protected Branch では Merge commit を作成できません。Merge を中止してください。</p>}
    {!project.identity.name || !project.identity.email ? <p className="notice">解決・Merge 完了には Project Settings で Git Identity を設定してください。</p> : null}
    <div className="merge-layout">
      <nav className="conflict-list" aria-label="Conflicts">{merge.conflicts.map(c => <button key={c.id}
        className={current?.id === c.id ? "active" : ""} onClick={() => setSelected(c.id)}>
        <strong>{c.resolution ? <Check size={14} /> : <span className="error-dot" />}{c.masterId}</strong>
        <small>{labels[c.kind]}</small><span>{c.primaryKey ? JSON.stringify(c.primaryKey) : ""}{c.column ? ` / ${c.column}` : ""}</span>
      </button>)}</nav>
      {current ? <ConflictDetail key={current.id} conflict={current} editable={editable} onResolve={resolution => action.mutate({ command: "resolve_conflict", id: current.id, resolution })} />
        : <div className="empty-state"><Check size={32} /><h2>{merge.error ? "Merge を読み込めません" : "すべて自動解決されました"}</h2></div>}
    </div>
    <div className="merge-footer">
      <label>Merge commit message（省略可）<input value={message} onChange={e => setMessage(e.target.value)} placeholder="Git の Merge message を使用" disabled={busy} /></label>
      <button className="primary" disabled={!editable || merge.remaining > 0 || !!merge.error} onClick={() => action.mutate({ command: "complete_merge", message })}>Complete Merge</button>
      <p className="hint">全件解決後、変更を保存・Stage し、Merge commit を作成します。途中で Project を閉じた場合、再度解決が必要です。</p>
    </div>
  </section>;
}
function ConflictDetail({ conflict: c, editable, onResolve }: { conflict: Conflict; editable: boolean; onResolve: (r: Resolution) => void }) {
  const [custom, setCustom] = useState(c.resolution?.kind === "custom" ? c.resolution.value : c.resolution?.kind === "comment" ? c.resolution.value?.body ?? "" : c.kind === "comment" ? commentValue(c.ours)?.body ?? commentValue(c.theirs)?.body ?? "" : typeof c.ours === "string" ? c.ours : "");
  return <article className="conflict-detail"><h2>{labels[c.kind]}</h2><p>{c.masterId} {c.primaryKey && JSON.stringify(c.primaryKey)} {c.column && `/ ${c.column}`}</p>
    {c.kind === "projectConfig" && <p className="notice">{c.masterId === "(CSV path / Master ID)" ? "CSV path または Master ID が衝突するため、Project 全体の定義とデータを選択します。" : c.masterId === "(Project Settings)" ? "Project の Git 設定を選択します。" : "Master の定義とデータをまとめて選択します。異なる Primary Key 間の Row 対応付けは行いません。"}</p>}
    {c.kind === "comment" && <p className="hint">{c.column ? "Cell" : c.primaryKey ? "Row" : "Table"} Comment · 本文を選択するか、手動で合成・編集してください。空白だけの本文は削除になります。</p>}
    <div className="merge-sides">{([ ["Base", c.base], ["Your Branch", c.ours], ["Incoming", c.theirs] ] as const).map(([title, value]) => <section key={title}><h3>{title}</h3><pre>{display(c.kind === "comment" ? commentValue(value)?.body ?? null : value)}</pre>{c.kind === "comment" && commentValue(value) && <small>更新: {commentValue(value)!.updatedBy.name}<br />{new Date(commentValue(value)!.updatedAt).toLocaleString()}</small>}</section>)}</div>
    <div className="resolve-actions">
      <button disabled={!editable} className={c.resolution?.kind === "ours" ? "primary" : ""} onClick={() => onResolve({ kind: "ours" })}>Keep Mine{c.ours === null ? "（削除）" : ""}</button>
      <button disabled={!editable} className={c.resolution?.kind === "theirs" ? "primary" : ""} onClick={() => onResolve({ kind: "theirs" })}>Use Incoming{c.theirs === null ? "（削除）" : ""}</button>
      {c.resolution && <span className="hint">選択済み・完了前に変更できます</span>}
    </div>
    {(c.kind === "cell" || c.kind === "comment") && <form onSubmit={e => { e.preventDefault(); if (editable) onResolve({ kind: "custom", value: custom }); }}>
      <label>{c.kind === "comment" ? "Combine / Edit manually（空白のみで削除）" : "Custom（空文字も指定できます）"}<textarea rows={4} value={custom} disabled={!editable} onChange={e => setCustom(e.target.value)} /></label>
      <button disabled={!editable} className={(c.resolution?.kind === "custom" || c.resolution?.kind === "comment") ? "primary" : ""}>この値で Resolve</button>
    </form>}
  </article>;
}
