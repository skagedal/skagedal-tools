import React, { useEffect, useMemo, useState } from "react";
import { Box, Text, useApp, useInput, useStdout } from "ink";
import type { Config, FieldConfig } from "../config.js";
import type { LogEntry } from "../entry.js";
import { renderField, renderValueDetailed, stringify } from "../entry.js";
import type { SourceHandle } from "../source.js";
import { entryHaystack, fuzzyMatch, killWordLeft, stripUnprintable } from "./filter.js";

interface Props {
  config: Config;
  source: SourceHandle;
  sourceLabel: string;
}

/**
 * A column definition the user can toggle at runtime. Pre-configured fields
 * keep their friendly `name`/`from` mapping; raw keys discovered later become
 * single-key entries with `name === from[0]`.
 */
interface ManagedField {
  name: string;
  from: string[];
  visible: boolean;
}

type View = "list" | "detail" | "fieldsMenu";

export const App: React.FC<Props> = ({ config, source, sourceLabel }) => {
  const { exit } = useApp();
  const { stdout } = useStdout();
  const [entries, setEntries] = useState<LogEntry[]>([]);
  const [cursor, setCursor] = useState(0);
  const [view, setView] = useState<View>("list");
  const [done, setDone] = useState(false);
  const [flash, setFlash] = useState<string | null>(null);
  const [follow, setFollow] = useState(false);
  const [detailCursor, setDetailCursor] = useState(0);
  const [menuCursor, setMenuCursor] = useState(0);
  const [fields, setFields] = useState<ManagedField[]>(() =>
    config.fields.map((f) => ({ name: f.name, from: [...f.from], visible: true })),
  );
  const [size, setSize] = useState<{ rows: number; cols: number }>(() => ({
    rows: stdout?.rows ?? 24,
    cols: stdout?.columns ?? 100,
  }));
  // Filter state.
  // - `filterMode` is true while the text field is open for editing.
  // - `filterActive` means a filter is being applied to the visible list.
  //   It can stay true after the text field closes (Enter), and only resets
  //   when the user Escapes out of the text field.
  // - `selectedId` tracks the entry id of the selected row, so we can
  //   re-anchor the cursor when the filter changes.
  const [filterText, setFilterText] = useState("");
  const [filterCursor, setFilterCursor] = useState(0);
  const [filterMode, setFilterMode] = useState(false);
  const [filterActive, setFilterActive] = useState(false);
  const [selectedId, setSelectedId] = useState<number | null>(null);

  useEffect(() => {
    if (!stdout) return;
    const onResize = () => setSize({ rows: stdout.rows, cols: stdout.columns });
    stdout.on("resize", onResize);
    return () => {
      stdout.off("resize", onResize);
    };
  }, [stdout]);

  useEffect(() => {
    // Batch incoming entries on the event loop so a high-throughput stream
    // doesn't trigger a React render per line.
    let buffer: LogEntry[] = [];
    let flushPending = false;
    const flush = () => {
      flushPending = false;
      if (buffer.length === 0) return;
      const batch = buffer;
      buffer = [];
      setEntries((prev) => prev.concat(batch));
      setFields((prev) => {
        const keys: string[] = [];
        const seen = new Set<string>();
        for (const e of batch) {
          for (const k of Object.keys(e.data)) {
            if (seen.has(k)) continue;
            seen.add(k);
            keys.push(k);
          }
        }
        return mergeSeenKeys(prev, keys);
      });
    };
    source.onEntry((entry) => {
      buffer.push(entry);
      if (flushPending) return;
      flushPending = true;
      setImmediate(flush);
    });
    source.onEnd(() => {
      flush();
      setDone(true);
    });
    return () => source.close();
  }, [source]);

  const totalRows = size.rows;
  const visible = Math.max(1, totalRows - 4);
  const activeFields = useMemo(() => visibleFieldConfigs(fields), [fields]);

  // Displayed entries: the (possibly filtered) list the user actually sees
  // and navigates. When the filter is inactive or empty this is just `entries`.
  const displayed = useMemo(() => {
    if (!filterActive || filterText.length === 0) return entries;
    return entries.filter((e) => fuzzyMatch(filterText, entryHaystack(e, activeFields)));
  }, [entries, filterActive, filterText, activeFields]);

  // Keep cursor anchored to the same entry across filter changes / arrivals.
  // We track `selectedId`; whenever `displayed` changes shape we re-derive
  // `cursor` from it. In follow mode we ride the tail of `displayed`.
  const [prevTail, setPrevTail] = useState({
    displayed,
    follow,
    entriesLength: entries.length,
  });
  if (
    prevTail.displayed !== displayed ||
    prevTail.follow !== follow ||
    prevTail.entriesLength !== entries.length
  ) {
    setPrevTail({ displayed, follow, entriesLength: entries.length });
    if (follow && displayed.length > 0) {
      const tailIdx = displayed.length - 1;
      if (cursor !== tailIdx) setCursor(tailIdx);
      const tailId = displayed[tailIdx]!.id;
      if (selectedId !== tailId) setSelectedId(tailId);
    } else if (selectedId != null) {
      const idx = displayed.findIndex((e) => e.id === selectedId);
      if (idx >= 0) {
        if (cursor !== idx) setCursor(idx);
      } else if (displayed.length > 0) {
        const clamped = Math.max(0, Math.min(cursor, displayed.length - 1));
        if (cursor !== clamped) setCursor(clamped);
        setSelectedId(displayed[clamped]!.id);
      } else {
        setSelectedId(null);
        if (cursor !== 0) setCursor(0);
      }
    } else if (displayed[cursor]) {
      setSelectedId(displayed[cursor]!.id);
    } else if (displayed.length > 0) {
      setCursor(0);
      setSelectedId(displayed[0]!.id);
    }
  }

  // Reset detail-row cursor whenever we change entries or change view, so the
  // cursor doesn't carry over from a previous entry's field count.
  const [prevDetail, setPrevDetail] = useState({ view, cursor });
  if (view !== prevDetail.view || cursor !== prevDetail.cursor) {
    setPrevDetail({ view, cursor });
    setDetailCursor(0);
  }

  // Helper used by all list-mode navigation: move cursor and pin the
  // selectedId so it survives subsequent filter changes / new entries.
  const moveCursorTo = (idx: number) => {
    if (displayed.length === 0) {
      setCursor(0);
      setSelectedId(null);
      return;
    }
    const clamped = Math.max(0, Math.min(idx, displayed.length - 1));
    setCursor(clamped);
    setSelectedId(displayed[clamped]!.id);
  };

  useInput((input, key) => {
    // Filter editing takes precedence over everything else: while the text
    // field is open it owns every keypress except Enter/Escape (which close
    // it). Typing updates the filter live so the list re-narrows as you go.
    if (filterMode) {
      if (key.return) {
        setFilterMode(false);
        return;
      }
      if (key.escape) {
        setFilterMode(false);
        setFilterActive(false);
        return;
      }
      if (key.leftArrow || (key.ctrl && input === "b")) {
        setFilterCursor((c) => Math.max(0, c - 1));
        return;
      }
      if (key.rightArrow || (key.ctrl && input === "f")) {
        setFilterCursor((c) => Math.min(filterText.length, c + 1));
        return;
      }
      if (key.home || (key.ctrl && input === "a")) {
        setFilterCursor(0);
        return;
      }
      if (key.end || (key.ctrl && input === "e")) {
        setFilterCursor(filterText.length);
        return;
      }
      if (key.backspace) {
        if (filterCursor > 0) {
          setFilterText(filterText.slice(0, filterCursor - 1) + filterText.slice(filterCursor));
          setFilterCursor(filterCursor - 1);
        }
        return;
      }
      if (key.delete || (key.ctrl && input === "d")) {
        if (filterCursor < filterText.length) {
          setFilterText(filterText.slice(0, filterCursor) + filterText.slice(filterCursor + 1));
        }
        return;
      }
      if (key.ctrl && input === "k") {
        setFilterText(filterText.slice(0, filterCursor));
        return;
      }
      if (key.ctrl && input === "w") {
        const cut = killWordLeft(filterText, filterCursor);
        setFilterText(cut.text);
        setFilterCursor(cut.cursor);
        return;
      }
      // Other ctrl combos are swallowed so they don't sneak into the buffer.
      if (key.ctrl) return;
      // Insert printable input. Multi-char input from paste lands here too.
      if (input && input.length > 0) {
        const printable = stripUnprintable(input);
        if (printable.length === 0) return;
        setFilterText(filterText.slice(0, filterCursor) + printable + filterText.slice(filterCursor));
        setFilterCursor(filterCursor + printable.length);
      }
      return;
    }

    if (view === "fieldsMenu") {
      if (input === "q" || key.escape || input === "v") {
        setView("list");
        return;
      }
      if (input === "j" || key.downArrow) {
        setMenuCursor((c) => Math.min(fields.length - 1, c + 1));
        return;
      }
      if (input === "k" || key.upArrow || input === "u") {
        setMenuCursor((c) => Math.max(0, c - 1));
        return;
      }
      if (input === " " || input === "t") {
        setFields((prev) => toggleAt(prev, menuCursor));
        return;
      }
      if (input === "J") {
        setFields((prev) => moveAt(prev, menuCursor, +1));
        setMenuCursor((c) => Math.min(fields.length - 1, c + 1));
        return;
      }
      if (input === "K") {
        setFields((prev) => moveAt(prev, menuCursor, -1));
        setMenuCursor((c) => Math.max(0, c - 1));
        return;
      }
      return;
    }

    if (view === "detail") {
      if (input === "q" || key.escape || input === "u") {
        setView("list");
        return;
      }
      const entry = displayed[cursor];
      const keys = entry ? Object.keys(entry.data) : [];
      // detailCursor 0 = entry header (whole entry); 1..keys.length = fields
      if (input === "j" || key.downArrow) {
        setDetailCursor((c) => Math.min(keys.length, c + 1));
        return;
      }
      if (input === "k" || key.upArrow) {
        setDetailCursor((c) => Math.max(0, c - 1));
        return;
      }
      if (input === "n") {
        moveCursorTo(cursor + 1);
        return;
      }
      if (input === "p") {
        moveCursorTo(cursor - 1);
        return;
      }
      if (input === "v") {
        setView("fieldsMenu");
        return;
      }
      if (input === "t") {
        if (detailCursor === 0 || !entry) return;
        const key = keys[detailCursor - 1]!;
        setFields((prev) => toggleByKey(prev, key));
        const newState = toggleByKey(fields, key);
        const fc = newState.find((f) => f.from[0] === key || f.from.includes(key));
        flashFor(setFlash, `${fc?.visible ? "showing" : "hiding"} "${fc?.name ?? key}"`);
        return;
      }
      if (input === "c") {
        if (!entry) return;
        if (detailCursor === 0) {
          copyToClipboard(safeJson(entry.data), `entry #${entry.id} JSON`, setFlash);
        } else {
          const key = keys[detailCursor - 1]!;
          const value = stringify(entry.data[key]);
          copyToClipboard(value, `${key}`, setFlash);
        }
        return;
      }
      return;
    }

    if (input === "q" || key.escape) {
      exit();
      return;
    }
    if (input === "/") {
      setFilterMode(true);
      setFilterActive(true);
      setFilterCursor(filterText.length);
      setFollow(false);
      return;
    }
    if (input === "v") {
      setMenuCursor(0);
      setView("fieldsMenu");
      return;
    }
    if (input === "f") {
      setFollow((v) => !v);
      return;
    }
    if (input === "j" || key.downArrow) {
      setFollow(false);
      moveCursorTo(cursor + 1);
      return;
    }
    if (input === "k" || key.upArrow) {
      setFollow(false);
      moveCursorTo(cursor - 1);
      return;
    }
    if (input === "u") {
      setFollow(false);
      moveCursorTo(cursor - 1);
      return;
    }
    if (key.pageDown) {
      setFollow(false);
      moveCursorTo(cursor + visible);
      return;
    }
    if (key.pageUp) {
      setFollow(false);
      moveCursorTo(cursor - visible);
      return;
    }
    if (input === "o" || key.return) {
      if (displayed.length > 0) {
        setFollow(false);
        setView("detail");
      }
      return;
    }
    if (input === "g") {
      setFollow(false);
      moveCursorTo(0);
      return;
    }
    if (input === "G") {
      setFollow(false);
      moveCursorTo(displayed.length - 1);
      return;
    }
  });

  const widths = useMemo(
    () => columnWidths(activeFields, entries, size.cols),
    [activeFields, entries, size.cols],
  );
  const start = Math.max(0, Math.min(cursor - Math.floor(visible / 2), displayed.length - visible));
  const window = displayed.slice(start, start + visible);

  if (view === "fieldsMenu") {
    return (
      <FieldsMenu
        fields={fields}
        cursor={menuCursor}
        flash={flash}
      />
    );
  }

  if (view === "detail" && displayed[cursor]) {
    return (
      <Detail
        entry={displayed[cursor]}
        sourceLabel={sourceLabel}
        position={{ index: cursor, total: displayed.length }}
        fields={fields}
        cursor={detailCursor}
        flash={flash}
        width={size.cols}
      />
    );
  }

  return (
    <Box flexDirection="column" height={totalRows} width={size.cols}>
      <Header
        fields={activeFields}
        widths={widths}
        sourceLabel={sourceLabel}
        count={entries.length}
        filteredCount={filterActive && filterText.length > 0 ? displayed.length : null}
        done={done}
      />
      <Box flexDirection="column" flexGrow={1}>
        {window.length === 0 ? (
          <Text dimColor>
            {entries.length === 0
              ? "(waiting for log lines…)"
              : "(no entries match filter)"}
          </Text>
        ) : (
          window.map((entry, i) => {
            const idx = start + i;
            const selected = !follow && idx === cursor;
            const tail = follow && idx === displayed.length - 1;
            return (
              <Row
                key={entry.id}
                entry={entry}
                fields={activeFields}
                widths={widths}
                selected={selected}
                tail={tail}
              />
            );
          })
        )}
      </Box>
      <StatusBar
        follow={follow}
        cursor={cursor}
        total={displayed.length}
        filterMode={filterMode}
        filterActive={filterActive}
        filterText={filterText}
        filterCursor={filterCursor}
      />
    </Box>
  );
};

