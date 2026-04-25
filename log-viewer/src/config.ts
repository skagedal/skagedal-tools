import { existsSync, mkdirSync, readFileSync, writeFileSync } from "fs";
import { homedir } from "os";
import { dirname, join } from "path";
import { parse as parseToml } from "smol-toml";

/**
 * Configuration for log-viewer. The config file lives at
 *   ~/.skagedal-tools/log-viewer/config.toml
 * (override via $SKAGEDAL_TOOLS_HOME for tests, or $LOG_VIEWER_CONFIG for an
 * explicit path).
 *
 * Example:
 *
 *   # Fields displayed in the log list, in order. The first that resolves to a
 *   # non-empty value wins per logical column.
 *   fields = [
 *     { name = "time", from = ["@timestamp", "ts", "time"] },
 *     { name = "level", from = ["level", "severity"] },
 *     { name = "message", from = ["message", "msg", "@message"] },
 *   ]
 *
 *   # Field name used to wrap lines that aren't valid JSON. Matches log-jsonify.
 *   default_field = "message"
 */
export interface FieldConfig {
  /** Display name (column header). */
  name: string;
  /** Candidate field names to look up, in order, on each entry. */
  from: string[];
}

export interface Config {
  fields: FieldConfig[];
  defaultField: string;
}

export const DEFAULT_CONFIG: Config = {
  fields: [
    { name: "time", from: ["@timestamp", "timestamp", "time", "ts"] },
    { name: "level", from: ["level", "severity", "lvl"] },
    { name: "message", from: ["message", "msg", "@message"] },
  ],
  defaultField: "message",
};

export class ConfigError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "ConfigError";
  }
}

export function configPath(env: NodeJS.ProcessEnv = process.env): string {
  if (env.LOG_VIEWER_CONFIG) return env.LOG_VIEWER_CONFIG;
  const base = env.SKAGEDAL_TOOLS_HOME || join(homedir(), ".skagedal-tools");
  return join(base, "log-viewer", "config.toml");
}

export function loadConfig(path: string = configPath()): Config {
  if (!existsSync(path)) return { ...DEFAULT_CONFIG };
  const raw = readFileSync(path, "utf8");
  return parseConfig(raw);
}

export function parseConfig(toml: string): Config {
  let doc: Record<string, unknown>;
  try {
    doc = parseToml(toml) as Record<string, unknown>;
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    throw new ConfigError(`failed to parse config.toml: ${message}`);
  }

  const fields = parseFields(doc.fields) ?? DEFAULT_CONFIG.fields;
  const defaultField =
    typeof doc.default_field === "string" ? doc.default_field : DEFAULT_CONFIG.defaultField;
  return { fields, defaultField };
}

function parseFields(value: unknown): FieldConfig[] | null {
  if (!Array.isArray(value)) return null;
  const out: FieldConfig[] = [];
  for (const entry of value) {
    if (!entry || typeof entry !== "object") continue;
    const e = entry as Record<string, unknown>;
    if (typeof e.name !== "string") continue;
    const from = Array.isArray(e.from)
      ? (e.from.filter((f): f is string => typeof f === "string"))
      : typeof e.from === "string"
        ? [e.from]
        : [e.name];
    out.push({ name: e.name, from });
  }
  return out.length > 0 ? out : null;
}

const TEMPLATE = `# log-viewer config
#
# Each entry in \`fields\` is a column shown in the log list. \`from\` lists
# candidate keys to read from each JSON entry; the first one with a non-empty
# value wins.

default_field = "message"

[[fields]]
name = "time"
from = ["@timestamp", "timestamp", "time", "ts"]

[[fields]]
name = "level"
from = ["level", "severity", "lvl"]

[[fields]]
name = "message"
from = ["message", "msg", "@message"]
`;

export function ensureConfigFile(path: string = configPath()): boolean {
  if (existsSync(path)) return false;
  mkdirSync(dirname(path), { recursive: true });
  writeFileSync(path, TEMPLATE, { encoding: "utf8", flag: "wx" });
  return true;
}
