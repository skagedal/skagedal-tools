#!/usr/bin/env node

import { readFileSync } from "fs";
import { Command, InvalidArgumentError, Option } from "commander";
import { CloudWatchLogsClient } from "@aws-sdk/client-cloudwatch-logs";

import { parseTimeRange, TimeRangeParseError } from "./time-range";
import { runInsightsQuery, QueryRow } from "./insights";
import {
  applyEnvironment,
  ConfigError,
  configPath,
  defaultQueryForApp,
  Environment,
  ENVIRONMENTS,
  loadSettings,
  resolveRepoDefaults,
} from "./config";

type OutputFormat = "ndjson" | "json" | "tsv";

interface CliOptions {
  logGroup: string[];
  time: string;
  query?: string;
  queryFile?: string;
  limit?: number;
  region?: string;
  profile?: string;
  environment?: Environment;
  app?: string;
  output: OutputFormat;
  quiet: boolean;
}

const FALLBACK_QUERY = "fields @timestamp, @message | sort @timestamp desc";

function parsePositiveInt(raw: string): number {
  const n = parseInt(raw, 10);
  if (!Number.isFinite(n) || n <= 0) {
    throw new InvalidArgumentError("expected a positive integer");
  }
  return n;
}

function parseOutput(raw: string): OutputFormat {
  if (raw !== "ndjson" && raw !== "json" && raw !== "tsv") {
    throw new InvalidArgumentError(`unknown output format: ${raw}`);
  }
  return raw;
}

function buildProgram(): Command {
  const program = new Command();
  program
    .name("cloudwatch-insights")
    .description(
      "Download logs from AWS CloudWatch Logs Insights.\n\n" +
        "Time range examples:\n" +
        "  5h                                last 5 hours\n" +
        "  30m                               last 30 minutes\n" +
        "  500ms                             last 500 milliseconds\n" +
        "  13.00-13.01                       time range today\n" +
        "  09:15:00.000-09:15:00.500         millisecond precision today\n" +
        "  yesterday 17-18                   time range on a named day\n" +
        "  2026-04-22T13:00:00Z/2026-04-22T14:00:00Z   explicit ISO range\n" +
        "  \"last Monday 9am to 5pm\"          natural language (chrono-node)\n\n" +
        "Defaults are read per git repository from ~/.config/cloudwatch-insights/settings.toml.\n" +
        'Example: [my-repo]\n         group = "/{env}/team-icc"\n         app   = "installer-notification"\n',
    )
    .option(
      "-g, --log-group <name...>",
      "log group name (repeat or comma-separate for multiple; overrides config)",
      (value: string, previous: string[] = []) =>
        previous.concat(value.split(",").map((s) => s.trim()).filter(Boolean)),
      [] as string[],
    )
    .requiredOption("-t, --time <range>", "time range (see examples above)")
    .option("-q, --query <string>", "CloudWatch Insights query string")
    .option("-f, --query-file <path>", "read query from file (use '-' for stdin)")
    .option("--app <name>", "filter by `app` field (overrides config's app)")
    .addOption(
      new Option(
        "-e, --environment <env>",
        "substituted for {env} in the log group template",
      ).choices([...ENVIRONMENTS]),
    )
    .option<number>("-l, --limit <n>", "maximum number of rows to return", parsePositiveInt)
    .option("-r, --region <region>", "AWS region (overrides AWS_REGION)")
    .option("--profile <name>", "AWS profile (sets AWS_PROFILE)")
    .option<OutputFormat>(
      "-o, --output <format>",
      "output format: ndjson, json, tsv",
      parseOutput,
      "ndjson" as OutputFormat,
    )
    .option("--quiet", "suppress progress output on stderr", false);
  return program;
}

function readQueryFile(path: string): string {
  const source = path === "-" ? 0 : path;
  return readFileSync(source, "utf8");
}

interface ResolvedPlan {
  logGroups: string[];
  query: string;
  source: {
    logGroups: "cli" | "config";
    query: "cli" | "query-file" | "config-app" | "default";
  };
}