const Header: React.FC<{
  fields: FieldConfig[];
  widths: number[];
  sourceLabel: string;
  count: number;
  filteredCount: number | null;
  done: boolean;
}> = ({ fields, widths, sourceLabel, count, filteredCount, done }) => (
  <Box flexDirection="column">
    <Text>
      <Text bold>log-viewer</Text>
      <Text dimColor>
        {" "}· {sourceLabel} · {count} entries{done ? " (eof)" : ""}
        {filteredCount !== null ? ` · ${filteredCount} match` : ""}
      </Text>
    </Text>
    <Box>
      <Text> </Text>
      {fields.map((field, i) => (
        <Box key={field.name} width={widths[i]} marginRight={1}>
          <Text bold underline>{truncate(field.name, widths[i] - 1)}</Text>
        </Box>
      ))}
    </Box>
  </Box>
);

const Row: React.FC<{
  entry: LogEntry;
  fields: FieldConfig[];
  widths: number[];
  selected: boolean;
  tail: boolean;
}> = ({ entry, fields, widths, selected, tail }) => {
  const levelField = fields.find((f) => f.name.toLowerCase() === "level");
  const levelValue = levelField ? renderField(entry, levelField) : "";
  const color = selected ? undefined : levelColor(levelValue);
  return (
    <Box>
      <Text color={tail ? "green" : undefined}>{tail ? "▌" : " "}</Text>
      {fields.map((field, i) => (
        <Box key={field.name} width={widths[i]} marginRight={1}>
          <Text inverse={selected} color={color}>
            {truncate(renderField(entry, field), Math.max(1, widths[i] - 1))}
          </Text>
        </Box>
      ))}
    </Box>
  );
};

