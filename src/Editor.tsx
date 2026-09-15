import { useEffect, useMemo, useRef, useState } from "react";
import { AgGridReact } from "ag-grid-react";
import {
  type ColDef,
  type GetContextMenuItems,
  type GridApi,
  type ProcessDataFromClipboardParams,
} from "ag-grid-community";
import {
  Columns3,
  Copy,
  Eraser,
  KeyRound,
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
import { useBusy, useRepositoryAction } from "./api";
import { useUI } from "./store";
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

type GridRow = { key: PrimaryKey; values: string[] };
// Keep arbitrary user column names separate from AG Grid's internal column IDs.
const columnName = (id: string) => (id.startsWith("data:") ? id.slice(5) : "");

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
  const grid = useRef<AgGridReact<GridRow>>(null);
  const def = project.data.config.masters[masterId];
  const editable = !!project.identity.name && !!project.identity.email && !project.git.protected && !project.git.mergeInProgress && !busy;
  const rowData = useMemo(
    () =>
      master.table.rows.map((values) => ({
        key: rowKey(values, master, def),
        values,
      })),
    [master.table, def],
  );
  const edit = (edits: CellEdit[]) => {
    if (!editable || !edits.length) return;
    if (edits.some((e) => def.primaryKey.includes(e.column))) {
      ui.set({
        error:
          "Primary Key を含む操作は、全体を適用できません。選択範囲を変更してください。",
      });
      return;
    }
    action.mutate({
      command: "edit_project",
      operation: { type: "editCells", masterId, edits },
    });
  };
  const selectionEdits = (value: string, copyTopRow = false): CellEdit[] => {
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
  };
  const columnDefs = useMemo<ColDef<GridRow>[]>(() => {
    const order = [
      ...def.primaryKey,
      ...master.table.columns.filter((c) => !def.primaryKey.includes(c)),
    ];
    return order.map((column) => {
      const pk = def.primaryKey.includes(column);
      const index = master.table.columns.indexOf(column);
      return {
        ...masterColumn<GridRow>(column, def.primaryKey),
        valueGetter: (p) => p.data?.values[index] ?? "",
        valueParser: (p) => String(p.newValue ?? ""),
        editable: () => editable && !pk,
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
    // selection, retain all selected rows for bulk deletion.
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
        action: () => ui.set({ dialog: "addRow" }),
      },
      {
        name: "この Row を複製",
        disabled: !editable || !row,
        action: () => {
          if (!node || !row) return;
          node.setSelected(true, true);
          ui.set({ selectedKeys: [row.key], cell, dialog: "duplicateRow" });
        },
      },
      {
        name: selectedKeys.length > 1
          ? `選択した ${selectedKeys.length} 件の Row を削除`
          : "この Row を削除",
        disabled: !editable || !row,
        action: () => {
          if (!row) return;
          ui.set({ selectedKeys, cell, dialog: "deleteRows" });
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
        <div className="editor-title">
          <h1>{masterId}</h1>
          <span className="badge">MASTER</span>
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
            onClick={() => ui.set({ dialog: "addRow" })}
          >
            <Plus size={16} />
            Row 追加
          </button>
          <button
            disabled={!editable || ui.selectedKeys.length !== 1}
            title="選択した Row を複製"
            aria-label="Row を複製"
            onClick={() => ui.set({ dialog: "duplicateRow" })}
          >
            <Copy size={16} />
          </button>
          <button
            disabled={!editable || !ui.selectedKeys.length}
            title="選択した Row を削除"
            aria-label="Row を削除"
            onClick={() => ui.set({ dialog: "deleteRows" })}
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
      <div className="editor-content">
        <div className="grid-panel">
          <div className="table-comment">
            <CommentBox
              key={`table:${masterId}:${master.comments.table?.body}`}
              label="Table Comment"
              comment={master.comments.table}
              masterId={masterId}
              target={{ kind: "table" }}
              editable={editable}
            />
          </div>
          <div className="grid-hint">
            <span>
              <KeyRound size={13} />
              Primary Key は固定・編集不可
            </span>
            <div>
              <button
                disabled={!editable || !rangeCount}
                onClick={() => edit(selectionEdits(""))}
              >
                <Eraser size={13} />
                空文字にする
              </button>
              <button
                disabled={!editable || !rangeCount}
                onClick={() => setFillValue("")}
              >
                選択範囲を埋める
              </button>
            </div>
          </div>
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
              ref={grid}
              theme={masterGridTheme}
              rowData={rowData}
              columnDefs={columnDefs}
              defaultColDef={{
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
              }}
              readOnlyEdit
              stopEditingWhenCellsLoseFocus
              suppressCutToClipboard
              getContextMenuItems={contextMenuItems}
              allowContextMenuWithControlKey
              cellSelection={{ suppressMultiRanges: false }}
              rowSelection={{ mode: "multiRow", enableClickSelection: false }}
              selectionColumnDef={{
                pinned: "left",
                width: 44,
                maxWidth: 44,
                resizable: false,
                suppressHeaderMenuButton: true,
              }}
              getRowId={(p) => keyId(p.data.key)}
              quickFilterText={ui.search}
              processDataFromClipboard={paste}
              onGridReady={(e) => setApi(e.api)}
              onModelUpdated={(e) =>
                setVisibleRows(e.api.getDisplayedRowCount())
              }
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
              onCellEditRequest={(e) =>
                edit([
                  {
                    primaryKey: e.data.key,
                    column: columnName(e.column.getColId()),
                    value: String(e.newValue ?? ""),
                  },
                ])
              }
              overlayNoRowsTemplate="<span>Row がありません。「Row 追加」から作成できます。</span>"
            />
          </div>
          <div className="grid-footer">
            <span>
              {visibleRows.toLocaleString()} /{" "}
              {master.table.rows.length.toLocaleString()} rows
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
  editable,
}: {
  masterId: string;
  master: Master;
  def: Definition;
  editable: boolean;
}) {
  const ui = useUI();
  const key =
    ui.cell?.primaryKey ??
    (ui.selectedKeys.length === 1 ? ui.selectedKeys[0] : null);
  const row = key
    ? master.table.rows.find(
        (r) => keyId(rowKey(r, master, def)) === keyId(key),
      )
    : null;
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
                <code>{key[i]}</code>
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
      {key && row && (
        <CommentBox
          key={`row:${keyId(key)}:${rowComment?.body}`}
          label="Row Comment"
          comment={rowComment}
          masterId={masterId}
          target={{ kind: "row", primaryKey: key }}
          editable={editable}
        />
      )}
      {key && row && column && (
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
          placeholder={target.kind === "table"
            ? "テーブルの説明を追加…（入力欄を離れると自動保存）"
            : "コメントを追加…"}
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
