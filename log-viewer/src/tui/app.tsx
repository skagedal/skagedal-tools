import React, { useEffect, useMemo, useState } from "react";
import { Box, Text, useApp, useInput, useStdout } from "ink";
import clipboard from "clipboardy";
import type { Config } from "../config.js";
import type { LogEntry } from "../entry.js";
import { renderColumns } from "../entry.js";
import type { SourceHandle } from "../source.js";

interface Props {
  config: Config;
  source: SourceHandle;
  sourceLabel: string;
}

export const App: React.FC<Props> = ({ config, source, sourceLabel }) => {
  const { exit } = useApp();
  const { stdout } = useStdout();
  const [entries, setEntries] = useState<LogEntry[]>([]);
  const [cursor, setCursor] = useState(0);
  const [view, setView] = useState<"list" | "detail">("list");
  const [done, setDone] = useState(false);
  const [flash, setFlash] = useState<string | null>(null);
  const [follow, setFollow] = useState(false);

  useEffect(() => {
    source.onEntry((entry) => {
      setEntries((prev) => [...prev, entry]);
    });
    source.onEnd(() => setDone(true));
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

  // Reserve rows for the header (2), the status footer (1), and a margin (1).
  const totalRows = stdout?.rows ?? 24;
  const visible = Math.max(1, totalRows - 4);

  useInput((input, key) => {
    if (view === "detail") {
      if (input === "q" || key.escape || input === "u") {
        setView("list");
        return;
      }
      if (input === "j" || key.downArrow) {
        setCursor((c) => Math.min(entries.length - 1, c + 1));
        return;
      }
      if (input === "k" || key.upArrow) {
        setCursor((c) => Math.max(0, c - 1));
        return;
      }
      if (input === "c") {
        const entry = entries[cursor];
        if (!entry) return;
        const text = safeJson(entry.data);
        try {
          clipboard.writeSync(text);
          flashFor(setFlash, `copied entry #${entry.id} (${text.length} bytes)`);
        } catch (err) {
          flashFor(setFlash, `copy failed: ${err instanceof Error ? err.message : String(err)}`);
        }
        return;
      }
      return;
    }
    if (input === "q" || key.escape || (key.ctrl && input === "c")) {
      source.close();
      exit();
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

  const widths = useMemo(() => columnWidths(config, stdout?.columns ?? 100), [config, stdout?.columns]);
  const start = Math.max(0, Math.min(cursor - Math.floor(visible / 2), entries.length - visible));
  const window = entries.slice(start, start + visible);

  if (view === "detail" && entries[cursor]) {
    return (
      <Detail
        entry={entries[cursor]}
        sourceLabel={sourceLabel}
        position={{ index: cursor, total: entries.length }}
        flash={flash}
      />
    );
  }

  return (
    <Box flexDirection="column" height={totalRows} width={stdout?.columns}>
      <Header
        config={config}
        widths={widths}
        sourceLabel={sourceLabel}
        count={entries.length}
        done={done}
        follow={follow}
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
                config={config}
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
              move · u up · o open · g/G top/bottom · q quit
            </Text>
          </Text>
        ) : (
          <Text dimColor>
            {entries.length === 0 ? 0 : cursor + 1}/{entries.length} · j/k move · u up · o open · f
            follow · g/G top/bottom · q quit
          </Text>
        )}
      </Box>
    </Box>
  );
};

const Header: React.FC<{
  config: Config;
  widths: number[];
  sourceLabel: string;
  count: number;
  done: boolean;
  follow: boolean;
}> = ({ config, widths, sourceLabel, count, done, follow }) => (
  <Box flexDirection="column">
    <Text>
      {follow ? (
        <>
          <Text color="green" bold inverse>{" FOLLOW "}</Text>
          <Text> </Text>
        </>
      ) : null}
      <Text bold>log-viewer</Text>
      <Text dimColor>
        {" "}· {sourceLabel} · {count} entries{done ? " (eof)" : ""}
      </Text>
    </Text>
    <Box>
      {config.fields.map((field, i) => (
        <Box key={field.name} width={widths[i]} marginRight={1}>
          <Text bold underline>{truncate(field.name, widths[i] - 1)}</Text>
        </Box>
      ))}
    </Box>
  </Box>
);

const Row: React.FC<{
  entry: LogEntry;
  config: Config;
  widths: number[];
  selected: boolean;
  tail: boolean;
}> = ({ entry, config, widths, selected, tail }) => {
  const cols = renderColumns(entry, config);
  const levelValue = cols.find((c) => c.name.toLowerCase() === "level")?.value ?? "";
  const color = selected ? undefined : levelColor(levelValue);
  return (
    <Box>
      <Text color={tail ? "green" : undefined}>{tail ? "▌" : " "}</Text>
      {cols.map((col, i) => (
        <Box key={col.name} width={widths[i]} marginRight={1}>
          <Text inverse={selected} color={color}>
            {truncate(col.value, Math.max(1, widths[i] - 1))}
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
  flash: string | null;
}> = ({ entry, sourceLabel, position, flash }) => {
  const pretty = safeJson(entry.data);
  return (
    <Box flexDirection="column">
      <Text>
        <Text bold>entry #{entry.id}</Text>
        <Text dimColor>
          {" "}
          · {position.index + 1}/{position.total} · {sourceLabel}
          {entry.wrapped ? " · wrapped" : ""}
        </Text>
      </Text>
      <Box marginTop={1} flexDirection="column">
        {pretty.split("\n").map((line, i) => (
          <Text key={i}>{line}</Text>
        ))}
      </Box>
      <Box marginTop={1}>
        <Text dimColor>
          j/k next/prev · c copy json · u/esc back · q back
          {flash ? `  ·  ${flash}` : ""}
        </Text>
      </Box>
    </Box>
  );
};

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

function columnWidths(config: Config, totalColumns: number): number[] {
  const n = config.fields.length;
  if (n === 0) return [];
  // Give the last column whatever's left; share the rest evenly with sane minimums.
  const minimums = config.fields.map((f) => Math.max(8, f.name.length + 2));
  const fixed = minimums.slice(0, -1).reduce((a, b) => a + b, 0);
  const last = Math.max(20, totalColumns - fixed - n);
  return [...minimums.slice(0, -1), last];
}

function truncate(value: string, width: number): string {
  const clean = value.replace(/\s+/g, " ");
  if (clean.length <= width) return clean;
  return clean.slice(0, Math.max(0, width - 1)) + "…";
}

function levelColor(level: string): string | undefined {
  const v = level.toLowerCase();
  if (v.includes("error") || v === "err" || v === "fatal") return "red";
  if (v.includes("warn")) return "yellow";
  if (v.includes("info")) return "cyan";
  if (v.includes("debug") || v.includes("trace")) return "gray";
  return undefined;
}