const Detail: React.FC<{
  entry: LogEntry;
  sourceLabel: string;
  position: { index: number; total: number };
  fields: ManagedField[];
  cursor: number;
  flash: string | null;
  width: number;
}> = ({ entry, sourceLabel, position, fields, cursor, flash, width }) => {
  const keys = Object.keys(entry.data);
  const headerSelected = cursor === 0;
  const keyColumnWidth = Math.max(
    8,
    Math.min(24, keys.reduce((w, k) => Math.max(w, k.length), 0) + 1),
  );
  return (
    <Box flexDirection="column">
      <Text inverse={headerSelected}>
        <Text bold>entry #{entry.id}</Text>
        <Text dimColor={!headerSelected}>
          {" "}
          · {position.index + 1}/{position.total} · {sourceLabel}
          {entry.wrapped ? " · wrapped" : ""}
        </Text>
      </Text>
      <Box marginTop={1} flexDirection="column">
        {keys.length === 0 ? (
          <Text dimColor>(empty)</Text>
        ) : (
          keys.map((key, i) => {
            const selected = cursor === i + 1;
            const visible = isKeyVisible(fields, key);
            return (
              <FieldRow
                key={key}
                fieldName={key}
                value={entry.data[key]}
                visible={visible}
                selected={selected}
                keyColumnWidth={keyColumnWidth}
                width={width}
              />
            );
          })
        )}
      </Box>
      <Box marginTop={1}>
        <Text dimColor>
          j/k field · n/p next/prev entry · u/esc back · t toggle visibility · c copy · v fields menu
          {flash ? `  ·  ${flash}` : ""}
        </Text>
      </Box>
    </Box>
  );
};

