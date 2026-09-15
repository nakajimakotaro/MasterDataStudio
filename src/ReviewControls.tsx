export function Pager({ offset, size, total, busy = false, onChange }: { offset: number; size: number; total: number; busy?: boolean; onChange: (offset: number) => void }) {
  return <div className="result-pager" aria-label="ページ移動">
    <span role="status">{total ? `${(offset + 1).toLocaleString()}–${Math.min(offset + size, total).toLocaleString()} / ${total.toLocaleString()} 件` : "0 件"}</span>
    <button disabled={busy || offset === 0} onClick={() => onChange(0)}>先頭</button>
    <button disabled={busy || offset === 0} onClick={() => onChange(Math.max(0, offset - size))}>前へ</button>
    <button disabled={busy || offset + size >= total} onClick={() => onChange(offset + size)}>次へ</button>
  </div>;
}
export const changeLabels: Record<string, string> = {
  cell: "セル", comment: "コメント", addedRow: "行追加", deletedRow: "行削除",
  addedColumn: "列追加", deletedColumn: "列削除", addedMaster: "Master 追加", deletedMaster: "Master 削除",
  projectConfig: "Project 設定", masterDefinition: "Master 定義",
};
export function previewValue(value: unknown): string {
  if (value === null || value === undefined) return "（なし）";
  if (value === "") return "（空文字）";
  const text = typeof value === "string" ? value : JSON.stringify(value);
  return text.length > 160 ? `${text.slice(0, 160)}…` : text;
}
