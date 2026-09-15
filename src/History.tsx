import { useState } from "react";
import { useHistory, useHistoryDetail } from "./api";
import { describeChange } from "./ChangeDescription";
import type { Snapshot } from "./types";

export function History({ project }: { project: Snapshot }) {
  const history = useHistory(project);
  const [selected, setSelected] = useState<string | null>(null);
  const [master, setMaster] = useState("");
  const commits = history.data?.pages.flatMap(p => p.commits) ?? [];
  const current = commits.find(c => c.oid === selected) ?? commits[0];
  const detail = useHistoryDetail(project.root, current?.oid ?? null);
  const changes = detail.data?.changes ?? [];
  const masters = [...new Set(changes.map(c => c.masterId))];
  return <div className="history-panel">
    <p className="hint">{project.git.branch || "detached HEAD"} のコミット履歴。Merge は先頭の親との差分を表示します。</p>
    {history.isPending && <p role="status">履歴を読み込み中…</p>}
    {history.error && <div role="alert" className="inline-error">履歴を取得できません: {String(history.error)} <button onClick={() => void history.refetch()}>再試行</button></div>}
    {!history.isPending && !history.error && !commits.length && <div className="empty-state">まだ Commit がありません。</div>}
    <div className="history-layout">
      <nav className="conflict-list" aria-label="Commit history">
        {commits.map(commit => <button key={commit.oid} className={current?.oid === commit.oid ? "active" : ""} onClick={() => { setSelected(commit.oid); setMaster(""); }}>
          <strong>{commit.subject}</strong>
          <code>{commit.oid.slice(0, 8)}{commit.parents.length > 1 ? " · Merge" : ""}</code>
          <small>{commit.author.name} · {new Date(commit.authoredAt).toLocaleString()}</small>
        </button>)}
        {history.hasNextPage && <button disabled={history.isFetchingNextPage} onClick={() => void history.fetchNextPage()}>{history.isFetchingNextPage ? "読み込み中…" : "さらに 50 件"}</button>}
      </nav>
      {current && <article className="history-detail">
        <h3>{current.subject}</h3>
        <p className="hint">{current.author.name} &lt;{current.author.email}&gt;<br />{new Date(current.authoredAt).toLocaleString()}</p>
        <code className="commit-oid">{current.oid}</code>
        {detail.isPending && <p role="status">変更内容を読み込み中…</p>}
        {detail.error && <div role="alert" className="inline-error">変更内容を読み込めません: {String(detail.error)} <button onClick={() => void detail.refetch()}>再試行</button></div>}
        {detail.data && <>
          <pre>{detail.data.message}</pre>
          <label>変更の対象<select value={master} onChange={e => setMaster(e.target.value)}><option value="">すべて ({changes.length})</option>{masters.map(id => <option key={id}>{id}</option>)}</select></label>
          {!changes.length && <p className="hint">Master / Comment / Project 設定の変更はありません。</p>}
          {changes.filter(c => !master || c.masterId === master).map((change, index) => <div className="change-item" key={index}><div><strong>{change.masterId}</strong><span>{describeChange(change)}</span></div></div>)}
        </>}
      </article>}
    </div>
  </div>;
}
