#!/usr/bin/env node

import { createReadStream, existsSync, readFileSync } from "fs";
import { spawnSync } from "child_process";
import { pipeline } from "stream/promises";
import { cli, define } from "gunshi";
import { CloudWatchLogsClient } from "@aws-sdk/client-cloudwatch-logs";

import { parseTimeRange, TimeRangeParseError } from "./time-range.mjs";
import { runInsightsQuery } from "./insights.mjs";
import { flattenRow } from "./flatten.mjs";
import {
  ConfigError,
  configPath,
  ensureConfigFile,
  expandTemplate,
  isValidEnvironmentName,
  loadSettings,
  RepoConfig,
  resolveEnvConfig,
  resolveRepoDefaults,
} from "./config.mjs";
import {
  currentInsightsPath,
  ensureCurrentInsights,
  FrontMatter,
  latestRunPath,
  loadQueryFile,
  parseQueryFile,
  resetCurrentInsights,
  runResultPath,
  SeedFrontMatter,
  updateLatestSymlink,
  writeResults,
} from "./query-file.mjs";
import { LinkValues, runLink } from "./link.mjs";
import { ParseLinkValues, runParseLink } from "./parse-link.mjs";
import { RawValues, runRaw } from "./raw.mjs";

const DESCRIPTION = "Download logs from AWS CloudWatch Logs Insights.";

const TIME_DESCRIPTION =
  "time range. Examples: 5h, 30m, 500ms, 13.00-13.01, " +
  "09:15:00.000-09:15:00.500, \"yesterday 17-18\", " +
  "2026-04-22T13:00:00Z/2026-04-22T14:00:00Z, or natural language via chrono-node. " +
  "Defaults to the front-matter `time` if present, otherwise 1h.";

// ---------------------------------------------------------------------------
// query subcommand
// ---------------------------------------------------------------------------

const queryCmd = define({
  name: "query",
  description: "Run a CloudWatch Logs Insights query (opens $EDITOR when no -q/-f is given)",
  args: {
    "log-group": {
      type: "string",
      short: "g",
      multiple: true,
      description: "log group name (repeat or comma-separate; overrides config)",
    },
    time: {
      type: "string",
      short: "t",
      description: TIME_DESCRIPTION,
    },
    query: {
      type: "string",
      short: "q",
      description: "CloudWatch Insights query string (skips editor)",
    },
    "query-file": {
      type: "string",
      short: "f",
      description: "read query from file (use '-' for stdin; skips editor)",
    },
    environment: {
      type: "string",
      short: "e",
      description: "environment name (lower kebab-case); substituted for {{ env }} in the query and log group template",
    },
    limit: {
      type: "number",
      short: "l",
      description: "maximum number of rows to return",
    },
    region: {
      type: "string",
      short: "r",
      description: "AWS region (overrides AWS_REGION)",
    },
    profile: {
      type: "string",
      description: "AWS profile (sets AWS_PROFILE)",
    },
    new: {
      type: "boolean",
      default: false,
      description: "reset current.insights to the default template before opening the editor",
    },
    quiet: {
      type: "boolean",
      default: false,
      description: "suppress progress output on stderr",
    },
  },
  run: async (ctx) => {
    await runQuery(ctx.values as unknown as QueryValues);
  },
});

interface QueryValues {
  "log-group"?: string[];
  time?: string;
  query?: string;
  "query-file"?: string;
  environment?: string;
  limit?: number;
  region?: string;
  profile?: string;
  new: boolean;
  quiet: boolean;
}

