import { readFileSync, existsSync, mkdirSync, writeFileSync, appendFileSync } from "fs";
import { homedir } from "os";
import { basename, dirname, join } from "path";
import { spawnSync } from "child_process";
import { parse as parseToml } from "smol-toml";

/** AWS-related settings scoped to a single environment. */
export interface EnvConfig {
  awsProfile?: string;
  region?: string;
  /** AWS account ID — used to build log-group ARNs for shareable console links. */
  accountId?: string;
}

/**
 * Configuration that applies to a repository — either as the top-level
 * [defaults] block or a per-repo [repo.<name>] block. Per-repo values
 * fully replace defaults values (arrays are not merged).
 */
export interface RepoConfig {
  /** Log group template, may contain "{{ env }}" / "{{ app }}" placeholders. */
  group?: string;
  /** App name — used to build a default Insights query. */
  app?: string;
  /** Field names whose value is a JSON object to be flattened into the row. */
  flattenFields?: string[];
  /**
   * Per-environment overrides scoped to this repo, parsed from
   * [repo.<repo>.env.<env>] sections. Each entry is merged on top of the
   * top-level [env.<env>] config on a per-key basis.
   */
  envOverrides?: Record<string, EnvConfig>;
}

export interface Settings {
  /** Defaults applied to every repo when not overridden. */
  defaults: RepoConfig;
  /** Per-repo configuration, keyed by git repo basename. */
  repos: Record<string, RepoConfig>;
  /** Top-level [env.<name>] sections — defaults for every repo. */
  environments: Record<string, EnvConfig>;
}

export class ConfigError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "ConfigError";
  }
}

const ENV_NAME_RE = /^[a-z][a-z0-9]*(-[a-z0-9]+)*$/;

/**
 * Returns true if `name` is a syntactically valid environment name —
 * lower kebab-case: starts with a letter, segments separated by single
 * dashes, no underscores or uppercase. Used both to validate CLI input
 * and to filter [env.<name>] keys in the config.
 */
export function isValidEnvironmentName(name: string): boolean {
  return ENV_NAME_RE.test(name);
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
    return { defaults: {}, repos: {}, environments: {} };
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

  const defaults = isPlainObject(doc.defaults) ? parseRepoConfig(doc.defaults) : {};

  const repos: Record<string, RepoConfig> = {};
  if (isPlainObject(doc.repo)) {
    for (const [name, value] of Object.entries(doc.repo)) {
      if (isPlainObject(value)) {
        repos[name] = parseRepoConfig(value);
      }
    }
  }

  const environments = isPlainObject(doc.env) ? parseEnvironmentMap(doc.env) : {};

  return { defaults, repos, environments };
}

function parseRepoConfig(section: Record<string, unknown>): RepoConfig {
  const config: RepoConfig = {};
  if (typeof section.group === "string") config.group = section.group;
  if (typeof section.app === "string") config.app = section.app;
  if (Array.isArray(section["flatten-fields"])) {
    const fields = section["flatten-fields"].filter((f): f is string => typeof f === "string");
    config.flattenFields = fields;
  }
  if (isPlainObject(section.env)) {
    const overrides = parseEnvironmentMap(section.env);
    if (Object.keys(overrides).length > 0) config.envOverrides = overrides;
  }
  return config;
}

function parseEnvironmentMap(section: Record<string, unknown>): Record<string, EnvConfig> {
  const out: Record<string, EnvConfig> = {};
  for (const [name, value] of Object.entries(section)) {
    if (!isValidEnvironmentName(name)) continue;
    if (!isPlainObject(value)) continue;
    out[name] = parseEnvConfig(value);
  }
  return out;
}

function parseEnvConfig(section: Record<string, unknown>): EnvConfig {
  const config: EnvConfig = {};
  if (typeof section["aws-profile"] === "string") config.awsProfile = section["aws-profile"];
  if (typeof section.region === "string") config.region = section.region;
  if (typeof section["account-id"] === "string") config.accountId = section["account-id"];
  return config;
}

