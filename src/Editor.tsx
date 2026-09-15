import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { AgGridReact } from "ag-grid-react";
import {
  type ColDef,
  type GetContextMenuItems,
  type GridApi,
  type GridOptions,
  type ProcessDataFromClipboardParams,
} from "ag-grid-community";
import {
  Columns3,
  Copy,
  Eraser,
  MessageSquare,
  PanelRightClose,
  PanelRightOpen,
  Plus,
  Redo2,
  Search,
  Settings2,
  Trash2,
  Undo2,
} from "lucide-react";
import { masterColumn, masterGridTheme } from "./MasterGrid";
import { CellEditBatch } from "./cellEditBatch";
import { useBusy, useRepositoryAction } from "./api";
import { useUI, type DraftRow } from "./store";
import {
  keyId,
  rowKey,
  type CellEdit,
  type Comment,
  type CommentTarget,
  type Definition,
  type Master,
  type PrimaryKey,
  type Snapshot,
} from "./types";

type GridRow = { key: PrimaryKey; values: string[]; draft?: boolean };
const emptyDrafts: DraftRow[] = [];
// Keep arbitrary user column names separate from AG Grid's internal column IDs.
const columnName = (id: string) => (id.startsWith("data:") ? id.slice(5) : "");

// Selection updates must not rebuild columns or re-sort the grid: doing so can
// remove the cell anchoring an open context menu and immediately close it.
const editorGridOptions: Pick<GridOptions<GridRow>,
  "rowSelection" | "selectionColumnDef" | "postSortRows" | "getRowId"
> = {
  rowSelection: {
    mode: "multiRow",
    enableClickSelection: true,
    selectAll: "filtered",
    ctrlASelectsRows: true,
  },
  selectionColumnDef: {
    pinned: "left",
    width: 44,
    maxWidth: 44,
    resizable: false,
    suppressHeaderMenuButton: true,
    headerTooltip: "検索・フィルターに一致する全行を選択 / 解除",
  },
  postSortRows: ({ nodes }) => {
    // Keep saved rows sorted normally, and append drafts in creation order.
    nodes.sort((a, b) => {
      const aDraft = !!a.data?.draft;
      const bDraft = !!b.data?.draft;
      if (aDraft && bDraft) return a.sourceRowIndex - b.sourceRowIndex;
      return Number(aDraft) - Number(bDraft);
    });
  },
  getRowId: (p) => keyId(p.data.key),
};

