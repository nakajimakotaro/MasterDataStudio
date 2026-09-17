import { useMemo, useState } from "react";
import { AgGridReact } from "ag-grid-react";
import type { ColDef } from "ag-grid-community";
import { KeyRound, RotateCcw, Search } from "lucide-react";
import { useBusy, useChangeReview, useRepositoryAction } from "./api";
import { describeChange } from "./ChangeDescription";
import { masterColumn, masterGridTheme } from "./MasterGrid";
import { buildReviewTable, changeTone, type ReviewRow, type ReviewTone } from "./reviewModel";
import { Pager } from "./ReviewControls";
import { useUI } from "./store";
import type { ChangeReviewData, SemanticChange, Snapshot } from "./types";

const labels: Record<ReviewTone, string> = { added: "追加", modified: "変更", deleted: "削除" };
function Badge({ tone }: { tone: ReviewTone }) {
  return <span className={`review-badge review-${tone}`}>{labels[tone]}</span>;
}

function Value({ value }: { value: string | null }) {
  return <pre>{value === null ? <em>（なし）</em> : value === "" ? <em>（空文字）</em> : value}</pre>;
}

function ChangeDetail({ change, disabled }: { change: SemanticChange; disabled: boolean }) {
  const action = useRepositoryAction();
  return <section className="review-change-detail">
    <Badge tone={changeTone(change)} />
    <p>{describeChange(change)}</p>
    {change.kind === "comment" && <span className="hint">コメントの変更</span>}
    <button disabled={disabled} title="この変更を HEAD の状態へ戻す" onClick={() => action.mutate({ command: "revert_change", change })}>
      <RotateCcw size={14} /> この変更を Revert
    </button>
  </section>;
}

export function ChangeReview({ project }: { project: Snapshot }) {
  const review = useChangeReview(project);
  const busy = useBusy();
  const ui = useUI();
  const editable = !!project.identity.name && !!project.identity.email && !project.git.protected && !project.git.mergeInProgress;
  return <>
    {review.isPending && <div className="review-message" role="status">変更前後のデータを読み込み中…</div>}
    {review.error && <div className="review-message inline-error" role="alert">差分を読み込めません: {String(review.error)} <button disabled={busy} onClick={() => void review.refetch()}>再試行</button></div>}
    {review.data && !review.error && <ReviewContent key={project.root} data={review.data} initialMaster={ui.masterId} disabled={busy || !editable || review.isFetching} />}
    {!!review.data?.scriptChanges?.length && <div className="review-message"><strong>Script metadata の変更（Commit 対象）</strong><ul>{review.data.scriptChanges.map(path => <li key={path}><code>{path}</code></li>)}</ul></div>}
    <div className="modal-footer">
      <button disabled={busy} onClick={() => ui.set({ dialog: null })}>閉じる</button>
      <button className="primary" disabled={busy || !editable || (!review.data?.changes.length && !review.data?.scriptChanges?.length) || review.isFetching || !!review.error} onClick={() => ui.set({ dialog: "commit" })}>Commit へ</button>
    </div>
  </>;
}

function ReviewContent({ data, initialMaster, disabled }: { data: ChangeReviewData; initialMaster: string | null; disabled: boolean }) {
  const [selectedMaster, setSelectedMaster] = useState(initialMaster);
  const counts = useMemo(() => {
    const counts = new Map<string, number>();
    for (const c of data.changes) counts.set(c.masterId, (counts.get(c.masterId) ?? 0) + 1);
    return counts;
  }, [data.changes]);
  const masters = [...counts.keys()];
  const masterId = masters.find(id => id === selectedMaster) ?? masters[0];
  if (!masterId) return <div className="review-message empty-state">CSV / Comment の変更はありません。</div>;
  return <div className="review-layout">
    <nav className="review-masters" aria-label="変更した Master">
      <div className="eyebrow">CHANGED MASTERS</div>
      {masters.map(id => <button key={id} className={id === masterId ? "active" : ""} aria-current={id === masterId ? "page" : undefined} onClick={() => setSelectedMaster(id)}>
        <strong>{id === "(Project Settings)" ? "Project Settings" : id}</strong>
        <small>{counts.get(id)!.toLocaleString()} 件の変更</small>
      </button>)}
    </nav>
    <ReviewMaster key={masterId} data={data} masterId={masterId} disabled={disabled} />
  </div>;
}