async function runQuery(values: QueryValues): Promise<void> {
  if (values.query && values["query-file"]) {
    fail("--query and --query-file are mutually exclusive", 2);
  }

  const settings = loadSettings();
  const { defaults, sectionName } = resolveRepoDefaults(settings);

  // Validate the CLI environment up-front so the seeded front-matter (and any
  // error from an invalid value) appears before we open the editor.
  const cliEnvironment = validateEnvironmentArg(values.environment, "--environment");

  const { queryBody, frontMatter } = await resolveQuerySource(values, defaults, cliEnvironment);

  const environment =
    cliEnvironment ?? validateEnvironmentArg(frontMatter.env, "front-matter `env`");
  const timeExpr = values.time ?? frontMatter.time ?? "1h";
  const limit = values.limit;

  let range;
  try {
    range = parseTimeRange(timeExpr);
  } catch (err) {
    if (err instanceof TimeRangeParseError) fail(err.message, 2);
    throw err;
  }

  const templateVars: Record<string, string | undefined> = {
    env: environment,
    app: frontMatter.app ?? defaults.app,
  };

  const logGroups = resolveLogGroups({
    cliLogGroups: flatten((values["log-group"] ?? []).map((s) => s.split(","))),
    frontMatterLogGroup: frontMatter["log-group"],
    configGroupTemplate: defaults.group,
    templateVars,
    sectionName,
  });

  let expandedQuery: string;
  try {
    expandedQuery = expandTemplate(queryBody, templateVars);
  } catch (err) {
    if (err instanceof ConfigError) fail(err.message, 2);
    throw err;
  }

  const envConfig = environment ? resolveEnvConfig(settings, sectionName, environment) : {};
  const profile = values.profile ?? envConfig.awsProfile;
  const region =
    values.region ?? envConfig.region ?? defaults.region ?? process.env.AWS_REGION;
  if (profile) {
    process.env.AWS_PROFILE = profile;
  }
  const client = new CloudWatchLogsClient({ region });

  if (!values.quiet) {
    if (profile && !values.profile) {
      process.stderr.write(`  AWS_PROFILE: ${profile} (from [env.${environment}])\n`);
    }
    if (!values.region) {
      if (envConfig.region) {
        process.stderr.write(`  AWS region:  ${envConfig.region} (from [env.${environment}])\n`);
      } else if (defaults.region) {
        process.stderr.write(
          `  AWS region:  ${defaults.region} (from ${sectionName ? `[repo.${sectionName}] or [defaults]` : "[defaults]"})\n`,
        );
      }
    }
  }

  if (!values.quiet) {
    process.stderr.write(
      `Querying ${logGroups.length} log group(s) from ${range.startTime.toISOString()} ` +
        `to ${range.endTime.toISOString()}\n`,
    );
    process.stderr.write(`  log groups: ${logGroups.join(", ")}\n`);
  }

  let lastStatus = "";
  const result = await runInsightsQuery({
    client,
    logGroups,
    queryString: expandedQuery,
    startTime: range.startTime,
    endTime: range.endTime,
    limit,
    onStatus: (status) => {
      if (values.quiet || status === lastStatus) return;
      lastStatus = String(status);
      process.stderr.write(`  status: ${status}\n`);
    },
  });

  const flattenFields = defaults.flattenFields ?? [];
  const rows = flattenFields.length === 0
    ? result.rows
    : result.rows.map((r) => flattenRow(r, flattenFields));

  const outPath = runResultPath();
  writeResults({ path: outPath, rows });
  const linkPath = updateLatestSymlink(outPath);

  process.stdout.write(outPath + "\n");

  if (!values.quiet) {
    const { recordsMatched, recordsScanned, bytesScanned } = result.statistics ?? {};
    process.stderr.write(
      `Done. ${result.rows.length} rows written (matched=${recordsMatched ?? "?"} ` +
        `scanned=${recordsScanned ?? "?"} bytes=${bytesScanned ?? "?"}).\n`,
    );
    process.stderr.write(`  ${linkPath} → ${outPath}\n`);
  }
}

interface QuerySource {
  queryBody: string;
  frontMatter: FrontMatter;
}