const FieldRow: React.FC<{
  fieldName: string;
  value: unknown;
  visible: boolean;
  selected: boolean;
  keyColumnWidth: number;
  width: number;
}> = ({ fieldName, value, visible, selected, keyColumnWidth, width }) => {
  const rendered = renderValueDetailed(value);
  const lines = rendered.split("\n");
  const valueWidth = Math.max(10, width - keyColumnWidth - 4);
  return (
    <Box>
      <Box width={2}><Text>{visible ? "•" : " "}</Text></Box>
      <Box width={keyColumnWidth} marginRight={1}>
        <Text inverse={selected} bold dimColor={!selected && !visible}>
          {truncate(fieldName, keyColumnWidth - 1)}
        </Text>
      </Box>
      <Box flexDirection="column">
        {lines.map((line, i) => (
          <Text key={i} inverse={selected} dimColor={!selected && !visible}>
            {truncate(line, valueWidth)}
          </Text>
        ))}
      </Box>
    </Box>
  );
};

const FieldsMenu: React.FC<{
  fields: ManagedField[];
  cursor: number;
  flash: string | null;
}> = ({ fields, cursor, flash }) => {
  const nameWidth = Math.max(
    8,
    fields.reduce((w, f) => Math.max(w, f.name.length), 0) + 1,
  );
  return (
    <Box flexDirection="column">
      <Text>
        <Text bold>Fields</Text>
        <Text dimColor> · select which columns appear and in what order</Text>
      </Text>
      <Box marginTop={1} flexDirection="column">
        {fields.length === 0 ? (
          <Text dimColor>(no fields seen yet)</Text>
        ) : (
          fields.map((f, i) => {
            const selected = i === cursor;
            const mark = f.visible ? "[x]" : "[ ]";
            const fromHint = f.from.length > 1 || f.from[0] !== f.name
              ? ` (from: ${f.from.join(", ")})`
              : "";
            return (
              <Text key={f.name} inverse={selected} dimColor={!selected && !f.visible}>
                {" "}{mark} {pad(f.name, nameWidth)}{fromHint}
              </Text>
            );
          })
        )}
      </Box>
      <Box marginTop={1}>
        <Text dimColor>
          j/k (or u) move · space/t toggle · J/K reorder · v/q/esc back
          {flash ? `  ·  ${flash}` : ""}
        </Text>
      </Box>
    </Box>
  );
};