export function Editor({
  project,
  masterId,
  master,
}: {
  project: Snapshot;
  masterId: string;
  master: Master;
}) {
  const ui = useUI();
  const action = useRepositoryAction();
  const busy = useBusy();
  const [api, setApi] = useState<GridApi<GridRow> | null>(null);
  const [visibleRows, setVisibleRows] = useState(master.table.rows.length);
  const [fillValue, setFillValue] = useState<string | null>(null);
  const [rangeCount, setRangeCount] = useState(0);
  const newRowKeys = useRef<string[]>([]);
  const fillEdits = useRef(new CellEditBatch());
  const grid = useRef<AgGridReact<GridRow>>(null);
  const def = project.data.config.masters[masterId];
  const editable = !!project.identity.name && !!project.identity.email && !project.git.protected && !project.git.mergeInProgress && !busy;
  const cellSelection = useMemo<GridOptions<GridRow>["cellSelection"]>(() => ({
    suppressMultiRanges: false,
    handle: editable ? {
      mode: "fill",
      direction: "xy",
      suppressClearOnFillReduction: true,
      setFillValue: (params) => {
        // Copy a single seed verbatim, including leading zeroes. Let AG Grid
        // extend numeric sequences and handle Alt-drag itself.
        if (params.initialValues.length === 1 && !params.event.altKey)
          return params.initialValues[0];
        return false;
      },
    } : undefined,
  }), [editable]);
  const draftScope = JSON.stringify([project.root, project.git.branch, masterId]);
  const drafts = ui.drafts[draftScope] ?? emptyDrafts;
  const setDrafts = useCallback((rows: DraftRow[]) => {
    const state = useUI.getState();
    state.set({ drafts: { ...state.drafts, [draftScope]: rows } });
  }, [draftScope]);
  const rowData = useMemo<GridRow[]>(() => [
    ...master.table.rows.map((values) => ({ key: rowKey(values, master, def), values })),
    ...drafts.map((row) => ({ key: row.key, values: master.table.columns.map(c => row.cells[c] ?? ""), draft: true })),
  ], [master.table, def, drafts]);
  const addRows = (sources: Pick<GridRow, "values">[] = [{ values: [] }]) => {
    if (!editable || !sources.length) return;
    const added = sources.map((source) => ({
      key: ["draft", crypto.randomUUID()],
      cells: Object.fromEntries(master.table.columns.map((c, i) =>
        [c, source.values[i] ?? ""])),
    }));
    newRowKeys.current = added.map(row => keyId(row.key));
    setDrafts([...drafts, ...added]);
    ui.set({ search: "", selectedKeys: added.map(row => row.key), cell: null });
    api?.setFilterModel(null);
  };
  const duplicateRows = (keys: PrimaryKey[]) => {
    const ids = new Set(keys.map(keyId));
    addRows(rowData.filter(row => ids.has(keyId(row.key))));
  };
  const deleteRows = (keys: PrimaryKey[]) => {
    const ids = new Set(keys.map(keyId));
    setDrafts(drafts.filter(row => !ids.has(keyId(row.key))));
    const savedKeys = keys.filter(key => !drafts.some(row => keyId(row.key) === keyId(key)));
    ui.set({ selectedKeys: savedKeys, cell: null, dialog: savedKeys.length ? "deleteRows" : null });
  };
  const saveDrafts = () => {
    action.mutate({ command: "edit_project", operation: {
      type: "createRows", masterId,
      rows: drafts.map(row => master.table.columns.map(c => row.cells[c] ?? "")),
    } }, { onSuccess: () => setDrafts([]) });
  };
  const edit = useCallback((edits: CellEdit[]) => {
    if (!editable || !edits.length) return;
    const isDraft = (e: CellEdit) => drafts.some(row => keyId(row.key) === keyId(e.primaryKey));
    const updateDrafts = () => setDrafts(drafts.map(row => {
      const cells = { ...row.cells };
      for (const e of edits) if (keyId(e.primaryKey) === keyId(row.key)) cells[e.column] = e.value;
      return { ...row, cells };
    }));
    const savedEdits = edits.filter(e => !isDraft(e));
    if (savedEdits.length) action.mutate({
      command: "edit_project",
      operation: { type: "editCells", masterId, edits: savedEdits },
    }, { onSuccess: updateDrafts });
    else updateDrafts();
  }, [editable, drafts, setDrafts, action.mutate, masterId]);
  const selectionEdits = useCallback((
    value: string,
    copyTopRow = false,
  ): CellEdit[] => {
    if (!api) return [];
    const edits = new Map<string, CellEdit>();
    for (const range of api.getCellRanges() ?? []) {
      if (!range.startRow || !range.endRow) continue;
      for (
        let i = Math.min(range.startRow.rowIndex, range.endRow.rowIndex);
        i <= Math.max(range.startRow.rowIndex, range.endRow.rowIndex);
        i++
      ) {
        const row = api.getDisplayedRowAtIndex(i)?.data;
        if (!row) continue;
        for (const col of range.columns) {
          const column = columnName(col.getColId());
          if (master.table.columns.includes(column))
            edits.set(JSON.stringify([row.key, column]), {
              primaryKey: row.key,
              column,
              value: copyTopRow
                ? (api.getDisplayedRowAtIndex(
                    Math.min(range.startRow.rowIndex, range.endRow.rowIndex),
                  )?.data?.values[master.table.columns.indexOf(column)] ?? "")
                : value,
            });
        }
      }
    }
    return [...edits.values()];
  }, [api, master.table.columns]);
  const defaultColDef = useMemo<ColDef<GridRow>>(() => ({
    sortable: true,
    filter: "agTextColumnFilter",
    resizable: true,
    suppressMovable: true,
    suppressKeyboardEvent: (p) => {
      if (
        !p.editing &&
        (p.event.key === "Delete" || p.event.key === "Backspace")
      ) {
        edit(selectionEdits(""));
        return true;
      }
      if (
        !p.editing &&
        (p.event.metaKey || p.event.ctrlKey) &&
        p.event.key.toLowerCase() === "x"
      )
        return true;
      return false;
    },
  }), [edit, selectionEdits]);
  const columnDefs = useMemo<ColDef<GridRow>[]>(() => {
    const order = [
      ...def.primaryKey,
      ...master.table.columns.filter((c) => !def.primaryKey.includes(c)),
    ];
    return order.map((column) => {
      const index = master.table.columns.indexOf(column);
      return {
        ...masterColumn<GridRow>(column, def.primaryKey),
        valueGetter: (p) => p.data?.values[index] ?? "",
        valueParser: (p) => String(p.newValue ?? ""),
        editable,
        tooltipValueGetter: (p) => {
          const comment = master.comments.cells.find(
            (c) =>
              keyId(c.primaryKey) === keyId(p.data?.key ?? []) &&
              c.column === column,
          );
          return comment?.comment.body ?? String(p.value ?? "");
        },
        cellClassRules: {
          "commented-cell": (p) =>
            master.comments.cells.some(
              (c) =>
                keyId(c.primaryKey) === keyId(p.data?.key ?? []) &&
                c.column === column,
            ),
        },
      };
    });
  }, [def, master.table.columns, master.comments, editable]);

  const paste = (params: ProcessDataFromClipboardParams<GridRow>) => {
    const focus = params.api.getFocusedCell();
    if (!focus || !editable) return null;
    const columns = params.api.getAllDisplayedColumns();
    const start = columns.findIndex(
      (c) => c.getColId() === focus.column.getColId(),
    );
    const edits: CellEdit[] = [];
    const data = [...params.data];
    if (data.length > 1 && data.at(-1)?.length === 1 && data.at(-1)?.[0] === "")
      data.pop();
    for (let y = 0; y < data.length; y++) {
      const row = params.api.getDisplayedRowAtIndex(focus.rowIndex + y)?.data;
      if (!row || start + data[y].length > columns.length) {
        ui.set({
          error:
            "貼り付け範囲がテーブルを超えています。必要な Row / Column を先に追加してください。",
        });
        return null;
      }
      for (let x = 0; x < data[y].length; x++)
        edits.push({
          primaryKey: row.key,
          column: columnName(columns[start + x].getColId()),
          value: data[y][x],
        });
    }
    edit(edits);
    return null;
  };

  const contextMenuItems: GetContextMenuItems<GridRow> = (params) => {
    const node = params.node;
    const row = node?.data;
    // Right-clicking outside the selection targets that row. Within a
    // selection, retain all selected rows for bulk duplication and deletion.
    if (node && row && !node.isSelected()) node.setSelected(true, true);
    const selectedKeys = params.api.getSelectedRows().map((r) => r.key);
    const column = columnName(params.column?.getColId() ?? "");
    const cell = row && master.table.columns.includes(column)
      ? { primaryKey: row.key, column }
      : null;
    if (row) ui.set({ selectedKeys, cell });

    return [
      {
        name: "Row を追加",
        disabled: !editable,
        action: () => addRows(),
      },
      {
        name: selectedKeys.length > 1
          ? `選択した ${selectedKeys.length} 件の Row を複製`
          : "この Row を複製",
        disabled: !editable || !row,
        action: () => {
          if (!row) return;
          duplicateRows(selectedKeys);
        },
      },
      {
        name: selectedKeys.length > 1
          ? `選択した ${selectedKeys.length} 件の Row を削除`
          : "この Row を削除",
        disabled: !editable || !row,
        action: () => {
          if (!row) return;
          deleteRows(selectedKeys);
        },
      },
      ...(row ? ["separator", "copy", "copyWithHeaders"] as const : []),
    ];
  };

  useEffect(() => {
    const keys = new Set(rowData.map((row) => keyId(row.key)));
    const state = useUI.getState();
    const selectedKeys = state.selectedKeys.filter((key) =>
      keys.has(keyId(key)),
    );
    const cell =
      state.cell &&
      keys.has(keyId(state.cell.primaryKey)) &&
      master.table.columns.includes(state.cell.column)
        ? state.cell
        : null;
    state.set({ selectedKeys, cell });
  }, [rowData, master.table.columns]);

  useEffect(() => {
    const keydown = (event: KeyboardEvent) => {
      if (
        !(event.metaKey || event.ctrlKey) ||
        event.key.toLowerCase() !== "z" ||
        event.altKey
      )
        return;
      if (
        (event.target as HTMLElement)?.closest(
          'input, textarea, [contenteditable="true"], dialog',
        )
      )
        return;
      event.preventDefault();
      if (!editable) return;
      const command = event.shiftKey ? "redo" : "undo";
      if (command === "undo" ? project.canUndo : project.canRedo)
        action.mutate({ command });
    };
    document.addEventListener("keydown", keydown);
    return () => document.removeEventListener("keydown", keydown);
  }, [editable, project.canUndo, project.canRedo, action]);

  return (
    <div className="editor">
      <div className="editor-heading">
        <div className="breadcrumb">
          <button onClick={() => ui.selectMaster(null)}>Masters</button>
          <span>/</span>
          {masterId}
        </div>
        <div className="editor-title editor-master-title">
          <h1 title={masterId}>{masterId}</h1>
          <span className="badge">MASTER</span>
          <TableComment
            key={masterId}
            masterId={masterId}
            comment={master.comments.table}
            editable={editable}
          />
          <code>{def.path}</code>
          <button
            className="icon-button"
            aria-label="Master Settings"
            title="Master Settings"
            disabled={busy}
            onClick={() => ui.set({ dialog: "masterSettings" })}
          >
            <Settings2 size={16} />
          </button>
        </div>
      </div>
      <div className="editor-toolbar">
        <div className="toolbar-group">
          <button
            disabled={!editable}
            onClick={() => addRows()}
          >
            <Plus size={16} />
            Row 追加
          </button>
          <button
            disabled={!editable || !ui.selectedKeys.length}
            title={ui.selectedKeys.length > 1
              ? `選択した ${ui.selectedKeys.length} 件の Row を複製`
              : "選択した Row を複製"}
            aria-label="Row を複製"
            onClick={() => duplicateRows(ui.selectedKeys)}
          >
            <Copy size={16} />
          </button>
          <button
            disabled={!editable || !ui.selectedKeys.length}
            title="選択した Row を削除"
            aria-label="Row を削除"
            onClick={() => deleteRows(ui.selectedKeys)}
          >
            <Trash2 size={16} />
          </button>
        </div>
        <div className="toolbar-group">
          <button
            disabled={!editable}
            onClick={() => ui.set({ dialog: "addColumn" })}
          >
            <Columns3 size={16} />
            Column 追加
          </button>
          <button
            disabled={
              !editable ||
              !master.table.columns.some((c) => !def.primaryKey.includes(c))
            }
            title="Column を削除"
            aria-label="Column を削除"
            onClick={() => ui.set({ dialog: "deleteColumn" })}
          >
            <Trash2 size={16} />
          </button>
        </div>
        <div className="toolbar-group">
          <button
            disabled={!editable || !project.canUndo}
            title="Undo · ⌘/Ctrl Z"
            aria-label="Undo"
            onClick={() => action.mutate({ command: "undo" })}
          >
            <Undo2 size={17} />
          </button>
          <button
            disabled={!editable || !project.canRedo}
            title="Redo · ⌘/Ctrl Shift Z"
            aria-label="Redo"
            onClick={() => action.mutate({ command: "redo" })}
          >
            <Redo2 size={17} />
          </button>
        </div>
        <div className="toolbar-group">
          <button
            disabled={!editable || !rangeCount}
            title="空文字にする"
            aria-label="空文字にする"
            onClick={() => edit(selectionEdits(""))}
          >
            <Eraser size={16} />
          </button>
          <button
            disabled={!editable || !rangeCount}
            onClick={() => setFillValue("")}
          >
            選択範囲を埋める
          </button>
        </div>
        <div className="toolbar-spacer" />
        <label className="search-box">
          <Search size={16} />
          <input
            aria-label="Master 内を検索"
            value={ui.search}
            onChange={(e) => ui.set({ search: e.target.value })}
            placeholder="検索…"
          />
        </label>
        <button
          className={ui.inspector ? "toggled" : ""}
          aria-label="Inspector 表示切り替え"
          title="Inspector"
          onClick={() => ui.set({ inspector: !ui.inspector })}
        >
          {ui.inspector ? (
            <PanelRightClose size={18} />
          ) : (
            <PanelRightOpen size={18} />
          )}
        </button>
      </div>
      {drafts.length > 0 && <div className="fill-bar">
        <span>未保存の新規行: {drafts.length} 件 · Primary Key を入力して保存してください。</span>
        <button className="primary" disabled={!editable || drafts.some(row => def.primaryKey.some(c => !row.cells[c]))} onClick={saveDrafts}>新規行を保存</button>
      </div>}
      <div className="editor-content">
        <div className="grid-panel">
          {fillValue !== null && (
            <form
              className="fill-bar"
              onSubmit={(e) => {
                e.preventDefault();
                edit(selectionEdits(fillValue));
                setFillValue(null);
              }}
            >
              <label>
                選択したセルに同じ値を設定
                <input
                  autoFocus
                  value={fillValue}
                  onChange={(e) => setFillValue(e.target.value)}
                />
              </label>
              <button className="primary" disabled={!editable}>
                適用
              </button>
              <button type="button" onClick={() => setFillValue(null)}>
                キャンセル
              </button>
            </form>
          )}
          <div
            className="ag-container"
            onKeyDownCapture={(event) => {
              if (!(event.metaKey || event.ctrlKey)) return;
              if (
                event.key.toLowerCase() === "d" &&
                !api?.getEditingCells().length
              ) {
                event.preventDefault();
                event.stopPropagation();
                edit(selectionEdits("", true));
              } else if (
                event.key === "Enter" &&
                api?.getEditingCells().length
              ) {
                event.preventDefault();
                event.stopPropagation();
                const value = api.getCellEditorInstances()[0]?.getValue();
                api.stopEditing(true);
                edit(selectionEdits(String(value ?? "")));
              }
            }}
          >
            <AgGridReact<GridRow>
              {...editorGridOptions}
              cellSelection={cellSelection}
              ref={grid}
              theme={masterGridTheme}
              rowData={rowData}
              columnDefs={columnDefs}
              defaultColDef={defaultColDef}
              readOnlyEdit
              stopEditingWhenCellsLoseFocus
              suppressCutToClipboard
              getContextMenuItems={contextMenuItems}
              allowContextMenuWithControlKey
              quickFilterText={ui.search}
              processDataFromClipboard={paste}
              onGridReady={(e) => setApi(e.api)}
              onModelUpdated={(e) => {
                setVisibleRows(e.api.getDisplayedRowCount());
                if (!newRowKeys.current.length) return;
                const nodes = newRowKeys.current.flatMap(key => {
                  const node = e.api.getRowNode(key);
                  return node ? [node] : [];
                });
                if (nodes.length !== newRowKeys.current.length) return;
                newRowKeys.current = [];
                e.api.deselectAll();
                e.api.setNodesSelected({ nodes, newValue: true });
                const node = nodes[0];
                e.api.ensureNodeVisible(node, "bottom");
                if (node.rowIndex !== null) e.api.setFocusedCell(node.rowIndex, `data:${def.primaryKey[0]}`);
              }}
              onCellSelectionChanged={() =>
                setRangeCount(selectionEdits("").length)
              }
              onSelectionChanged={(e) =>
                ui.set({
                  selectedKeys: e.api.getSelectedRows().map((r) => r.key),
                  cell: null,
                })
              }
              onCellFocused={(e) => {
                if (
                  e.rowIndex === null ||
                  !e.column ||
                  typeof e.column === "string"
                )
                  return;
                const row = e.api.getDisplayedRowAtIndex(e.rowIndex)?.data;
                const column = columnName(e.column.getColId());
                if (row && master.table.columns.includes(column))
                  ui.set({ cell: { primaryKey: row.key, column } });
              }}
              onFillStart={() => fillEdits.current.start()}
              onFillEnd={() => edit(fillEdits.current.finish())}
              onCellEditRequest={(e) => {
                if (String(e.newValue ?? "") === String(e.oldValue ?? "")) return;
                edit(fillEdits.current.request({
                  primaryKey: e.data.key,
                  column: columnName(e.column.getColId()),
                  value: String(e.newValue ?? ""),
                }));
              }}
              overlayNoRowsTemplate="<span>Row がありません。「Row 追加」から作成できます。</span>"
            />
          </div>
          <div className="grid-footer">
            <span>
              {visibleRows.toLocaleString()} /{" "}
              {rowData.length.toLocaleString()} rows
              <span className="footer-separator">·</span>
              {master.table.columns.length} columns
            </span>
            <span>
              {ui.selectedKeys.length
                ? `${ui.selectedKeys.length} rows selected`
                : "Double-click to edit"}
              <span className="footer-separator">·</span>UTF-8 / LF
            </span>
          </div>
        </div>
        {ui.inspector && (
          <Inspector
            masterId={masterId}
            master={master}
            def={def}
            draft={rowData.find(row => row.draft && keyId(row.key) === keyId(ui.cell?.primaryKey ?? ui.selectedKeys[0] ?? []))}
            editable={editable}
          />
        )}
      </div>
    </div>
  );
}

