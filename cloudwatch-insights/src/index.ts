#!/usr/bin/env node

import { readFileSync } from "fs";
import { spawnSync } from "child_process";
import { cli, define } from "gunshi";
import { CloudWatchLogsClient } from "@aws-sdk/client-cloudwatch-logs";

import { parseTimeRange, TimeRangeParseError } from "./time-range.js";
import { runInsightsQuery, QueryRow } from "./insights.js";
import {
  applyEnvironment,
  ConfigError,
  configPath,
  defaultQueryForApp,
  ensureConfigFile,
  Environment,
  ENVIRONMENTS,
  loadSettings,
  resolveRepoDefaults,
} from "./config.js";

type OutputFormat = "jsonl" | "json";
const OUTPUT_FORMATS: readonly OutputFormat[] = ["jsonl", "json"];

const FALLBACK_QUERY = "fields @timestamp, @message | sort @timestamp desc";

const DESCRIPTION = "Download logs from AWS CloudWatch Logs Insights.";

const TIME_DESCRIPTION =
  "time range (defaults to 1h). Examples: 5h, 30m, 500ms, 13.00-13.01, " +
  "09:15:00.000-09:15:00.500, \"yesterday 17-18\", " +
  "2026-04-22T13:00:00Z/2026-04-22T14:00:00Z, or natural language via chrono-node";

const command = define({
  name: "cloudwatch-insights",
  description: DESCRIPTION,
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
      default: "1h",
      description: TIME_DESCRIPTION,
    },
    query: {
      type: "string",
      short: "q",
      description: "CloudWatch Insights query string",
    },
    "query-file": {
      type: "string",
      short: "f",
      description: "read query from file (use '-' for stdin)",
    },
    app: {
      type: "string",
      description: "filter by `app` field (overrides config's app)",
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
    output: {
      type: "string",
      short: "o",
      default: "jsonl",
      description: "output format: jsonl | json",
    },
    quiet: {
      type: "boolean",
      default: false,
      description: "suppress progress output on stderr",
    },
  },
  run: async (ctx) => {
    await runCommand(ctx.values);
  },
});

interface Values {
  "log-group"?: string[];
  time: string;
  query?: string;
  "query-file"?: string;
  app?: string;
  environment?: string;
  limit?: number;
  region?: string;
  profile?: string;
  output: string;
  quiet: boolean;
}

async function runCommand(raw: Record<string, unknown>): Promise<void> {
  const values = raw as unknown as Values;

  const output = validateChoice("output", values.output, OUTPUT_FORMATS) as OutputFormat;
  const environment = values.environment
    ? (validateChoice("environment", values.environment, ENVIRONMENTS) as Environment)
    : undefined;
  const logGroupsRaw = flatten((values["log-group"] ?? []).map((s) => s.split(",")));
  if (values.limit !== undefined && (!Number.isFinite(values.limit) || values.limit <= 0)) {
    fail("--limit must be a positive integer", 2);
  }

  let range;
  try {
    range = parseTimeRange(values.time);
  } catch (err) {
    if (err instanceof TimeRangeParseError) fail(err.message, 2);
    throw err;
  }

  let plan: ResolvedPlan;
  try {
    plan = resolvePlan({
      logGroupsRaw,
      query: values.query,
      queryFile: values["query-file"],
      app: values.app,
      environment,
    });
  } catch (err) {
    if (err instanceof ConfigError || err instanceof Error) fail(err.message, 2);
    throw err;
  }

  if (values.profile) {
    process.env.AWS_PROFILE = values.profile;
  }

  const client = new CloudWatchLogsClient({
    region: values.region ?? process.env.AWS_REGION,
  });

  if (!values.quiet) {
    process.stderr.write(
      `Querying ${plan.logGroups.length} log group(s) from ${range.startTime.toISOString()} ` +
        `to ${range.endTime.toISOString()}\n`,
    );
    process.stderr.write(`  log groups: ${plan.logGroups.join(", ")} (${plan.source.logGroups})\n`);
    process.stderr.write(`  query source: ${plan.source.query}\n`);
  }

  let lastStatus = "";
  const result = await runInsightsQuery({
    client,
    logGroups: plan.logGroups,
    queryString: plan.query,
    startTime: range.startTime,
    endTime: range.endTime,
    limit: values.limit,
    onStatus: (status) => {
      if (values.quiet || status === lastStatus) return;
      lastStatus = String(status);
      process.stderr.write(`  status: ${status}\n`);
    },
  });

  renderRows(result.rows, output);

  if (!values.quiet && result.statistics) {
    const { recordsMatched, recordsScanned, bytesScanned } = result.statistics;
    process.stderr.write(
      `Done. ${result.rows.length} rows returned ` +
        `(matched=${recordsMatched ?? "?"} scanned=${recordsScanned ?? "?"} bytes=${bytesScanned ?? "?"}).\n`,
    );
  }
}