function isPlainObject(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
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
 * Pick the configuration for the current git repo (by basename of the repo
 * root), merging on top of the [defaults] section. Per-repo values fully
 * replace defaults values (arrays are not concatenated). Returns the
 * section name we looked up so callers can print a helpful message.
 */
export function resolveRepoDefaults(
  settings: Settings,
  cwd: string = process.cwd(),
): { sectionName: string | null; defaults: RepoConfig } {
  const root = findGitRoot(cwd);
  if (!root) return { sectionName: null, defaults: { ...settings.defaults } };
  const name = basename(root);
  const repo = settings.repos[name];
  return { sectionName: name, defaults: mergeRepoConfig(settings.defaults, repo) };
}

function mergeRepoConfig(base: RepoConfig, override: RepoConfig | undefined): RepoConfig {
  if (!override) return { ...base };
  return {
    group: override.group ?? base.group,
    app: override.app ?? base.app,
    flattenFields: override.flattenFields ?? base.flattenFields,
    envOverrides: override.envOverrides ?? base.envOverrides,
  };
}

/**
 * Resolve the effective env config for a repo + environment by merging the
 * top-level [env.<env>] entry with the per-repo [repo.<repo>.env.<env>]
 * override on a per-key basis (override wins for keys it sets).
 */
export function resolveEnvConfig(
  settings: Settings,
  repoName: string | null,
  envName: string,
): EnvConfig {
  const base = settings.environments[envName] ?? {};
  const override = repoName ? settings.repos[repoName]?.envOverrides?.[envName] : undefined;
  if (!override) return { ...base };
  const merged: EnvConfig = {};
  const awsProfile = override.awsProfile ?? base.awsProfile;
  const region = override.region ?? base.region;
  const accountId = override.accountId ?? base.accountId;
  if (awsProfile !== undefined) merged.awsProfile = awsProfile;
  if (region !== undefined) merged.region = region;
  if (accountId !== undefined) merged.accountId = accountId;
  return merged;
}

const PLACEHOLDER_RE = /\{\{\s*([A-Za-z_][A-Za-z0-9_]*)\s*\}\}/g;

/**
 * Substitute Jinja-style `{{ name }}` placeholders (whitespace inside the
 * braces is allowed). Throws ConfigError if the template references a name
 * whose value is undefined in `vars`.
 */
export function expandTemplate(
  template: string,
  vars: Record<string, string | undefined>,
): string {
  return template.replace(PLACEHOLDER_RE, (_, name: string) => {
    const value = vars[name];
    if (value === undefined) {
      throw new ConfigError(
        `template "${template}" uses {{ ${name} }} but no ${name} was supplied`,
      );
    }
    return value;
  });
}

/**
 * Template used to seed a fresh `current.insights`. The seeded body keeps
 * `{{ app }}` / `{{ env }}` placeholders intact — they're expanded at query
 * execution time. When no app is configured, the `filter app = ...` line is
 * omitted from the seed.
 */
export const DEFAULT_QUERY_TEMPLATE = `fields @timestamp, @message
| sort @timestamp desc
| filter app = '{{ app }}'
| filter level in ['WARN', 'ERROR']
| limit 200`;

export function defaultQuery(app?: string): string {
  if (app === undefined) {
    return DEFAULT_QUERY_TEMPLATE.split("\n")
      .filter((line) => !/\{\{\s*app\s*\}\}/.test(line))
      .join("\n");
  }
  return DEFAULT_QUERY_TEMPLATE;
}

const DEFAULT_SETTINGS_TEMPLATE = `# cloudwatch-insights settings
#
# Top-level [defaults] applies to every repo. Per-repo overrides live under
# [repo.<repo-name>] and fully replace the corresponding defaults value
# (arrays are not merged).
#
# Supported fields (in either [defaults] or [repo.<name>]):
#   group           = "/{{ env }}/logs"  # log group template; {{ env }}, {{ app }} expanded at query time
#   app             = "my-service"        # default app filter for the seeded query
#   flatten-fields  = ["@message"]        # JSON-decode these fields and merge their keys into the row
#
# Environments are declared as [env.<name>] (any lower kebab-case name) and may
# set:
#   aws-profile = "..."   # exported as AWS_PROFILE for the run
#   region      = "..."   # AWS region used for the run
#   account-id  = "..."   # AWS account ID; only needed for \`link\` (else looked up via STS)
#
# A repo can override individual env keys via [repo.<repo>.env.<env>] — only
# keys you mention are overridden; the rest fall through to the top-level entry.
#
# Example:
#   [defaults]
#   flatten-fields = ["@message"]
#
#   [env.systest]
#   aws-profile = "company-systest"
#   region      = "eu-west-1"
#
#   [env.prod]
#   aws-profile = "company-prod"
#   region      = "us-east-1"
#
#   [repo.example-service]
#   group = "/{{ env }}/logs"
#   app   = "example-service"
#
#   [repo.example-service.env.prod]
#   aws-profile = "example-prod"   # region inherits from [env.prod]
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
    if (!hasRepoSectionHeader(contents, name)) {
      const block = placeholderSection(name);
      const prefix = endsWithNewline(contents) ? "\n" : "\n\n";
      appendFileSync(path, prefix + block, "utf8");
      addedSection = name;
    }
  }

  return { fileCreated, addedSection };
}

/**
 * Does the raw file already contain `[repo.<name>]` as a section header —
 * either as real TOML or as a commented-out placeholder? Used so we don't
 * append a second placeholder for a repo that already has one.
 */
function hasRepoSectionHeader(contents: string, name: string): boolean {
  const escaped = name.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const re = new RegExp(`^\\s*#?\\s*\\[repo\\.${escaped}\\]\\s*$`, "m");
  return re.test(contents);
}

function placeholderSection(repoName: string): string {
  return (
    `# ${repoName}: uncomment and fill in the values you want as defaults\n` +
    `# when running cloudwatch-insights from this repository.\n` +
    `# [repo.${repoName}]\n` +
    `# group = "/{{ env }}/logs"\n` +
    `# app   = "${repoName}"\n`
  );
}

function endsWithNewline(contents: string): boolean {
  return contents.length === 0 || contents.endsWith("\n");
}
