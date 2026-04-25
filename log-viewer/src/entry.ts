import type { Config, FieldConfig } from "./config.js";

/** A single parsed log line. */
export interface LogEntry {
  /** Index in the order entries arrived. */
  id: number;
  /** Raw input line (without trailing newline). */
  raw: string;
  /** Parsed JSON payload (always an object — non-JSON lines are wrapped). */
  data: Record<string, unknown>;
  /** True when the original line wasn't valid JSON and was wrapped. */
  wrapped: boolean;
}

export function parseLine(raw: string, id: number, defaultField: string): LogEntry {
  const trimmed = raw.replace(/\r$/, "");
  try {
    const parsed = JSON.parse(trimmed);
    if (parsed && typeof parsed === "object" && !Array.isArray(parsed)) {
      return { id, raw: trimmed, data: parsed as Record<string, unknown>, wrapped: false };
    }
    // Valid JSON but not an object (e.g. a bare string/number) — wrap it so the
    // viewer always has a stable shape to render.
    return {
      id,
      raw: trimmed,
      data: { [defaultField]: parsed },
      wrapped: true,
    };
  } catch {
    return {
      id,
      raw: trimmed,
      data: { [defaultField]: trimmed },
      wrapped: true,
    };
  }
}

/** Render the value of one configured field on an entry. Returns "" if absent. */
export function renderField(entry: LogEntry, field: FieldConfig): string {
  for (const key of field.from) {
    const value = entry.data[key];
    if (value === undefined || value === null || value === "") continue;
    return stringify(value);
  }
  return "";
}

function stringify(value: unknown): string {
  if (typeof value === "string") return value;
  if (typeof value === "number" || typeof value === "boolean") return String(value);
  try {
    return JSON.stringify(value);
  } catch {
    return String(value);
  }
}

/** Render every configured column for an entry, returning [name, value] pairs. */
export function renderColumns(entry: LogEntry, config: Config): { name: string; value: string }[] {
  return config.fields.map((field) => ({ name: field.name, value: renderField(entry, field) }));
}