function Inspector({
  masterId,
  master,
  def,
  draft,
  editable,
}: {
  masterId: string;
  master: Master;
  def: Definition;
  draft?: GridRow;
  editable: boolean;
}) {
  const ui = useUI();
  const key =
    ui.cell?.primaryKey ??
    (ui.selectedKeys.length === 1 ? ui.selectedKeys[0] : null);
  const row = draft?.values ?? (key
    ? master.table.rows.find(
        (r) => keyId(rowKey(r, master, def)) === keyId(key),
      )
    : null);
  const column = ui.cell?.column;
  const rowComment = master.comments.rows.find(
    (c) => keyId(c.primaryKey) === keyId(key ?? []),
  )?.comment;
  const cellComment = master.comments.cells.find(
    (c) => keyId(c.primaryKey) === keyId(key ?? []) && c.column === column,
  )?.comment;
  return (
    <aside className="inspector">
      <div className="inspector-heading">
        <MessageSquare size={16} />
        <h2>Inspector</h2>
      </div>
      <section className="selection-details">
        <div className="eyebrow">SELECTION</div>
        <strong>{masterId}</strong>
        {key && (
          <div className="key-details">
            {def.primaryKey.map((c, i) => (
              <div key={c}>
                <span>{c}</span>
                <code>{draft ? draft.values[master.table.columns.indexOf(c)] || "未入力" : key[i]}</code>
              </div>
            ))}
          </div>
        )}
        {column && row && (
          <div className="value-details">
            <label>{column}</label>
            <pre>
              {row[master.table.columns.indexOf(column)] || (
                <span className="empty-value">Empty string</span>
              )}
            </pre>
          </div>
        )}
        {!key && (
          <p className="hint">
            セルを選択すると、関連する Row / Cell Comment を表示します。
          </p>
        )}
      </section>
      {key && row && !draft && (
        <CommentBox
          key={`row:${keyId(key)}:${rowComment?.body}`}
          label="Row Comment"
          comment={rowComment}
          masterId={masterId}
          target={{ kind: "row", primaryKey: key }}
          editable={editable}
        />
      )}
      {key && row && column && !draft && (
        <CommentBox
          key={`cell:${keyId(key)}:${column}:${cellComment?.body}`}
          label="Cell Comment"
          comment={cellComment}
          masterId={masterId}
          target={{ kind: "cell", primaryKey: key, column }}
          editable={editable}
        />
      )}
      <div className="inspector-note">
        コメントは plain text です。
        <br />
        入力欄を離れると自動保存されます。
      </div>
    </aside>
  );
}

