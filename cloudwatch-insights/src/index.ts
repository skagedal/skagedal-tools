#!/usr/bin/env node

import { createReadStream, existsSync, readFileSync } from "fs";
import { spawnSync } from "child_process";
import { pipeline } from "stream/promises";
import { cli, define } from "gunshi";
import { CloudWatchLogsClient } from "@aws-sdk/client-cloudwatch-logs";

import { parseTimeRange, TimeRangeParseError } from "./time-range.js";
import { runInsightsQuery } from "./insights.js";
import {
  applyEnvironment,
  ConfigError,
  configPath,
  ensureConfigFile,
  Environment,
  ENVIRONMENTS,
  loadSettings,
  resolveRepoDefaults,
} from "./config.js";
import {
  currentInsightsPath,
  ensureCurrentInsights,
  FrontMatter,
  latestRunPath,
  loadQueryFile,
  parseQueryFile,
  runResultPath,
  updateLatestSymlink,
  writeResults,
} from "./query-file.js";

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
      description: "substituted for {env} in the log group template: systest | uat | prod",
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
  quiet: boolean;
}

async function runQuery(values: QueryValues): Promise<void> {
  if (values.query && values["query-file"]) {
    fail("--query and --query-file are mutually exclusive", 2);
  }

  const settings = loadSettings();
  const { defaults, sectionName } = resolveRepoDefaults(settings);

  const { queryBody, frontMatter } = await resolveQuerySource(values, defaults.app);

  const environment = pickEnvironment(values.environment ?? frontMatter.environment);
  const timeExpr = values.time ?? frontMatter.time ?? "1h";
  const limit = values.limit ?? frontMatter.limit;

  let range;
  try {
    range = parseTimeRange(timeExpr);
  } catch (err) {
    if (err instanceof TimeRangeParseError) fail(err.message, 2);
    throw err;
  }

  const logGroups = resolveLogGroups({
    cliLogGroups: flatten((values["log-group"] ?? []).map((s) => s.split(","))),
    frontMatterLogGroup: frontMatter.logGroup,
    configGroupTemplate: defaults.group,
    environment,
    sectionName,
  });

  if (values.profile) {
    process.env.AWS_PROFILE = values.profile;
  }
  const client = new CloudWatchLogsClient({
    region: values.region ?? process.env.AWS_REGION,
  });

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
    queryString: queryBody,
    startTime: range.startTime,
    endTime: range.endTime,
    limit,
    onStatus: (status) => {
      if (values.quiet || status === lastStatus) return;
      lastStatus = String(status);
      process.stderr.write(`  status: ${status}\n`);
    },
  });

  const outPath = runResultPath();
  writeResults({ path: outPath, rows: result.rows });
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
  configApp: string | undefined,
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

  // Editor flow
  const path = currentInsightsPath();
  const seeded = ensureCurrentInsights(path, configApp);
  if (seeded && !values.quiet) {
    process.stderr.write(`Seeded ${path}\n`);
  }
  openEditor(path);
  const parsed = loadQueryFile(path);
  if (!parsed.body) {
    fail(`${path} has no query body`, 2);
  }
  return { queryBody: parsed.body, frontMatter: parsed.frontMatter };
}

interface ResolveLogGroupsArgs {
  cliLogGroups: string[];
  frontMatterLogGroup: string | string[] | undefined;
  configGroupTemplate: string | undefined;
  environment: Environment | undefined;
  sectionName: string | null;
}

function resolveLogGroups(args: ResolveLogGroupsArgs): string[] {
  const fromCli = args.cliLogGroups.map((s) => s.trim()).filter(Boolean);
  if (fromCli.length > 0) {
    try {
      return fromCli.map((g) => applyEnvironment(g, args.environment));
    } catch (err) {
      if (err instanceof ConfigError) fail(err.message, 2);
      throw err;
    }
  }
  const fromFm = toArray(args.frontMatterLogGroup);
  if (fromFm.length > 0) {
    try {
      return fromFm.map((g) => applyEnvironment(g, args.environment));
    } catch (err) {
      if (err instanceof ConfigError) fail(err.message, 2);
      throw err;
    }
  }
  if (args.configGroupTemplate) {
    try {
      return [applyEnvironment(args.configGroupTemplate, args.environment)];
    } catch (err) {
      if (err instanceof ConfigError) fail(err.message, 2);
      throw err;
    }
  }
  fail(
    args.sectionName
      ? `no log group given, and config section [${args.sectionName}] has no "group". Set it with \`cloudwatch-insights edit-config\` or pass --log-group.`
      : "no log group given, and no matching config section was found. Pass --log-group or run `cloudwatch-insights edit-config`.",
    2,
  );
}

// ---------------------------------------------------------------------------
// show subcommand
// ---------------------------------------------------------------------------

const showCmd = define({
  name: "show",
  description: "Stream the contents of latest-run.jsonl to stdout",
  args: {},
  run: async () => {
    const path = latestRunPath();
    if (!existsSync(path)) {
      fail(`no runs found (${path} does not exist). Run \`cloudwatch-insights query\` first.`, 2);
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

function pickEnvironment(raw: string | undefined): Environment | undefined {
  if (raw === undefined) return undefined;
  if (!ENVIRONMENTS.includes(raw as Environment)) {
    fail(`--environment must be one of: ${ENVIRONMENTS.join(", ")} (got ${JSON.stringify(raw)})`, 2);
  }
  return raw as Environment;
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
      "Usage: cloudwatch-insights <query|show|edit-config>\n" +
        "Run `cloudwatch-insights --help` for details.\n",
    );
    process.exit(2);
  },
});

try {
  await cli(process.argv.slice(2), main, {
    name: "cloudwatch-insights",
    version: "1.0.0",
    description: DESCRIPTION,
    renderHeader: null,
    subCommands: {
      query: queryCmd,
      show: showCmd,
      "edit-config": editConfigCmd,
    },
  });
} catch (err) {
  const message = err instanceof Error ? err.message : String(err);
  process.stderr.write(`error: ${message}\n`);
  process.exit(1);
}
