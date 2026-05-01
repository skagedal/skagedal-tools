import { useEffect, useLayoutEffect, useMemo, useRef } from "react";
import {
  createColumnHelper,
  flexRender,
  getCoreRowModel,
  useReactTable,
  type CellContext,
} from "@tanstack/react-table";
import { useVirtualizer } from "@tanstack/react-virtual";
import type { LogEntry, ManagedField } from "./types";
import {
  classifyLevel,
  copyText,
  isKeyVisible,
  pickField,
  renderValueDetailed,
  stringifyValue,
} from "./util";

interface Props {
  entries: LogEntry[];
  fields: ManagedField[];
  cursor: number;
  follow: boolean;
  expanded: Set<number>;
  detailCursor: number;
  onSelect: (entryIdx: number) => void;
  onToggleExpand: (entryId: number) => void;
  onSelectDetailRow: (entryIdx: number, detailIdx: number) => void;
  onToggleByKey: (key: string) => void;
  onFlash: (msg: string) => void;
}

type Item =
  | { kind: "row"; entryIdx: number }
  | { kind: "detail"; entryIdx: number };

const columnHelper = createColumnHelper<LogEntry>();

export function LogTable(props: Props) {
  const {
    entries,
    fields,
    cursor,
    follow,
    expanded,
    detailCursor,
    onSelect,
    onToggleExpand,
    onSelectDetailRow,
    onToggleByKey,
    onFlash,
  } = props;

  const parentRef = useRef<HTMLDivElement>(null);

  const visibleFields = useMemo(() => fields.filter((f) => f.visible), [fields]);

  const columns = useMemo(
    () =>
      visibleFields.map((field) =>
        columnHelper.accessor((row: LogEntry) => pickField(row.data, field.from), {
          id: field.name,
          header: () => field.name,
          cell: (info: CellContext<LogEntry, string>) => info.getValue(),
        }),
      ),
    [visibleFields],
  );

  // TanStack Table's useReactTable returns functions that the React Compiler
  // can't memoize safely. Nothing actionable here until the library updates.
  // See https://github.com/TanStack/table/issues/6137
  // eslint-disable-next-line react-hooks/incompatible-library
  const table = useReactTable({
    data: entries,
    columns,
    getCoreRowModel: getCoreRowModel(),
    getRowId: (row) => String(row.id),
  });

  const tableRows = table.getRowModel().rows;

  const items = useMemo<Item[]>(() => {
    const out: Item[] = [];
    for (let i = 0; i < entries.length; i++) {
      out.push({ kind: "row", entryIdx: i });
      if (expanded.has(entries[i]!.id)) {
        out.push({ kind: "detail", entryIdx: i });
      }
    }
    return out;
  }, [entries, expanded]);

  const entryToItem = useMemo(() => {
    const map = new Array<number>(entries.length);
    let item = 0;
    for (let i = 0; i < entries.length; i++) {
      map[i] = item;
      item += 1;
      if (expanded.has(entries[i]!.id)) item += 1;
    }
    return map;
  }, [entries, expanded]);

  const virtualizer = useVirtualizer({
    count: items.length,
    getScrollElement: () => parentRef.current,
    estimateSize: (i) => (items[i]?.kind === "detail" ? 220 : 24),
    overscan: 24,
    // Key by entry id + kind so measured sizes follow their item when the
    // items array shifts (e.g. collapsing a detail row removes one item).
    // Without this the cache is keyed by index and leaves a phantom gap.
    getItemKey: (index) => {
      const item = items[index];
      if (!item) return index;
      return `${item.kind}-${entries[item.entryIdx]!.id}`;
    },
  });

  // Re-measure when columns change (their widths affect text wrapping → row heights).
  useEffect(() => {
    virtualizer.measure();
  }, [visibleFields.length, virtualizer]);

  // Keep the selected row scrolled into view. In follow mode we always pin to
  // the tail; otherwise scroll when the cursor moves.
  useLayoutEffect(() => {
    if (entries.length === 0) return;
    const target = follow ? entries.length - 1 : cursor;
    const itemIdx = entryToItem[target];
    if (itemIdx == null) return;
    virtualizer.scrollToIndex(itemIdx, { align: "auto" });
  }, [cursor, follow, entryToItem, virtualizer, entries.length]);

  const colCount = Math.max(1, visibleFields.length);
  const headerGroups = table.getHeaderGroups();

  return (
    <div ref={parentRef} className="scroll">
      <div className="header-row">
        {visibleFields.length === 0 ? (
          <div className="cell col-flex">(no visible fields — open the Fields menu)</div>
        ) : (
          headerGroups.map((hg) =>
            hg.headers.map((header, i) => (
              <div
                key={header.id}
                className={`cell ${i === hg.headers.length - 1 ? "col-flex" : "col-fit"}`}
              >
                {flexRender(header.column.columnDef.header, header.getContext())}
              </div>
            )),
          )
        )}
      </div>
      <div className="virtual-track" style={{ height: virtualizer.getTotalSize() }}>
        {virtualizer.getVirtualItems().map((vrow) => {
          const item = items[vrow.index];
          if (!item) return null;
          const entry = entries[item.entryIdx]!;
          const tableRow = tableRows[item.entryIdx];
          const isSelected = item.entryIdx === cursor && !follow;
          if (item.kind === "row") {
            return (
              <div
                key={`row-${entry.id}`}
                ref={virtualizer.measureElement}
                data-index={vrow.index}
                className={`row${isSelected ? " selected" : ""}`}
                style={{ transform: `translateY(${vrow.start}px)` }}
                onClick={() => {
                  onSelect(item.entryIdx);
                  onToggleExpand(entry.id);
                }}
              >
                {visibleFields.length === 0 || !tableRow ? (
                  <div className="cell col-flex">{visibleFields.length === 0 ? "—" : ""}</div>
                ) : (
                  tableRow.getVisibleCells().map((cell, i) => {
                    const value = cell.getValue<string>();
                    const fit = i === visibleFields.length - 1 ? "col-flex" : "col-fit";
                    const lvl =
                      cell.column.id.toLowerCase() === "level"
                        ? `level-${classifyLevel(value)}`
                        : "";
                    return (
                      <div key={cell.id} className={`cell ${fit} ${lvl}`.trim()}>
                        {flexRender(cell.column.columnDef.cell, cell.getContext())}
                      </div>
                    );
                  })
                )}
              </div>
            );
          }
          return (
            <div
              key={`detail-${entry.id}`}
              ref={virtualizer.measureElement}
              data-index={vrow.index}
              className="row detail-row"
              style={{ transform: `translateY(${vrow.start}px)` }}
            >
              <div className="detail-cell" style={{ gridColumn: `span ${colCount}` }}>
                <DetailContent
                  entry={entry}
                  fields={fields}
                  parentSelected={item.entryIdx === cursor && !follow}
                  detailCursor={detailCursor}
                  onSelectDetailRow={(detailIdx) => onSelectDetailRow(item.entryIdx, detailIdx)}
                  onToggleByKey={onToggleByKey}
                  onFlash={onFlash}
                />
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}

function DetailContent({
  entry,
  fields,
  parentSelected,
  detailCursor,
  onSelectDetailRow,
  onToggleByKey,
  onFlash,
}: {
  entry: LogEntry;
  fields: ManagedField[];
  parentSelected: boolean;
  detailCursor: number;
  onSelectDetailRow: (detailIdx: number) => void;
  onToggleByKey: (key: string) => void;
  onFlash: (msg: string) => void;
}) {
  const keys = Object.keys(entry.data);
  const headSelected = parentSelected && detailCursor === 0;

  const handleCopy = (text: string, label: string) => {
    copyText(text).then((ok) => onFlash(ok ? `copied ${label}` : "copy failed"));
  };

  return (
    <div className="detail">
      <div
        className={`detail-head${headSelected ? " detail-selected" : ""}`}
        onClick={() => onSelectDetailRow(0)}
      >
        <strong>
          entry #{entry.id}
          {entry.wrapped ? " (wrapped)" : ""}
        </strong>
        <button
          type="button"
          onClick={(ev) => {
            ev.stopPropagation();
            handleCopy(JSON.stringify(entry.data, null, 2), `entry #${entry.id}`);
          }}
        >
          Copy JSON
        </button>
      </div>
      {keys.length === 0 ? (
        <div className="detail-empty">(empty)</div>
      ) : (
        <dl className="detail-fields">
          {keys.map((key, idx) => {
            const visible = isKeyVisible(fields, key);
            const selected = parentSelected && detailCursor === idx + 1;
            return (
              <div
                key={key}
                className={[
                  "detail-field",
                  !visible && "hidden-field",
                  selected && "detail-selected",
                ]
                  .filter(Boolean)
                  .join(" ")}
                onClick={() => onSelectDetailRow(idx + 1)}
              >
                <input
                  type="checkbox"
                  checked={visible}
                  title={visible ? "Hide this column" : "Show this column"}
                  onClick={(ev) => ev.stopPropagation()}
                  onChange={() => onToggleByKey(key)}
                />
                <dt>{key}</dt>
                <dd>{renderValueDetailed(entry.data[key])}</dd>
                <button
                  type="button"
                  onClick={(ev) => {
                    ev.stopPropagation();
                    handleCopy(stringifyValue(entry.data[key]), key);
                  }}
                >
                  Copy
                </button>
              </div>
            );
          })}
        </dl>
      )}
    </div>
  );
}
