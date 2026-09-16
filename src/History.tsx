import { useState } from "react";
import { useHistory, useHistoryDetail } from "./api";
import { describeChange } from "./ChangeDescription";
import { changeLabels, Pager, previewValue } from "./ReviewControls";
import type { ChangeFilter, HistoryCommit, SemanticChange, Snapshot } from "./types";

export function History({ project }: { project: Snapshot }) {
  return <HistoryBrowser key={`${project.root}:${project.git.branch}`} project={project} />;
}
function HistoryBrowser({ project }: { project: Snapshot }) {
  const [cursors, setCursors] = useState<(string | null)[]>([null]);
  const [query, setQuery] = useState("");
  const [author, setAuthor] = useState("");
  const [search, setSearch] = useState({ query: "", author: "" });
  const [selected, setSelected] = useState<HistoryCommit | null>(null);
  const history = useHistory(project, cursors.at(-1)!, search.query, search.author);
  const commits = history.data?.commits ?? [];
  const current = selected ?? commits[0];
  return <div className="history-panel">
    <p className="hint">{project.git.branch || "detached HEAD"} · コミットを探し、対象・列で差分を絞り込めます。Merge は先頭の親と比較します。</p>
    <form className="result-filters" onSubmit={e => { e.preventDefault(); setSearch({ query: query.trim(), author: author.trim() }); setCursors([null]); setSelected(null); }}>
      <label>コミットメッセージ<input value={query} onChange={e => setQuery(e.target.value)} placeholder="全履歴から検索" autoCorrect="off" autoCapitalize="none" spellCheck={false} autoComplete="off" /></label>
      <label>作成者<input value={author} onChange={e => setAuthor(e.target.value)} placeholder="名前・メール" autoCorrect="off" autoCapitalize="none" spellCheck={false} autoComplete="off" /></label>
      <button type="submit">検索</button>
      <button type="button" onClick={() => { setQuery(""); setAuthor(""); setSearch({ query: "", author: "" }); setCursors([null]); setSelected(null); void history.refetch(); }}>最新の履歴へ</button>
    </form>
    <div className="history-layout">
      <section className="history-commits">
        {history.isPending && <p role="status">履歴を読み込み中…</p>}
        {history.error && <div role="alert" className="inline-error">履歴を取得できません: {String(history.error)} <button onClick={() => void history.refetch()}>再試行</button></div>}
        {!history.isPending && !history.error && !commits.length && <div className="empty-state">該当する Commit がありません。</div>}
        <nav className="conflict-list" aria-label="Commit history">
          {commits.map(commit => <button key={commit.oid} className={current?.oid === commit.oid ? "active" : ""} aria-current={current?.oid === commit.oid ? "true" : undefined} onClick={() => setSelected(commit)}>
            <strong>{commit.subject}</strong><code>{commit.oid.slice(0, 8)}{commit.parents.length > 1 ? " · Merge" : ""}</code>
            <small>{commit.author.name} · {new Date(commit.authoredAt).toLocaleString()}</small>
          </button>)}
        </nav>
        <div className="result-pager"><span>{cursors.length.toLocaleString()} ページ · 最大 50 件</span>
          <button disabled={history.isFetching || cursors.length === 1} onClick={() => { setCursors(c => c.slice(0, -1)); setSelected(null); }}>前へ</button>
          <button disabled={history.isFetching || !history.data?.nextCursor} onClick={() => { setCursors(c => [...c.slice(0, -1), history.data!.head, history.data!.nextCursor]); setSelected(null); }}>次へ</button>
        </div>
      </section>
      {current && <CommitDetail key={current.oid} root={project.root} commit={current} />}
    </div>
  </div>;
}
function CommitDetail({ root, commit }: { root: string; commit: HistoryCommit }) {
  const [filter, setFilter] = useState<ChangeFilter>({ master: "", kind: "", column: "", query: "", offset: 0 });
  const [query, setQuery] = useState("");
  const [selected, setSelected] = useState<SemanticChange | null>(null);
  const detail = useHistoryDetail(root, commit.oid, filter);
  const data = detail.data;
  const update = (next: Partial<ChangeFilter>) => { setFilter(f => ({ ...f, ...next, offset: next.offset ?? 0 })); setSelected(null); };
  return <article className="history-detail">
    <h3>{commit.subject}</h3>
    <p className="hint">{commit.author.name} &lt;{commit.author.email}&gt; · {new Date(commit.authoredAt).toLocaleString()}</p>
    <code className="commit-oid">{commit.oid}</code>
    <form className="result-filters" onSubmit={e => { e.preventDefault(); update({ query: query.trim() }); }}>
      <label>対象<select value={filter.master} onChange={e => update({ master: e.target.value, column: "", kind: "" })}><option value="">すべての Master</option>{Object.entries(data?.masters ?? (filter.master ? { [filter.master]: 0 } : {})).map(([id, n]) => <option key={id} value={id}>{id} ({n.toLocaleString()})</option>)}</select></label>
      <label>種類<select value={filter.kind} onChange={e => update({ kind: e.target.value })}><option value="">すべて</option>{Object.entries(data?.kinds ?? (filter.kind ? { [filter.kind]: 0 } : {})).map(([id, n]) => <option key={id} value={id}>{changeLabels[id] ?? id} ({n.toLocaleString()})</option>)}</select></label>
      <label>列<select value={filter.column} onChange={e => update({ column: e.target.value })}><option value="">すべての列</option>{Object.entries(data?.columns ?? (filter.column ? { [filter.column]: 0 } : {})).map(([id, n]) => <option key={id} value={id}>{id} ({n.toLocaleString()})</option>)}</select></label>
      <label>差分内検索<input value={query} onChange={e => setQuery(e.target.value)} placeholder="キー・変更前後の値" autoCorrect="off" autoCapitalize="none" spellCheck={false} autoComplete="off" /></label><button>絞り込む</button>
    </form>
    {detail.isPending && <p role="status">変更内容を読み込み中…</p>}
    {detail.error && <div role="alert" className="inline-error">変更内容を読み込めません: {String(detail.error)} <button onClick={() => void detail.refetch()}>再試行</button></div>}
    {data && <>
      <p className="result-summary">変更 {data.total.toLocaleString()} 件 · {Object.keys(data.masters).length.toLocaleString()} Master · 絞り込み {data.matched.toLocaleString()} 件</p>
      <details><summary>コミットメッセージ全文</summary><pre>{data.message}</pre></details>
      <Pager offset={data.offset} size={data.pageSize} total={data.matched} onChange={offset => update({ offset })} />
      <div className="result-table-wrap"><table className="result-table"><thead><tr><th>対象 / キー / 列</th><th>種類</th><th>変更前</th><th>変更後</th></tr></thead><tbody>
        {data.changes.map((change, i) => {
          const target = change.kind === "comment" ? change.target : change;
          const location = [change.masterId, "primaryKey" in target ? JSON.stringify(target.primaryKey) : "", "column" in target ? target.column : ""].filter(Boolean).join(" / ");
          return <tr key={i} className={selected === change ? "active" : ""}><td><button className="result-link" onClick={() => setSelected(change)}>{location}</button></td><td>{changeLabels[change.kind]}</td><td>{previewValue("before" in change ? change.before : null)}</td><td>{previewValue("after" in change ? change.after : null)}</td></tr>;
        })}
      </tbody></table></div>
      {!data.matched && <p className="empty-state">{data.total ? "条件に一致する差分はありません。" : "管理対象の変更はありません。"}</p>}
      {selected && <section className="selected-change"><h4>選択した変更の全文</h4><pre>{describeChange(selected)}</pre></section>}
    </>}
  </article>;
}