const StatusBar: React.FC<{
  follow: boolean;
  cursor: number;
  total: number;
  filterMode: boolean;
  filterActive: boolean;
  filterText: string;
  filterCursor: number;
}> = ({ follow, cursor, total, filterMode, filterActive, filterText, filterCursor }) => {
  if (filterMode) {
    return (
      <Box>
        <Text>/</Text>
        <FilterField text={filterText} cursor={filterCursor} />
        <Text dimColor>  enter: keep filter · esc: clear filter</Text>
      </Box>
    );
  }
  const pos = total === 0 ? 0 : cursor + 1;
  const filterTag = filterActive && filterText.length > 0
    ? <Text color="yellow"> [/{filterText}]</Text>
    : null;
  if (follow) {
    return (
      <Box>
        <Text>
          <Text color="green" bold inverse>{" FOLLOW "}</Text>
          <Text dimColor>
            {" "}{pos}/{total} · any nav key exits · j/k move · u up · o open · v fields ·
            g/G top/bottom · / filter · q quit
          </Text>
          {filterTag}
        </Text>
      </Box>
    );
  }
  return (
    <Box>
      <Text dimColor>
        {pos}/{total} · j/k move · PgUp/PgDn page · u up · o open · f follow · v fields ·
        g/G top/bottom · / filter · q quit
      </Text>
      {filterTag}
    </Box>
  );
};

const FilterField: React.FC<{ text: string; cursor: number }> = ({ text, cursor }) => {
  // Render a fake block cursor by inverting the character under it. If the
  // cursor is at end-of-text we draw an inverted space.
  const before = text.slice(0, cursor);
  const at = cursor < text.length ? text[cursor]! : " ";
  const after = cursor < text.length ? text.slice(cursor + 1) : "";
  return (
    <Text>
      <Text>{before}</Text>
      <Text inverse>{at}</Text>
      <Text>{after}</Text>
    </Text>
  );
};