async function resolveQuerySource(
  values: QueryValues,
  defaults: RepoConfig,
  cliEnvironment: string | undefined,
): Promise<QuerySource> {
  if (values.query) {
    return { queryBody: values.query, frontMatter: {} };
  }
  if (values["query-file"]) {
    const source = values["query-file"] === "-" ? 0 : values["query-file"];
    const raw = readFileSync(source, "utf8");
    // Support front-matter in explicit files too.
    const parsed = parseQueryFile(raw);
    return { queryBody: parsed.body, frontMatter: parsed.frontMatter };
  }

  // Editor flow: pre-fill the front-matter with what we'd actually run, so
  // the user can see and edit it before saving.
  const seedOptions = {
    app: defaults.app,
    frontMatter: buildSeedFrontMatter(values, defaults, cliEnvironment),
  };
  const path = currentInsightsPath();
  if (values.new) {
    resetCurrentInsights(path, seedOptions);
    if (!values.quiet) process.stderr.write(`Reset ${path} to default template\n`);
  } else {
    const seeded = ensureCurrentInsights(path, seedOptions);
    if (seeded && !values.quiet) {
      process.stderr.write(`Seeded ${path}\n`);
    }
  }
  openEditor(path);
  const parsed = loadQueryFile(path);
  if (!parsed.body) {
    fail(`${path} has no query body`, 2);
  }
  return { queryBody: parsed.body, frontMatter: parsed.frontMatter };
}

function buildSeedFrontMatter(
  values: QueryValues,
  defaults: RepoConfig,
  cliEnvironment: string | undefined,
): SeedFrontMatter {
  const cliLogGroups = flatten((values["log-group"] ?? []).map((s) => s.split(",")))
    .map((s) => s.trim())
    .filter(Boolean);

  let logGroup: string | string[] | undefined;
  if (cliLogGroups.length === 1) logGroup = cliLogGroups[0];
  else if (cliLogGroups.length > 1) logGroup = cliLogGroups;
  else if (defaults.group) logGroup = defaults.group;

  return {
    time: values.time ?? "1h",
    env: cliEnvironment,
    app: defaults.app,
    "log-group": logGroup,
  };
}

interface ResolveLogGroupsArgs {
  cliLogGroups: string[];
  frontMatterLogGroup: string | string[] | undefined;
  configGroupTemplate: string | undefined;
  templateVars: Record<string, string | undefined>;
  sectionName: string | null;
}

function resolveLogGroups(args: ResolveLogGroupsArgs): string[] {
  const expand = (templates: string[]): string[] => {
    try {
      return templates.map((t) => expandTemplate(t, args.templateVars));
    } catch (err) {
      if (err instanceof ConfigError) fail(err.message, 2);
      throw err;
    }
  };

  const fromCli = args.cliLogGroups.map((s) => s.trim()).filter(Boolean);
  if (fromCli.length > 0) return expand(fromCli);

  const fromFm = toArray(args.frontMatterLogGroup);
  if (fromFm.length > 0) return expand(fromFm);

  if (args.configGroupTemplate) return expand([args.configGroupTemplate]);

  fail(
    args.sectionName
      ? `no log group given, and config section [${args.sectionName}] has no "group". Set it with \`cloudwatch-insights edit-config\` or pass --log-group.`
      : "no log group given, and no matching config section was found. Pass --log-group or run `cloudwatch-insights edit-config`.",
    2,
  );
}

// ---------------------------------------------------------------------------
// link subcommand
// ---------------------------------------------------------------------------

