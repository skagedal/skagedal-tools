import React, { useEffect, useMemo, useState } from "react";
import { Box, Text, useApp, useInput, useStdout } from "ink";
import clipboard from "clipboardy";
import type { Config, FieldConfig } from "../config.js";
import type { LogEntry } from "../entry.js";
import { renderField, renderValueDetailed, stringify } from "../entry.js";
import type { SourceHandle } from "../source.js";

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

  // Keep cursor in bounds as entries arrive; in follow mode, ride the tail.
  useEffect(() => {
    if (entries.length === 0) return;
    if (follow) {
      setCursor(entries.length - 1);
    } else if (cursor >= entries.length) {
      setCursor(entries.length - 1);
    }
  }, [entries.length, cursor, follow]);

  // Reset detail-row cursor whenever we change entries or change view, so the
  // cursor doesn't carry over from a previous entry's field count.
  useEffect(() => {
    setDetailCursor(0);
  }, [view, cursor]);

  const totalRows = size.rows;
  const visible = Math.max(1, totalRows - 4);
  const activeFields = useMemo(() => visibleFieldConfigs(fields), [fields]);

  useInput((input, key) => {
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
      const entry = entries[cursor];
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
        setCursor((c) => Math.min(entries.length - 1, c + 1));
        return;
      }
      if (input === "p") {
        setCursor((c) => Math.max(0, c - 1));
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
      setCursor((c) => Math.min(entries.length - 1, c + 1));
      return;
    }
    if (input === "k" || key.upArrow) {
      setFollow(false);
      setCursor((c) => Math.max(0, c - 1));
      return;
    }
    if (input === "u") {
      setFollow(false);
      setCursor((c) => Math.max(0, c - 1));
      return;
    }
    if (input === "o" || key.return) {
      if (entries.length > 0) {
        setFollow(false);
        setView("detail");
      }
      return;
    }
    if (input === "g") {
      setFollow(false);
      setCursor(0);
      return;
    }
    if (input === "G") {
      setFollow(false);
      setCursor(Math.max(0, entries.length - 1));
      return;
    }
  });

  const widths = useMemo(
    () => columnWidths(activeFields, entries, size.cols),
    [activeFields, entries.length, size.cols],
  );
  const start = Math.max(0, Math.min(cursor - Math.floor(visible / 2), entries.length - visible));
  const window = entries.slice(start, start + visible);

  if (view === "fieldsMenu") {
    return (
      <FieldsMenu
        fields={fields}
        cursor={menuCursor}
        flash={flash}
      />
    );
  }

  if (view === "detail" && entries[cursor]) {
    return (
      <Detail
        entry={entries[cursor]}
        sourceLabel={sourceLabel}
        position={{ index: cursor, total: entries.length }}
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
        done={done}
      />
      <Box flexDirection="column" flexGrow={1}>
        {window.length === 0 ? (
          <Text dimColor>(waiting for log lines…)</Text>
        ) : (
          window.map((entry, i) => {
            const idx = start + i;
            const selected = !follow && idx === cursor;
            const tail = follow && idx === entries.length - 1;
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
      <Box>
        {follow ? (
          <Text>
            <Text color="green" bold inverse>{" FOLLOW "}</Text>
            <Text dimColor>
              {" "}
              {entries.length === 0 ? 0 : cursor + 1}/{entries.length} · any nav key exits · j/k
              move · u up · o open · v fields · g/G top/bottom · q quit
            </Text>
          </Text>
        ) : (
          <Text dimColor>
            {entries.length === 0 ? 0 : cursor + 1}/{entries.length} · j/k move · u up · o open · f
            follow · v fields · g/G top/bottom · q quit
          </Text>
        )}
      </Box>
    </Box>
  );
};

const Header: React.FC<{
  fields: FieldConfig[];
  widths: number[];
  sourceLabel: string;
  count: number;
  done: boolean;
}> = ({ fields, widths, sourceLabel, count, done }) => (
  <Box flexDirection="column">
    <Text>
      <Text bold>log-viewer</Text>
      <Text dimColor>
        {" "}· {sourceLabel} · {count} entries{done ? " (eof)" : ""}
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

function copyToClipboard(text: string, label: string, setFlash: (v: string | null) => void): void {
  try {
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