function visibleFieldConfigs(fields: ManagedField[]): FieldConfig[] {
  return fields.filter((f) => f.visible).map((f) => ({ name: f.name, from: f.from }));
}

function mergeSeenKeys(prev: ManagedField[], keys: string[]): ManagedField[] {
  const knownFroms = new Set<string>();
  for (const f of prev) for (const k of f.from) knownFroms.add(k);
  let next: ManagedField[] | null = null;
  for (const key of keys) {
    if (knownFroms.has(key)) continue;
    knownFroms.add(key);
    if (!next) next = [...prev];
    next.push({ name: key, from: [key], visible: false });
  }
  return next ?? prev;
}

function toggleAt(fields: ManagedField[], index: number): ManagedField[] {
  if (index < 0 || index >= fields.length) return fields;
  return fields.map((f, i) => (i === index ? { ...f, visible: !f.visible } : f));
}

function moveAt(fields: ManagedField[], index: number, delta: number): ManagedField[] {
  const target = index + delta;
  if (index < 0 || index >= fields.length || target < 0 || target >= fields.length) {
    return fields;
  }
  const next = [...fields];
  const [item] = next.splice(index, 1);
  next.splice(target, 0, item!);
  return next;
}

function toggleByKey(fields: ManagedField[], key: string): ManagedField[] {
  const idx = fields.findIndex((f) => f.from.includes(key));
  if (idx === -1) {
    return [...fields, { name: key, from: [key], visible: true }];
  }
  return toggleAt(fields, idx);
}

function isKeyVisible(fields: ManagedField[], key: string): boolean {
  const f = fields.find((f) => f.from.includes(key));
  return f ? f.visible : false;
}

async function copyToClipboard(
  text: string,
  label: string,
  setFlash: (v: string | null) => void,
): Promise<void> {
  try {
    const { default: clipboard } = await import("clipboardy");
    clipboard.writeSync(text);
    flashFor(setFlash, `copied ${label} (${text.length} bytes)`);
  } catch (err) {
    flashFor(setFlash, `copy failed: ${err instanceof Error ? err.message : String(err)}`);
  }
}

function safeJson(data: unknown): string {
  try {
    return JSON.stringify(data, null, 2);
  } catch {
    return String(data);
  }
}

function flashFor(
  setFlash: (v: string | null) => void,
  message: string,
  ms = 1500,
): void {
  setFlash(message);
  setTimeout(() => setFlash(null), ms);
}

function columnWidths(
  fields: FieldConfig[],
  entries: LogEntry[],
  totalColumns: number,
): number[] {
  const n = fields.length;
  if (n === 0) return [];
  const cap = Math.max(20, Math.floor(totalColumns / 3));
  // Only sample the most recent entries — scanning all of them turns each
  // render into O(N×F) work, which collapses on high-throughput streams.
  const sampleLimit = 200;
  const sampleStart = Math.max(0, entries.length - sampleLimit);
  const widths: number[] = [];
  for (let i = 0; i < n - 1; i++) {
    const field = fields[i]!;
    let w = field.name.length;
    for (let j = sampleStart; j < entries.length; j++) {
      const v = renderField(entries[j]!, field);
      if (v.length > w) w = v.length;
      if (w >= cap) {
        w = cap;
        break;
      }
    }
    widths.push(w + 1);
  }
  const fixed = widths.reduce((a, b) => a + b, 0);
  const last = Math.max(20, totalColumns - fixed - n);
  widths.push(last);
  return widths;
}

function truncate(value: string, width: number): string {
  const clean = value.replace(/\s+/g, " ");
  if (clean.length <= width) return clean;
  return clean.slice(0, Math.max(0, width - 1)) + "…";
}

function pad(value: string, width: number): string {
  if (value.length >= width) return value;
  return value + " ".repeat(width - value.length);
}

function levelColor(level: string): string | undefined {
  const v = level.toLowerCase();
  if (v.includes("error") || v === "err" || v === "fatal") return "red";
  if (v.includes("warn")) return "yellow";
  if (v.includes("info")) return "cyan";
  if (v.includes("debug") || v.includes("trace")) return "gray";
  return undefined;
}