const linkCmd = define({
  name: "link",
  description:
    "Print a shareable AWS Console URL for the query (no editor, no execution)",
  args: {
    "log-group": {
      type: "string",
      short: "g",
      multiple: true,
      description: "log group name (repeat or comma-separate; overrides config)",
    },
    time: {
      type: "string",
      short: "t",
      description: TIME_DESCRIPTION,
    },
    query: {
      type: "string",
      short: "q",
      description: "CloudWatch Insights query string (skips current.insights)",
    },
    "query-file": {
      type: "string",
      short: "f",
      description: "read query from file (use '-' for stdin; skips current.insights)",
    },
    environment: {
      type: "string",
      short: "e",
      description:
        "environment name (lower kebab-case); substituted for {{ env }} in the query and log group template",
    },
    region: {
      type: "string",
      short: "r",
      description: "AWS region (overrides AWS_REGION)",
    },
    profile: {
      type: "string",
      description: "AWS profile (sets AWS_PROFILE; only used for the STS lookup)",
    },
    "account-id": {
      type: "string",
      description:
        "AWS account ID for the log-group ARN (overrides config; falls back to STS GetCallerIdentity)",
    },
    "preserve-time-window": {
      type: "boolean",
      default: false,
      description:
        "always emit absolute start/end timestamps in the URL, even when -t is a relative duration like 1h",
    },
    open: {
      type: "boolean",
      default: false,
      description: "open the URL in the default browser",
    },
    quiet: {
      type: "boolean",
      default: false,
      description: "suppress diagnostic output on stderr",
    },
  },
  run: async (ctx) => {
    await runLink(ctx.values as unknown as LinkValues);
  },
});

// ---------------------------------------------------------------------------
// parse-link subcommand
// ---------------------------------------------------------------------------

const parseLinkCmd = define({
  name: "parse-link",
  description:
    "Decode an AWS Console Insights URL and recreate the query state (or print a `raw` shell command with --as-raw)",
  args: {
    url: {
      type: "string",
      description: "the URL to decode (alternatively pass it as a positional, or pipe it on stdin)",
    },
    "as-raw": {
      type: "boolean",
      default: false,
      description:
        "print a self-contained `cloudwatch-insights raw` shell command instead of writing current.insights",
    },
    output: {
      type: "string",
      short: "o",
      description:
        "write the .insights file to this path instead of the current slot's current.insights",
    },
    quiet: {
      type: "boolean",
      default: false,
      description: "suppress diagnostic output on stderr",
    },
  },
  run: async (ctx) => {
    // gunshi includes the subcommand name as the first positional; drop it
    // so the user-supplied URL (if any) is at index 0.
    const positionals = ((ctx.positionals ?? []) as string[]).slice(1);
    await runParseLink(ctx.values as unknown as ParseLinkValues, positionals);
  },
});

// ---------------------------------------------------------------------------
// raw subcommand
// ---------------------------------------------------------------------------

const rawCmd = define({
  name: "raw",
  description:
    "Run a query verbatim from a file (no templating, no front-matter, no config); write JSONL to stdout or --output",
  args: {
    "log-group": {
      type: "string",
      short: "g",
      multiple: true,
      description: "log group name (repeat or comma-separate; required)",
    },
    "query-file": {
      type: "string",
      short: "f",
      description: "read query from file (use '-' for stdin; required)",
    },
    time: {
      type: "string",
      short: "t",
      description:
        "time range (same syntax as `query --time`). Defaults to 1h.",
    },
    limit: {
      type: "number",
      short: "l",
      description: "maximum number of rows to return",
    },
    region: {
      type: "string",
      short: "r",
      description: "AWS region (overrides AWS_REGION)",
    },
    profile: {
      type: "string",
      description: "AWS profile (sets AWS_PROFILE)",
    },
    output: {
      type: "string",
      short: "o",
      description: "write JSONL results to this file instead of stdout",
    },
    quiet: {
      type: "boolean",
      default: false,
      description: "suppress progress output on stderr",
    },
  },
  run: async (ctx) => {
    await runRaw(ctx.values as unknown as RawValues);
  },
});

// ---------------------------------------------------------------------------
// show subcommand
// ---------------------------------------------------------------------------