function ReviewMaster({ data, masterId, disabled }: { data: ChangeReviewData; masterId: string; disabled: boolean }) {
  const table = useMemo(() => buildReviewTable(data, masterId), [data, masterId]);
  const [onlyChanged, setOnlyChanged] = useState(true);
  const [search, setSearch] = useState("");
  const [selection, setSelection] = useState<{ rowId: string | null; column: string } | null>(null);
  const [visibleRows, setVisibleRows] = useState(0);
  const [detailOffset, setDetailOffset] = useState(0);
  const columns = useMemo<ColDef<ReviewRow>[]>(() => table ? [
    {
      colId: "review:status", headerName: "差分", width: 115, pinned: "left", lockPosition: "left", sortable: false, filter: false,
      valueGetter: p => p.data?.tones.map(t => labels[t]).join("・") ?? "",
      cellRenderer: (p: { data?: ReviewRow }) => <span className="review-row-badges">{p.data?.tones.map(t => <Badge key={t} tone={t} />)}</span>,
    },
    ...table.columns.map(column => ({
      ...masterColumn<ReviewRow>(column.name, table.def.primaryKey),
      headerName: `${column.name}${column.tone ? ` · ${labels[column.tone]}` : ""}`,
      headerClass: [table.def.primaryKey.includes(column.name) ? "pk-header" : "", column.tone ? `review-${column.tone}` : ""].filter(Boolean),
      valueGetter: (p) => {
        const cell = p.data?.cells.get(column.name);
        return cell?.after ?? cell?.before ?? "";
      },
      tooltipValueGetter: (p) => {
        const cell = p.data?.cells.get(column.name);
        if (!cell) return "";
        const value = (v: string | null) => v === null ? "（なし）" : v === "" ? "（空文字）" : v;
        return cell.tone ? `${labels[cell.tone]}: ${value(cell.before)} → ${value(cell.after)}` : value(cell.after ?? cell.before);
      },
      cellClassRules: {
        "review-added": p => p.data?.cells.get(column.name)?.tone === "added",
        "review-modified": p => p.data?.cells.get(column.name)?.tone === "modified",
        "review-deleted": p => p.data?.cells.get(column.name)?.tone === "deleted",
        "commented-cell": p => !!p.data?.cells.get(column.name)?.changes.some(c => c.kind === "comment"),
      },
    } satisfies ColDef<ReviewRow>)),
  ] : [], [table]);
  const rows = useMemo(() => table?.rows.filter(row => !onlyChanged || row.changed) ?? [], [table, onlyChanged]);
  const row = selection?.rowId ? table?.rows.find(row => row.id === selection.rowId) : undefined;
  const cell = row && selection ? row.cells.get(selection.column) : undefined;
  const selectedColumn = selection && !selection.rowId ? table?.columns.find(c => c.name === selection.column) : undefined;
  const relevant = cell?.changes ?? selectedColumn?.changes ?? table?.tableChanges ?? data.changes.filter(c => c.masterId === masterId);
  const offset = Math.min(detailOffset, Math.max(0, Math.ceil(relevant.length / 100) - 1) * 100);
  return <div className="editor review-editor">
    <div className="editor-heading">
      <div className="breadcrumb">Change Review <span>/</span> {masterId}</div>
      <div className="editor-title"><h1>{masterId === "(Project Settings)" ? "Project Settings" : masterId}</h1><span className="badge">閲覧専用</span>{table && <code>{table.def.path}</code>}</div>
    </div>
    {table && <div className="editor-toolbar review-toolbar">
      <div className="review-legend" aria-label="差分の色"><Badge tone="added" /><Badge tone="modified" /><Badge tone="deleted" /></div>
      <label className="review-filter"><input type="checkbox" checked={onlyChanged} onChange={e => setOnlyChanged(e.target.checked)} />変更のある行のみ</label>
      <div className="toolbar-spacer" />
      <label className="search-box"><Search size={16} /><input aria-label="レビュー内を検索" placeholder="検索…" value={search} onChange={e => setSearch(e.target.value)} autoCorrect="off" autoCapitalize="none" spellCheck={false} autoComplete="off" /></label>
    </div>}
    <div className="editor-content review-content">
      {table && <div className="grid-panel">
        <div className="grid-hint"><span><KeyRound size={13} />Primary Key 固定・削除した値も表示</span><span>セル・列見出しを選択して詳細を確認</span></div>
        <div className="ag-container">
          <AgGridReact<ReviewRow>
            theme={masterGridTheme}
            rowData={rows}
            columnDefs={columns}
            defaultColDef={{ sortable: true, filter: "agTextColumnFilter", resizable: true, suppressMovable: true, editable: false }}
            getRowId={p => p.data.id}
            quickFilterText={search}
            cellSelection
            suppressCutToClipboard
            suppressClipboardPaste
            suppressContextMenu
            onModelUpdated={e => setVisibleRows(e.api.getDisplayedRowCount())}
            onCellFocused={e => {
              if (e.rowIndex === null || !e.column || typeof e.column === "string") return;
              const row = e.api.getDisplayedRowAtIndex(e.rowIndex)?.data;
              const id = e.column.getColId();
              if (row && id.startsWith("data:")) { setSelection({ rowId: row.id, column: id.slice(5) }); setDetailOffset(0); }
            }}
            onColumnHeaderClicked={e => {
              if (!("getColId" in e.column)) return;
              const id = e.column.getColId();
              if (id.startsWith("data:")) { setSelection({ rowId: null, column: id.slice(5) }); setDetailOffset(0); }
            }}
            overlayNoRowsTemplate="<span>表示する行がありません。絞り込み条件や右側のテーブル・設定の変更を確認してください。</span>"
          />
        </div>
        <div className="grid-footer"><span>{visibleRows.toLocaleString()} / {table.rows.length.toLocaleString()} rows · {table.columns.length} columns</span><span>Review · 閲覧専用</span></div>
      </div>}
      <aside className={`review-inspector${table ? "" : " review-settings"}`} aria-label="変更の詳細">
        <div className="inspector-heading"><h2>変更の詳細</h2>{table && <button onClick={() => { setSelection(null); setDetailOffset(0); }}>テーブル・設定</button>}</div>
        <div className="review-details-body">
          {selection && (cell || selectedColumn) && <section className="review-selection">
            <strong>{selection.column}</strong>
            {row && <code>{JSON.stringify(row.key)}</code>}
            {cell && <><h3>変更前</h3><Value value={cell.before} /><h3>変更後</h3><Value value={cell.after} /></>}
          </section>}
          {!relevant.length && <p className="hint">{selection ? "この選択範囲に変更はありません。" : "色の付いたセルや列見出しを選択すると、変更前後と Revert を確認できます。"}</p>}
          {relevant.length > 100 && <Pager offset={offset} size={100} total={relevant.length} onChange={setDetailOffset} />}
          {relevant.slice(offset, offset + 100).map(change => <ChangeDetail key={JSON.stringify(change)} change={change} disabled={disabled || data.before === null} />)}
          {relevant.length > 0 && data.before === null && <p className="hint">最初の Commit 前は Revert を利用できません。</p>}
        </div>
      </aside>
    </div>
  </div>;
}