interface ResolvedPlan {
  logGroups: string[];
  query: string;
  source: {
    logGroups: "cli" | "config";
    query: "cli" | "query-file" | "config-app" | "default";
  };
}

interface ResolveArgs {
  logGroupsRaw: string[];
  query: string | undefined;
  queryFile: string | undefined;
  app: string | undefined;
  environment: Environment | undefined;
}

function resolvePlan(args: ResolveArgs): ResolvedPlan {
  if (args.query && args.queryFile) {
    throw new Error("--query and --query-file are mutually exclusive");
  }

  const settings = loadSettings();
  const { defaults, sectionName } = resolveRepoDefaults(settings);

  let logGroups = args.logGroupsRaw.map((s) => s.trim()).filter(Boolean);
  let logGroupsSource: "cli" | "config" = "cli";
  if (logGroups.length === 0) {
    if (defaults.group) {
      logGroups = [applyEnvironment(defaults.group, args.environment)];
      logGroupsSource = "config";
    } else {
      throw new Error(
        sectionName
          ? `no --log-group given, and config section [${sectionName}] has no "group"`
          : "no --log-group given, and no matching config section was found",
      );
    }
  } else if (args.environment) {
    logGroups = logGroups.map((g) => applyEnvironment(g, args.environment));
  }

  let query: string;
  let querySource: ResolvedPlan["source"]["query"];
  if (args.query) {
    query = args.query;
    querySource = "cli";
  } else if (args.queryFile) {
    const source = args.queryFile === "-" ? 0 : args.queryFile;
    query = readFileSync(source, "utf8");
    querySource = "query-file";
  } else {
    const app = args.app ?? defaults.app;
    if (app) {
      query = defaultQueryForApp(app);
      querySource = "config-app";
    } else {
      query = FALLBACK_QUERY;
      querySource = "default";
    }
  }

  return {
    logGroups,
    query,
    source: { logGroups: logGroupsSource, query: querySource },
  };
}

function renderRows(rows: QueryRow[], format: OutputFormat): void {
  switch (format) {
    case "jsonl":
      for (const row of rows) {
        process.stdout.write(JSON.stringify(row) + "\n");
      }
      break;
    case "json":
      process.stdout.write(JSON.stringify(rows, null, 2) + "\n");
      break;
  }
}

function validateChoice<T extends string>(
  name: string,
  value: unknown,
  choices: readonly T[],
): T {
  if (typeof value !== "string" || !choices.includes(value as T)) {
    fail(`--${name} must be one of: ${choices.join(", ")} (got ${JSON.stringify(value)})`, 2);
  }
  return value as T;
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

const EDIT_CONFIG_HELP =
  "Usage: cloudwatch-insights edit-config\n\n" +
  "Open the settings.toml in $EDITOR (or $VISUAL), creating it with a\n" +
  "commented template if it does not yet exist.\n";

function runEditConfig(argv: string[]): void {
  if (argv.includes("--help") || argv.includes("-h")) {
    process.stdout.write(EDIT_CONFIG_HELP);
    return;
  }

  const path = configPath();
  const { fileCreated, addedSection } = ensureConfigFile(path);
  if (fileCreated) {
    process.stderr.write(`Created ${path}\n`);
  }
  if (addedSection) {
    process.stderr.write(`Added commented placeholder section [${addedSection}] for the current repository\n`);
  }

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

const argv = process.argv.slice(2);

// edit-config is handled manually rather than through gunshi's subCommands,
// because gunshi routes the first non-flag token to subcommand dispatch before
// consuming flag values — that turns "-t 5m" into "Command not found: 5m".
if (argv[0] === "edit-config") {
  runEditConfig(argv.slice(1));
  process.exit(0);
}

try {
  await cli(argv, command, {
    name: "cloudwatch-insights",
    version: "1.0.0",
    description: DESCRIPTION,
    renderHeader: null,
  });
} catch (err) {
  const message = err instanceof Error ? err.message : String(err);
  process.stderr.write(`error: ${message}\n`);
  process.exit(1);
}
