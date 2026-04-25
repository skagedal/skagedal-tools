import React, { useEffect, useMemo, useState } from "react";
import { Box, Text, useApp, useInput, useStdout } from "ink";
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

  useEffect(() => {
    source.onEntry((entry) => {
      setEntries((prev) => [...prev, entry]);
    });
    source.onEnd(() => setDone(true));
    return () => source.close();
  }, [source]);

  // Keep cursor in bounds as entries arrive.
  useEffect(() => {
    if (cursor >= entries.length && entries.length > 0) {
      setCursor(entries.length - 1);
    }
  }, [entries.length, cursor]);

  const rows = (stdout?.rows ?? 24) - 4;
  const visible = Math.max(1, rows);

  useInput((input, key) => {
    if (view === "detail") {
      if (input === "q" || key.escape || input === "u") {
        setView("list");
        return;
      }
      return;
    }
    if (input === "q" || key.escape || (key.ctrl && input === "c")) {
      source.close();
      exit();
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
    if (input === "u") {
      setCursor((c) => Math.max(0, c - 1));
      return;
    }
    if (input === "o" || key.return) {
      if (entries.length > 0) setView("detail");
      return;
    }
    if (input === "g") {
      setCursor(0);
      return;
    }
    if (input === "G") {
      setCursor(Math.max(0, entries.length - 1));
      return;
    }
  });

  const widths = useMemo(() => columnWidths(config, stdout?.columns ?? 100), [config, stdout?.columns]);
  const start = Math.max(0, Math.min(cursor - Math.floor(visible / 2), entries.length - visible));
  const window = entries.slice(start, start + visible);

  if (view === "detail" && entries[cursor]) {
    return <Detail entry={entries[cursor]} sourceLabel={sourceLabel} />;
  }

  return (
    <Box flexDirection="column">
      <Header config={config} widths={widths} sourceLabel={sourceLabel} count={entries.length} done={done} />
      {window.length === 0 ? (
        <Text dimColor>(waiting for log lines…)</Text>
      ) : (
        window.map((entry, i) => {
          const idx = start + i;
          const selected = idx === cursor;
          return <Row key={entry.id} entry={entry} config={config} widths={widths} selected={selected} />;
        })
      )}
      <Box marginTop={1}>
        <Text dimColor>
          {cursor + 1}/{entries.length} · j/k move · u up · o open · g/G top/bottom · q quit
        </Text>
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
}> = ({ config, widths, sourceLabel, count, done }) => (
  <Box flexDirection="column">
    <Text>
      <Text bold>log-viewer</Text>
      <Text dimColor> · {sourceLabel} · {count} entries{done ? " (eof)" : ""}</Text>
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
}> = ({ entry, config, widths, selected }) => {
  const cols = renderColumns(entry, config);
  const levelValue = cols.find((c) => c.name.toLowerCase() === "level")?.value ?? "";
  const color = selected ? undefined : levelColor(levelValue);
  return (
    <Box>
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

const Detail: React.FC<{ entry: LogEntry; sourceLabel: string }> = ({ entry, sourceLabel }) => {
  const pretty = (() => {
    try {
      return JSON.stringify(entry.data, null, 2);
    } catch {
      return entry.raw;
    }
  })();
  return (
    <Box flexDirection="column">
      <Text>
        <Text bold>entry #{entry.id}</Text>
        <Text dimColor> · {sourceLabel}{entry.wrapped ? " · wrapped" : ""}</Text>
      </Text>
      <Box marginTop={1} flexDirection="column">
        {pretty.split("\n").map((line, i) => (
          <Text key={i}>{line}</Text>
        ))}
      </Box>
      <Box marginTop={1}>
        <Text dimColor>u/esc back · q back</Text>
      </Box>
    </Box>
  );
};

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