const showCmd = define({
  name: "show",
  description: "Stream the contents of latest-run.jsonl to stdout",
  args: {
    path: {
      type: "boolean",
      default: false,
      description: "print the path to latest-run.jsonl instead of its contents",
    },
  },
  run: async (ctx) => {
    const path = latestRunPath();
    if (!existsSync(path)) {
      fail(`no runs found (${path} does not exist). Run \`cloudwatch-insights query\` first.`, 2);
    }
    if (ctx.values.path) {
      process.stdout.write(path + "\n");
      return;
    }
    await pipeline(createReadStream(path), process.stdout);
  },
});

// ---------------------------------------------------------------------------
// edit-config subcommand
// ---------------------------------------------------------------------------

const editConfigCmd = define({
  name: "edit-config",
  description: "Open settings.toml in $EDITOR, creating it (and a placeholder section for the current repo) if needed",
  args: {},
  run: () => {
    const path = configPath();
    const { fileCreated, addedSection } = ensureConfigFile(path);
    if (fileCreated) process.stderr.write(`Created ${path}\n`);
    if (addedSection) {
      process.stderr.write(
        `Added commented placeholder section [${addedSection}] for the current repository\n`,
      );
    }
    openEditor(path);
  },
});

// ---------------------------------------------------------------------------
// shared helpers
// ---------------------------------------------------------------------------

function openEditor(path: string): void {
  const editor = process.env.VISUAL || process.env.EDITOR;
  if (!editor) {
    fail("no editor set — define $EDITOR (or $VISUAL) and retry", 2);
  }
  const parts = editor.split(/\s+/).filter(Boolean);
  const cmd = parts[0];
  const args = [...parts.slice(1), path];
  const result = spawnSync(cmd, args, { stdio: "inherit" });
  if (result.error) {
    fail(`failed to launch editor (${editor}): ${result.error.message}`, 1);
  }
  if (typeof result.status === "number" && result.status !== 0) {
    process.exit(result.status);
  }
}

function validateEnvironmentArg(raw: string | undefined, source: string): string | undefined {
  if (raw === undefined) return undefined;
  if (!isValidEnvironmentName(raw)) {
    fail(
      `${source} must be lower kebab-case (e.g. "prod", "us-east-1") — got ${JSON.stringify(raw)}`,
      2,
    );
  }
  return raw;
}

function toArray<T>(value: T | T[] | undefined): T[] {
  if (value === undefined) return [];
  return Array.isArray(value) ? value : [value];
}

function flatten<T>(arrays: T[][]): T[] {
  const out: T[] = [];
  for (const a of arrays) out.push(...a);
  return out;
}

function fail(message: string, code = 1): never {
  process.stderr.write(`error: ${message}\n`);
  process.exit(code);
}

// ---------------------------------------------------------------------------
// main entry: unknown commands and `cloudwatch-insights` with no args both
// fall through to this, which prints a hint.
// ---------------------------------------------------------------------------

const main = define({
  name: "cloudwatch-insights",
  description: DESCRIPTION,
  args: {},
  run: () => {
    process.stderr.write(
      "Usage: cloudwatch-insights <query|raw|link|parse-link|show|edit-config>\n" +
        "Run `cloudwatch-insights --help` for details.\n",
    );
    process.exit(2);
  },
});

function rewriteEnvAlias(argv: string[]): string[] {
  return argv.map((a) => {
    if (a === "--env") return "--environment";
    if (a.startsWith("--env=")) return "--environment=" + a.slice("--env=".length);
    return a;
  });
}

try {
  await cli(rewriteEnvAlias(process.argv.slice(2)), main, {
    name: "cloudwatch-insights",
    version: "1.0.0",
    description: DESCRIPTION,
    renderHeader: null,
    subCommands: {
      query: queryCmd,
      raw: rawCmd,
      link: linkCmd,
      "parse-link": parseLinkCmd,
      show: showCmd,
      "edit-config": editConfigCmd,
    },
  });
} catch (err) {
  const message = err instanceof Error ? err.message : String(err);
  process.stderr.write(`error: ${message}\n`);
  process.exit(1);
}