function resolvePlan(options: CliOptions): ResolvedPlan {
  if (options.query && options.queryFile) {
    throw new Error("--query and --query-file are mutually exclusive");
  }

  const settings = loadSettings();
  const { defaults, sectionName } = resolveRepoDefaults(settings);

  let logGroups = options.logGroup;
  let logGroupsSource: "cli" | "config" = "cli";
  if (logGroups.length === 0) {
    if (defaults.group) {
      logGroups = [applyEnvironment(defaults.group, options.environment)];
      logGroupsSource = "config";
    } else {
      throw new Error(
        sectionName
          ? `no --log-group given, and config section [${sectionName}] has no "group"`
          : "no --log-group given, and no matching config section was found",
      );
    }
  } else if (options.environment) {
    logGroups = logGroups.map((g) => applyEnvironment(g, options.environment));
  }

  let query: string;
  let querySource: ResolvedPlan["source"]["query"];
  if (options.query) {
    query = options.query;
    querySource = "cli";
  } else if (options.queryFile) {
    query = readQueryFile(options.queryFile);
    querySource = "query-file";
  } else {
    const app = options.app ?? defaults.app;
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
    case "ndjson":
      for (const row of rows) {
        process.stdout.write(JSON.stringify(row) + "\n");
      }
      break;
    case "json":
      process.stdout.write(JSON.stringify(rows, null, 2) + "\n");
      break;
    case "tsv": {
      if (rows.length === 0) return;
      const columns = collectColumns(rows);
      process.stdout.write(columns.join("\t") + "\n");
      for (const row of rows) {
        const line = columns.map((c) => escapeTsv(row[c] ?? "")).join("\t");
        process.stdout.write(line + "\n");
      }
      break;
    }
  }
}

function collectColumns(rows: QueryRow[]): string[] {
  const seen = new Set<string>();
  const order: string[] = [];
  for (const row of rows) {
    for (const key of Object.keys(row)) {
      if (key === "@ptr") continue;
      if (!seen.has(key)) {
        seen.add(key);
        order.push(key);
      }
    }
  }
  return order;
}

function escapeTsv(value: string): string {
  return value.replace(/\t/g, " ").replace(/\r?\n/g, " ");
}

async function main(): Promise<void> {
  const program = buildProgram();
  program.parse(process.argv);
  const options = program.opts<CliOptions>();

  let range;
  try {
    range = parseTimeRange(options.time);
  } catch (err) {
    if (err instanceof TimeRangeParseError) {
      process.stderr.write(`error: ${err.message}\n`);
      process.exit(2);
    }
    throw err;
  }

  let plan: ResolvedPlan;
  try {
    plan = resolvePlan(options);
  } catch (err) {
    if (err instanceof ConfigError || err instanceof Error) {
      process.stderr.write(`error: ${err.message}\n`);
      process.exit(2);
    }
    throw err;
  }

  if (options.profile) {
    process.env.AWS_PROFILE = options.profile;
  }

  const client = new CloudWatchLogsClient({
    region: options.region ?? process.env.AWS_REGION,
  });

  if (!options.quiet) {
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
    limit: options.limit,
    onStatus: (status) => {
      if (options.quiet || status === lastStatus) return;
      lastStatus = String(status);
      process.stderr.write(`  status: ${status}\n`);
    },
  });

  renderRows(result.rows, options.output);

  if (!options.quiet && result.statistics) {
    const { recordsMatched, recordsScanned, bytesScanned } = result.statistics;
    process.stderr.write(
      `Done. ${result.rows.length} rows returned ` +
        `(matched=${recordsMatched ?? "?"} scanned=${recordsScanned ?? "?"} bytes=${bytesScanned ?? "?"}).\n`,
    );
  }
}

// Exported so the config file path is easy to discover from --help-ish commands.
export { configPath };

main().catch((err: unknown) => {
  const message = err instanceof Error ? err.message : String(err);
  process.stderr.write(`error: ${message}\n`);
  process.exit(1);
});
