import { readFileSync, existsSync, mkdirSync, writeFileSync, appendFileSync } from "fs";
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
 * Per the skagedal-tools convention, per-tool state lives under
 * ~/.skagedal-tools/<tool-name>/ (matches git-dirty-checker).
 *
 * Order:
 *   1. $CLOUDWATCH_INSIGHTS_CONFIG (explicit override, mainly for tests)
 *   2. $SKAGEDAL_TOOLS_HOME/cloudwatch-insights/settings.toml
 *   3. ~/.skagedal-tools/cloudwatch-insights/settings.toml
 */
export function configPath(env: NodeJS.ProcessEnv = process.env): string {
  if (env.CLOUDWATCH_INSIGHTS_CONFIG) return env.CLOUDWATCH_INSIGHTS_CONFIG;
  const base = env.SKAGEDAL_TOOLS_HOME || join(homedir(), ".skagedal-tools");
  return join(base, "cloudwatch-insights", "settings.toml");
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
 * Template used to seed a fresh `current.insights`. The placeholder
 * `{app}` is substituted with the configured app when available; when no
 * app is configured, the `filter app = ...` line is omitted.
 */
export const DEFAULT_QUERY_TEMPLATE = `fields @timestamp, @message
| sort @timestamp desc
| filter app = {app}
| filter level in ['WARN', 'ERROR']
| limit 200`;

export function defaultQuery(app?: string): string {
  if (app === undefined) {
    return DEFAULT_QUERY_TEMPLATE.split("\n")
      .filter((line) => !line.includes("{app}"))
      .join("\n");
  }
  return DEFAULT_QUERY_TEMPLATE.replace("{app}", app);
}

const DEFAULT_SETTINGS_TEMPLATE = `# cloudwatch-insights settings
#
# Each section is keyed by the git repository directory name. Supported fields:
#   group = "/{env}/my-team"    # log group template; {env} is replaced by --environment
#   app   = "my-service"        # default query filter on the \`app\` field
#
# Run \`cloudwatch-insights edit-config\` from inside a git repository and a
# commented placeholder section for that repo will be appended below.
`;

export interface EnsureResult {
  /** True if the config file itself was created in this call. */
  fileCreated: boolean;
  /** Name of a section appended for the current repo, or null if none was. */
  addedSection: string | null;
}

/**
 * Ensure the config file exists at `path`, creating any missing parent
 * directories and seeding the file with a commented template if absent.
 * Additionally, if called from inside a git repository whose basename is
 * not yet a section in the file, append a commented-out placeholder section
 * so the user just has to uncomment and fill in values.
 */
export function ensureConfigFile(
  path: string = configPath(),
  cwd: string = process.cwd(),
): EnsureResult {
  let fileCreated = false;
  if (!existsSync(path)) {
    mkdirSync(dirname(path), { recursive: true });
    writeFileSync(path, DEFAULT_SETTINGS_TEMPLATE, { encoding: "utf8", flag: "wx" });
    fileCreated = true;
  }

  let addedSection: string | null = null;
  const root = findGitRoot(cwd);
  if (root) {
    const name = basename(root);
    const contents = readFileSync(path, "utf8");
    if (!hasSectionHeader(contents, name)) {
      const block = placeholderSection(name);
      const prefix = endsWithNewline(contents) ? "\n" : "\n\n";
      appendFileSync(path, prefix + block, "utf8");
      addedSection = name;
    }
  }

  return { fileCreated, addedSection };
}

/**
 * Does the raw file already contain `[name]` as a section header — either
 * as real TOML or as a commented-out placeholder? Used so we don't append
 * a second placeholder for a repo that already has one.
 */
function hasSectionHeader(contents: string, name: string): boolean {
  const escaped = name.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const re = new RegExp(`^\\s*#?\\s*\\[${escaped}\\]\\s*$`, "m");
  return re.test(contents);
}

function placeholderSection(repoName: string): string {
  return (
    `# ${repoName}: uncomment and fill in the values you want as defaults\n` +
    `# when running cloudwatch-insights from this repository.\n` +
    `# [${repoName}]\n` +
    `# group = "/{env}/my-team"\n` +
    `# app   = "${repoName}"\n`
  );
}

function endsWithNewline(contents: string): boolean {
  return contents.length === 0 || contents.endsWith("\n");
}