function TableComment({ masterId, comment, editable }: {
  masterId: string;
  comment: Comment | null;
  editable: boolean;
}) {
  const details = useRef<HTMLDetailsElement>(null);
  const preview = comment?.body || (editable ? "コメントを追加" : "コメントなし");
  useEffect(() => {
    const close = (event: PointerEvent | KeyboardEvent) => {
      const element = details.current;
      if (!element?.open) return;
      const escape = event instanceof KeyboardEvent && event.key === "Escape";
      const outside = event instanceof PointerEvent && !element.contains(event.target as Node);
      if (!escape && !outside) return;
      // Commit a focused edit before hiding the panel.
      element.querySelector("textarea")?.blur();
      element.open = false;
      if (escape) {
        event.preventDefault();
        element.querySelector("summary")?.focus();
      }
    };
    document.addEventListener("pointerdown", close);
    document.addEventListener("keydown", close);
    return () => {
      document.removeEventListener("pointerdown", close);
      document.removeEventListener("keydown", close);
    };
  }, []);
  return (
    <details className="table-comment" ref={details}>
      <summary aria-label="テーブルコメント" title={preview}>
        <MessageSquare size={14} />
        <span>{preview}</span>
      </summary>
      <div className="table-comment-panel">
        <CommentBox
          key={`${masterId}:${comment?.body}`}
          label="Table Comment"
          comment={comment}
          masterId={masterId}
          target={{ kind: "table" }}
          editable={editable}
        />
      </div>
    </details>
  );
}

