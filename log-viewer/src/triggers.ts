import { spawn } from "child_process";
import type { LogEntry } from "./entry.js";

/**
 * A trigger fires a shell action the first time a configured field takes on
 * a value it hasn't seen before — useful for noticing new pods/hosts/jobs in
 * a streaming log feed. Pass `startupDelayMs` to skip values that already
 * exist when log-viewer starts (so a config of `kubectl logs` doesn't fire
 * once per running pod on attach).
 */
export interface TriggerConfig {
  /** Human-readable label used only for log lines. */
  name: string;
  /** Candidate field names to read; first non-empty value wins. */
  onNewValue: string[];
  /** Shell command to run. `{value}` and `{field}` are substituted (shell-quoted). */
  action: string;
  /** Suppress the action for this many milliseconds after startup. */
  startupDelayMs: number;
}

interface TriggerState {
  config: TriggerConfig;
  seen: Set<string>;
}

export interface TriggerRuntime {
  start(): void;
  handle(entry: LogEntry): void;
}

export interface TriggerHooks {
  /** Called when an action is about to be spawned. Useful for tests/logging. */
  onFire?: (info: { trigger: TriggerConfig; value: string; command: string }) => void;
  /** Override the start time (for tests). Default Date.now(). */
  now?: () => number;
}

export function createTriggerRuntime(
  configs: TriggerConfig[],
  hooks: TriggerHooks = {},
): TriggerRuntime {
  const states: TriggerState[] = configs.map((config) => ({ config, seen: new Set() }));
  const now = hooks.now ?? (() => Date.now());
  let startTime = now();

  return {
    start() {
      startTime = now();
    },
    handle(entry: LogEntry) {
      for (const state of states) {
        const value = pickValue(entry, state.config.onNewValue);
        if (value === null) continue;
        if (state.seen.has(value)) continue;
        state.seen.add(value);
        if (now() - startTime < state.config.startupDelayMs) continue;
        fireAction(state.config, value, entry, hooks.onFire);
      }
    },
  };
}

function pickValue(entry: LogEntry, candidates: string[]): string | null {
  for (const key of candidates) {
    const v = entry.data[key];
    if (v === undefined || v === null || v === "") continue;
    return typeof v === "string" ? v : JSON.stringify(v);
  }
  return null;
}

function fireAction(
  trigger: TriggerConfig,
  value: string,
  entry: LogEntry,
  onFire?: TriggerHooks["onFire"],
): void {
  const command = substitute(trigger.action, {
    value,
    field: trigger.onNewValue[0] ?? "",
  });
  onFire?.({ trigger, value, command });
  const child = spawn(command, {
    shell: true,
    stdio: "ignore",
    detached: true,
    env: {
      ...process.env,
      LOG_VIEWER_VALUE: value,
      LOG_VIEWER_FIELD: trigger.onNewValue[0] ?? "",
      LOG_VIEWER_TRIGGER: trigger.name,
      LOG_VIEWER_ENTRY: safeStringify(entry.data),
    },
  });
  child.on("error", (err) => {
    process.stderr.write(
      `log-viewer: trigger "${trigger.name}" failed: ${err.message}\n`,
    );
  });
  child.unref();
}

function substitute(template: string, vars: Record<string, string>): string {
  return template.replace(/\{(\w+)\}/g, (match, key: string) => {
    if (!(key in vars)) return match;
    return shellQuote(vars[key]!);
  });
}

function shellQuote(value: string): string {
  return "'" + value.replace(/'/g, "'\\''") + "'";
}

function safeStringify(value: unknown): string {
  try {
    return JSON.stringify(value);
  } catch {
    return "";
  }
}
