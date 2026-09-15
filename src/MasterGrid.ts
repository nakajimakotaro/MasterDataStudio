import { themeQuartz, type ColDef } from "ag-grid-community";

const cellCollator = new Intl.Collator("ja", { numeric: true });

export const masterGridTheme = themeQuartz.withParams({
  accentColor: "#437565",
  backgroundColor: "#ffffff",
  foregroundColor: "#273640",
  borderColor: "#e5e9ec",
  headerBackgroundColor: "#f7f9fa",
  headerTextColor: "#63727c",
  fontFamily:
    'Inter, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif',
  fontSize: 13,
  rowHeight: 40,
  headerHeight: 43,
  wrapperBorderRadius: 0,
  cellHorizontalPadding: 17,
});

export function masterColumn<T>(column: string, primaryKey: string[]): ColDef<T> {
  const pk = primaryKey.includes(column);
  return {
    colId: `data:${column}`,
    headerName: column,
    headerTooltip: pk ? `${column} · Primary Key（編集不可）` : column,
    pinned: pk ? "left" : undefined,
    lockPinned: true,
    lockPosition: pk ? "left" : undefined,
    cellDataType: "text",
    comparator: (a, b) => cellCollator.compare(String(a ?? ""), String(b ?? "")),
    initialSort: pk ? "asc" : undefined,
    initialSortIndex: pk ? primaryKey.indexOf(column) : undefined,
    cellClass: pk ? "pk-cell" : undefined,
    headerClass: pk ? "pk-header" : undefined,
    minWidth: pk ? 135 : 140,
    flex: pk ? undefined : 1,
  };
}