function CommentBox({
  label,
  comment,
  masterId,
  target,
  editable,
}: {
  label: string;
  comment?: Comment | null;
  masterId: string;
  target: CommentTarget;
  editable: boolean;
}) {
  const [body, setBody] = useState(comment?.body ?? "");
  const action = useRepositoryAction();
  const save = () => {
    if (body === (comment?.body ?? "") || !editable) return;
    action.mutate(
      {
        command: "edit_project",
        operation: { type: "setComment", masterId, target, body },
      },
      { onError: () => setBody(comment?.body ?? "") },
    );
  };
  const date = (value: string) => new Date(value).toLocaleString();
  return (
    <section className="comment-box">
      <label>
        <MessageSquare size={13} />
        {label}
        <textarea
          aria-label={label}
          disabled={!editable || action.isPending}
          value={body}
          onChange={(e) => setBody(e.target.value)}
          onBlur={save}
          onKeyDown={(e) => {
            if ((e.metaKey || e.ctrlKey) && e.key === "Enter") {
              e.preventDefault();
              e.currentTarget.blur();
            }
          }}
          placeholder="コメントを追加…"
          rows={target.kind === "table" ? 2 : 3}
        />
      </label>
      {comment && (
        <div className="comment-metadata">
          <div title={comment.createdBy.email}>
            <span>Created</span>
            <strong>{comment.createdBy.name}</strong>
            <time>{date(comment.createdAt)}</time>
          </div>
          <div title={comment.updatedBy.email}>
            <span>Updated</span>
            <strong>{comment.updatedBy.name}</strong>
            <time>{date(comment.updatedAt)}</time>
          </div>
          <button
            disabled={!editable || action.isPending}
            onClick={() =>
              action.mutate({
                command: "edit_project",
                operation: { type: "setComment", masterId, target, body: "" },
              })
            }
          >
            コメントを削除
          </button>
        </div>
      )}
    </section>
  );
}
