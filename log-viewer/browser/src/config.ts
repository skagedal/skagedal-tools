import { existsSync, mkdirSync, readFileSync, writeFileSync } from "fs";
import { homedir } from "os";
import { dirname, isAbsolute, join } from "path";
import JSON5 from "json5";
import type { TriggerConfig } from "./triggers.js";

/**
 * Configuration for log-viewer. The config file lives at
 *   ~/.config/skagedal-tools/log-viewer/config.json5
 * (override via $XDG_CONFIG_HOME, or $LOG_VIEWER_CONFIG for an explicit
 * path).
 *
 * JSON5 lets the file have comments, trailing commas, and unquoted keys.
 *
 * Example:
 *
 *   {
 *     // Field name used to wrap lines that aren't valid JSON. Matches log-jsonify.
 *     default_field: "message",
 *
 *     // Columns shown in the log list. The first \`from\` key with a non-empty
 *     // value wins per logical column.
 *     fields: [
 *       { name: "time", from: ["@timestamp", "ts", "time"] },
 *       { name: "level", from: ["level", "severity"] },
 *       { name: "message", from: ["message", "msg", "@message"] },
 *     ],
 *   }
 */
export interface FieldConfig {
  /** Display name (column header). */
  name: string;
  /** Candidate field names to look up, in order, on each entry. */
  from: string[];
}

export interface Profile {
  name: string;
  fields?: FieldConfig[];
  defaultField?: string;
}

export interface Config {
  fields: FieldConfig[];
  defaultField: string;
  triggers: TriggerConfig[];
  profiles: Profile[];
}

export const DEFAULT_CONFIG: Config = {
  fields: [
    { name: "time", from: ["@timestamp", "timestamp", "time", "ts"] },
    { name: "level", from: ["level", "severity", "lvl"] },
    { name: "message", from: ["message", "msg", "@message"] },
  ],
  defaultField: "message",
  triggers: [],
  profiles: [],
};

export class ConfigError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "ConfigError";
  }
}

export function configPath(env: NodeJS.ProcessEnv = process.env): string {
  if (env.LOG_VIEWER_CONFIG) return env.LOG_VIEWER_CONFIG;
  // XDG base directory spec: a relative $XDG_CONFIG_HOME is invalid and ignored.
  const xdg = env.XDG_CONFIG_HOME;
  const base = xdg && isAbsolute(xdg) ? xdg : join(homedir(), ".config");
  return join(base, "skagedal-tools", "log-viewer", "config.json5");
}

export function loadConfig(path: string = configPath()): Config {
  if (!existsSync(path)) return { ...DEFAULT_CONFIG };
  const raw = readFileSync(path, "utf8");
  return parseConfig(raw);
}

export function parseConfig(source: string): Config {
  let doc: Record<string, unknown>;
  try {
    const parsed = JSON5.parse(source);
    if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) {
      throw new ConfigError("config.json5 must be a JSON5 object at the top level");
    }
    doc = parsed as Record<string, unknown>;
  } catch (err) {
    if (err instanceof ConfigError) throw err;
    const message = err instanceof Error ? err.message : String(err);
    throw new ConfigError(`failed to parse config.json5: ${message}`);
  }

  const fields = parseFields(doc.fields) ?? DEFAULT_CONFIG.fields;
  const defaultField =
    typeof doc.default_field === "string" ? doc.default_field : DEFAULT_CONFIG.defaultField;
  const triggers = parseTriggers(doc.triggers);
  const profiles = parseProfiles(doc.profiles);
  return { fields, defaultField, triggers, profiles };
}

function parseProfiles(value: unknown): Profile[] {
  if (!Array.isArray(value)) return [];
  const out: Profile[] = [];
  for (const entry of value) {
    if (!entry || typeof entry !== "object") continue;
    const e = entry as Record<string, unknown>;
    const name = typeof e.name === "string" ? e.name : null;
    if (!name) continue;
    const fields = parseFields(e.fields) ?? undefined;
    const defaultField =
      typeof e.default_field === "string" ? e.default_field : undefined;
    out.push({ name, fields, defaultField });
  }
  return out;
}

/**
 * Apply a named profile to the loaded config. The profile's `fields` and
 * `default_field` (when present) replace the top-level values; everything
 * else (triggers, profile list) is preserved.
 */
export function applyProfile(config: Config, profileName: string): Config {
  const profile = config.profiles.find((p) => p.name === profileName);
  if (!profile) {
    const known = config.profiles.map((p) => p.name).join(", ") || "(none)";
    throw new ConfigError(`unknown profile "${profileName}" (defined: ${known})`);
  }
  return {
    ...config,
    fields: profile.fields ?? config.fields,
    defaultField: profile.defaultField ?? config.defaultField,
  };
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

const TEMPLATE = `// log-viewer config (JSON5: comments, trailing commas, unquoted keys ok)
{
  // Field name used to wrap lines that aren't valid JSON. Matches log-jsonify.
  default_field: "message",

  // Each entry is a column shown in the log list. \`from\` lists candidate keys
  // to read from each JSON entry; the first one with a non-empty value wins.
  fields: [
    { name: "time",    from: ["@timestamp", "timestamp", "time", "ts"] },
    { name: "level",   from: ["level", "severity", "lvl"] },
    { name: "message", from: ["message", "msg", "@message"] },
  ],

  // Triggers run a shell command the first time a field takes on a new value.
  // Useful for noticing new pods, hosts, jobs, etc. in a streaming feed.
  // {value} and {field} are substituted (shell-quoted) into \`action\`.
  // \`startup_delay_ms\` suppresses the trigger for that long after startup so
  // values that already exist when log-viewer attaches don't fire it.
  triggers: [
    // {
    //   name: "pod-deployed",
    //   on_new_value: "podname",
    //   action: "say new pod {value} deployed",
    //   startup_delay_ms: 2000,
    // },
  ],

  // Profiles override \`fields\` (and optionally \`default_field\`) when selected
  // via \`--profile <name>\`. Useful for switching between different log
  // shapes — e.g. one for app logs, one for kubectl/stern.
  profiles: [
    // {
    //   name: "stern",
    //   fields: [
    //     { name: "time", from: ["timestamp"] },
    //     { name: "pod",  from: ["podName"] },
    //     { name: "msg",  from: ["message"] },
    //   ],
    // },
  ],
}
`;

export function ensureConfigFile(path: string = configPath()): boolean {
  if (existsSync(path)) return false;
  mkdirSync(dirname(path), { recursive: true });
  writeFileSync(path, TEMPLATE, { encoding: "utf8", flag: "wx" });
  return true;
}
