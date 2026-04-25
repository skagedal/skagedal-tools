import { existsSync, mkdirSync, readFileSync, writeFileSync } from "fs";
import { homedir } from "os";
import { dirname, join } from "path";
import { parse as parseToml } from "smol-toml";
import type { TriggerConfig } from "./triggers.js";

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
  triggers: TriggerConfig[];
}

export const DEFAULT_CONFIG: Config = {
  fields: [
    { name: "time", from: ["@timestamp", "timestamp", "time", "ts"] },
    { name: "level", from: ["level", "severity", "lvl"] },
    { name: "message", from: ["message", "msg", "@message"] },
  ],
  defaultField: "message",
  triggers: [],
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
  const triggers = parseTriggers(doc.triggers);
  return { fields, defaultField, triggers };
}

function parseTriggers(value: unknown): TriggerConfig[] {
  if (!Array.isArray(value)) return [];
  const out: TriggerConfig[] = [];
  for (const entry of value) {
    if (!entry || typeof entry !== "object") continue;
    const e = entry as Record<string, unknown>;
    const action = typeof e.action === "string" ? e.action : null;
    if (!action) continue;
    const onNewValue = Array.isArray(e.on_new_value)
      ? e.on_new_value.filter((v): v is string => typeof v === "string")
      : typeof e.on_new_value === "string"
        ? [e.on_new_value]
        : [];
    if (onNewValue.length === 0) continue;
    const name = typeof e.name === "string" ? e.name : onNewValue[0]!;
    const startupDelayMs =
      typeof e.startup_delay_ms === "number" && e.startup_delay_ms >= 0
        ? e.startup_delay_ms
        : 0;
    out.push({ name, onNewValue, action, startupDelayMs });
  }
  return out;
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

# Triggers run a shell command the first time a field takes on a new value.
# Useful for noticing new pods, hosts, jobs, etc. in a streaming feed.
# {value} and {field} are substituted (shell-quoted) into \`action\`.
# \`startup_delay_ms\` suppresses the trigger for that long after startup so
# values that already exist when log-viewer attaches don't fire it.
#
# [[triggers]]
# name = "pod-deployed"
# on_new_value = "podname"
# action = "say new pod {value} deployed"
# startup_delay_ms = 2000
`;

export function ensureConfigFile(path: string = configPath()): boolean {
  if (existsSync(path)) return false;
  mkdirSync(dirname(path), { recursive: true });
  writeFileSync(path, TEMPLATE, { encoding: "utf8", flag: "wx" });
  return true;
}
