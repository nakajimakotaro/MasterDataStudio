import { useMemo, useState } from "react";
import { Check, GitMerge, RotateCcw } from "lucide-react";
import { useBusy, useRepositoryAction } from "./api";
import { Pager, previewValue } from "./ReviewControls";
import { emptyConflictFilter, filterConflicts, indexConflicts, unresolvedIds, type ConflictFilter } from "./conflictModel";
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
  const [filter, setFilter] = useState<ConflictFilter>(emptyConflictFilter);
  const [search, setSearch] = useState("");
  const [offset, setOffset] = useState(0);
  const [selection, setSelection] = useState<{ revision: number; ids: Set<string> }>({ revision: project.revision, ids: new Set() });
  const [batch, setBatch] = useState<{ ids: string[]; revision: number; side: "ours" | "theirs" } | null>(null);
  const batchSummary = useMemo(() => {
    if (!batch) return null;
    const ids = new Set(batch.ids);
    const targets = new Map<string, number>();
    let deletions = 0, overwrite = 0;
    for (const c of merge.conflicts) if (ids.has(c.id)) {
      const label = `${c.masterId} / ${labels[c.kind]}${c.column ? ` / ${c.column}` : ""}`;
      targets.set(label, (targets.get(label) ?? 0) + 1);
      if (c[batch.side] === null) deletions++;
      if (c.resolution) overwrite++;
    }
    return { targets: [...targets], deletions, overwrite };
  }, [batch, merge.conflicts]);
  const index = useMemo(() => indexConflicts(merge.conflicts), [merge.conflicts]);
  const filtered = useMemo(() => filterConflicts(index, filter), [index, filter]);
  const groups = useMemo(() => {
    const masters = new Map<string, { total: number; remaining: number }>();
    const columns = new Set<string>();
    for (const c of merge.conflicts) {
      const count = masters.get(c.masterId) ?? { total: 0, remaining: 0 };
      count.total++; if (!c.resolution) count.remaining++;
      masters.set(c.masterId, count);
      if ((!filter.master || c.masterId === filter.master) && c.column) columns.add(c.column);
    }
    return { masters, columns: [...columns].sort() };
  }, [merge.conflicts, filter.master]);
  const pageOffset = Math.min(offset, Math.max(0, Math.ceil(filtered.length / 100) - 1) * 100);
  const page = filtered.slice(pageOffset, pageOffset + 100);
  const selectedIds = selection.revision === project.revision ? selection.ids : new Set<string>();
  const current = page.find(c => c.id === selected) ?? page[0];
  const changeFilter = (next: Partial<ConflictFilter>) => {
    setFilter(f => ({ ...f, ...next })); setOffset(0); setSelected(null); setBatch(null);
    setSelection({ revision: project.revision, ids: new Set() });
  };
  const toggle = (id: string) => setSelection(() => {
    const ids = new Set(selectedIds); if (ids.has(id)) ids.delete(id); else ids.add(id);
    return { revision: project.revision, ids };
  });
  const editable = !busy && !merge.error && !project.git.protected && !!project.identity.name && !!project.identity.email;
  return <section className="merge-page">
    <div className="page-heading"><div><div className="eyebrow">MERGE</div><h1><GitMerge size={26} /> Conflict Resolver</h1>
      <p className="muted">自動マージ: {merge.automaticallyMerged.toLocaleString()} 件 · 残りの判断: {merge.remaining.toLocaleString()} 件</p></div>
      <button disabled={busy} onClick={() => setConfirmAbort(true)}><RotateCcw size={15} /> Merge を中止</button>
    </div>
    {confirmAbort && <div className="notice" role="alert"><p>Merge を中止して開始前の状態へ戻します。途中の解決内容は破棄されます。</p>
      <button disabled={busy} onClick={() => setConfirmAbort(false)}>戻る</button>{" "}
      <button className="danger-button" disabled={busy} onClick={() => action.mutate({ command: "abort_merge" })}>中止して元に戻す</button></div>}
    {merge.error && <div className="inline-error" role="alert">{merge.error}</div>}
    {project.git.protected && <p className="notice">Protected Branch では Merge commit を作成できません。Merge を中止してください。</p>}
    {!project.identity.name || !project.identity.email ? <p className="notice">解決・Merge 完了には Project Settings で Git Identity を設定してください。</p> : null}
    <div className="merge-progress"><progress aria-label="競合解決の進捗" max={merge.conflicts.length || 1} value={merge.conflicts.length - merge.remaining} /><span>{(merge.conflicts.length - merge.remaining).toLocaleString()} / {merge.conflicts.length.toLocaleString()} 件 解決済み</span></div>
    <form className="result-filters" onSubmit={e => { e.preventDefault(); changeFilter({ query: search }); }}>
      <label>対象<select value={filter.master} disabled={busy} onChange={e => changeFilter({ master: e.target.value, column: "" })}><option value="">すべての Master</option>{[...groups.masters].map(([id, n]) => <option key={id} value={id}>{id} · 未解決 {n.remaining.toLocaleString()} / {n.total.toLocaleString()}</option>)}</select></label>
      <label>種類<select value={filter.kind} disabled={busy} onChange={e => changeFilter({ kind: e.target.value })}><option value="">すべて</option>{Object.entries(labels).map(([id, label]) => <option key={id} value={id}>{label}</option>)}</select></label>
      <label>列<select value={filter.column} disabled={busy} onChange={e => changeFilter({ column: e.target.value })}><option value="">すべての列</option>{groups.columns.map(id => <option key={id}>{id}</option>)}</select></label>
      <label>状態<select value={filter.status} disabled={busy} onChange={e => changeFilter({ status: e.target.value as ConflictFilter["status"] })}><option value="unresolved">未解決</option><option value="resolved">解決済み</option><option value="all">すべて</option></select></label>
      <label>競合内検索<input value={search} disabled={busy} onChange={e => setSearch(e.target.value)} placeholder="キー・Base・両ブランチの値" autoCorrect="off" autoCapitalize="none" spellCheck={false} autoComplete="off" /></label><button disabled={busy}>絞り込む</button>
      <button type="button" disabled={busy} onClick={() => { setSearch(""); changeFilter(emptyConflictFilter); }}>条件をクリア</button>
    </form>
    <div className="batch-toolbar">
      <strong>{selectedIds.size.toLocaleString()} 件選択</strong>
      <button disabled={!editable || !page.length} onClick={() => setSelection({ revision: project.revision, ids: new Set([...selectedIds, ...unresolvedIds(page)]) })}>このページの未解決を選択</button>
      <button disabled={!editable || !filtered.length} onClick={() => setSelection({ revision: project.revision, ids: unresolvedIds(filtered) })}>絞り込み内の未解決を全選択</button>
      <button disabled={busy || !selectedIds.size} onClick={() => setSelection({ revision: project.revision, ids: new Set() })}>選択解除</button>
      <button disabled={!editable || !selectedIds.size} onClick={() => setBatch({ ids: [...selectedIds], revision: project.revision, side: "ours" })}>選択分を Keep Mine…</button>
      <button disabled={!editable || !selectedIds.size} onClick={() => setBatch({ ids: [...selectedIds], revision: project.revision, side: "theirs" })}>選択分を Use Incoming…</button>
    </div>
    {batch && <section className="batch-confirm notice" aria-label="一括解決の確認">
      <h3>{batch.ids.length.toLocaleString()} 件を {batch.side === "ours" ? "Your Branch" : "Incoming"} で解決</h3>
      <p>選択した競合にまとめて適用します。行・列・Master の競合では、その構造全体とデータが対象です。存在しない側を選ぶと削除になります。</p>
      <p>削除・不存在を採用: {batchSummary?.deletions.toLocaleString()} 件 · 既存の解決を変更: {batchSummary?.overwrite.toLocaleString()} 件</p>
      <ul>{batchSummary?.targets.slice(0, 10).map(([label, count]) => <li key={label}>{label}: {count.toLocaleString()} 件</li>)}</ul>
      {(batchSummary?.targets.length ?? 0) > 10 && <p>ほか {batchSummary!.targets.length - 10} グループ</p>}
      <button disabled={busy} onClick={() => setBatch(null)}>戻る</button>{" "}
      <button className="primary" disabled={!editable || batch.revision !== project.revision} onClick={() => action.mutate({ command: "resolve_conflicts", ids: batch.ids, resolution: { kind: batch.side }, revision: batch.revision }, { onSuccess: () => { setBatch(null); setSelection({ revision: -1, ids: new Set() }); } })}>{busy ? "適用中…" : `${batch.ids.length.toLocaleString()} 件に適用`}</button>
      {batch.revision !== project.revision && <p role="alert">競合が更新されました。対象を選び直してください。</p>}
    </section>}
    <Pager offset={pageOffset} size={100} total={filtered.length} busy={busy} onChange={n => { setOffset(n); setSelected(null); }} />
    <div className="conflict-workspace">
      <div className="result-table-wrap"><table className="result-table conflict-table"><thead><tr><th>選択</th><th>対象 / キー / 列</th><th>状態 / 種類</th><th>Your Branch</th><th>Incoming</th></tr></thead><tbody>
        {page.map(c => <tr key={c.id} className={current?.id === c.id ? "active" : ""}>
          <td><input type="checkbox" aria-label={`${c.masterId} ${JSON.stringify(c.primaryKey)} ${c.column ?? c.kind} を選択`} disabled={!editable} checked={selectedIds.has(c.id)} onChange={() => toggle(c.id)} /></td>
          <td><button className="result-link" onClick={() => setSelected(c.id)}>{c.masterId}<br /><code>{c.primaryKey ? JSON.stringify(c.primaryKey) : ""}{c.column ? ` / ${c.column}` : ""}</code></button></td>
          <td><span className={c.resolution ? "resolved-label" : "unresolved-label"}>{c.resolution ? "解決済み" : "未解決"}</span><small>{labels[c.kind]}</small></td><td>{previewValue(c.ours)}</td><td>{previewValue(c.theirs)}</td>
        </tr>)}
      </tbody></table>{!page.length && <div className="empty-state"><Check size={28} /><h2>{merge.error ? "Merge を読み込めません" : merge.remaining ? "条件に一致する競合はありません" : "すべて解決済みです"}</h2><p>状態や対象を変えて解決内容を確認できます。</p></div>}</div>
      {current && <ConflictDetail key={`${current.id}:${project.revision}`} conflict={current} editable={editable} onResolve={resolution => action.mutate({ command: "resolve_conflict", id: current.id, resolution })} />}
    </div>
    <div className="merge-footer">
      <label>Merge commit message（省略可）<input value={message} onChange={e => setMessage(e.target.value)} placeholder="Git の Merge message を使用" disabled={busy} autoCorrect="off" autoCapitalize="none" spellCheck={false} autoComplete="off" /></label>
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
      <label>{c.kind === "comment" ? "Combine / Edit manually（空白のみで削除）" : "Custom（空文字も指定できます）"}<textarea rows={4} value={custom} disabled={!editable} onChange={e => setCustom(e.target.value)} autoCorrect="off" autoCapitalize="none" spellCheck={false} autoComplete="off" /></label>
      <button disabled={!editable} className={(c.resolution?.kind === "custom" || c.resolution?.kind === "comment") ? "primary" : ""}>この値で Resolve</button>
    </form>}
  </article>;
}
