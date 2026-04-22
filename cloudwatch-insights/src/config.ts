import { readFileSync, existsSync } from "fs";
import { homedir } from "os";
import { basename, dirname, join } from "path";
import { spawnSync } from "child_process";
import { parse as parseToml } from "smol-toml";

export type Environment = "systest" | "uat" | "prod";

export const ENVIRONMENTS: readonly Environment[] = ["systest", "uat", "prod"];

/**
 * A single named section in settings.toml. Additional keys are ignored so
 * users can extend the schema without breaking old builds of this tool.
 */
export interface RepoDefaults {
  /** Log group template, may contain "{env}" placeholder. */
  group?: string;
  /** App name — used to build a default Insights query. */
  app?: string;
}

export interface Settings {
  /** Map from section name (conventionally: git repo basename) to defaults. */
  sections: Record<string, RepoDefaults>;
}

export class ConfigError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "ConfigError";
  }
}

/**
 * Return the absolute path to the config file.
 *
 * Order:
 *   1. $CLOUDWATCH_INSIGHTS_CONFIG (explicit override)
 *   2. $XDG_CONFIG_HOME/cloudwatch-insights/settings.toml
 *   3. ~/.config/cloudwatch-insights/settings.toml
 */
export function configPath(env: NodeJS.ProcessEnv = process.env): string {
  if (env.CLOUDWATCH_INSIGHTS_CONFIG) return env.CLOUDWATCH_INSIGHTS_CONFIG;
  const xdg = env.XDG_CONFIG_HOME || join(homedir(), ".config");
  return join(xdg, "cloudwatch-insights", "settings.toml");
}

/**
 * Load settings from disk. A missing file yields empty settings rather than
 * an error — the tool is fully usable without any config.
 */
export function loadSettings(path: string = configPath()): Settings {
  if (!existsSync(path)) {
    return { sections: {} };
  }
  const contents = readFileSync(path, "utf8");
  return parseSettings(contents);
}

/** Parse a TOML string into structured settings. Exported for tests. */
export function parseSettings(toml: string): Settings {
  let doc: Record<string, unknown>;
  try {
    doc = parseToml(toml) as Record<string, unknown>;
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    throw new ConfigError(`failed to parse settings.toml: ${message}`);
  }

  const sections: Record<string, RepoDefaults> = {};
  for (const [key, value] of Object.entries(doc)) {
    if (value && typeof value === "object" && !Array.isArray(value)) {
      const section = value as Record<string, unknown>;
      const defaults: RepoDefaults = {};
      if (typeof section.group === "string") defaults.group = section.group;
      if (typeof section.app === "string") defaults.app = section.app;
      sections[key] = defaults;
    }
  }
  return { sections };
}

/**
 * Resolve the top-level directory of the git repository containing `cwd`.
 * Returns null if `cwd` is not inside a git repository.
 *
 * We prefer shelling out to `git rev-parse` when available (handles
 * worktrees, submodules, $GIT_DIR correctly) and fall back to walking up
 * the tree looking for a ".git" entry.
 */
export function findGitRoot(cwd: string = process.cwd()): string | null {
  const result = spawnSync("git", ["-C", cwd, "rev-parse", "--show-toplevel"], {
    encoding: "utf8",
  });
  if (result.status === 0) {
    return result.stdout.trim() || null;
  }

  let dir = cwd;
  while (true) {
    if (existsSync(join(dir, ".git"))) return dir;
    const parent = dirname(dir);
    if (parent === dir) return null;
    dir = parent;
  }
}

/**
 * Pick the defaults section for the current git repo (by basename of the
 * repo root). Returns both the section name we looked up and the defaults
 * we found, so the caller can print a helpful message.
 */
export function resolveRepoDefaults(
  settings: Settings,
  cwd: string = process.cwd(),
): { sectionName: string | null; defaults: RepoDefaults } {
  const root = findGitRoot(cwd);
  if (!root) return { sectionName: null, defaults: {} };
  const name = basename(root);
  const defaults = settings.sections[name] ?? {};
  return { sectionName: name, defaults };
}

/** Substitute "{env}" in a template. Throws if the template uses {env} but none was supplied. */
export function applyEnvironment(template: string, env: Environment | undefined): string {
  if (!template.includes("{env}")) return template;
  if (!env) {
    throw new ConfigError(
      `log group template "${template}" uses {env} — pass --environment systest|uat|prod`,
    );
  }
  return template.replaceAll("{env}", env);
}

/**
 * Build the default Insights query for an `app` filter.
 * Escapes double quotes so unusual app names don't break the query.
 */
export function defaultQueryForApp(app: string): string {
  const escaped = app.replace(/\\/g, "\\\\").replace(/"/g, '\\"');
  return `fields @timestamp, @message, app | filter app = "${escaped}" | sort @timestamp desc`;
}
