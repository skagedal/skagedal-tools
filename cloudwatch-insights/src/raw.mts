/**
 * `cloudwatch-insights raw` — execute a query taken verbatim from a file.
 *
 * No templating, no front-matter parsing, no config lookup, no flatten-fields
 * post-processing, no editor, no run-results bookkeeping. The file's contents
 * are sent to CloudWatch as-is, and the resulting JSONL is written to stdout
 * (or to `--output` if given).
 */

import { createWriteStream, readFileSync } from "fs";
import { CloudWatchLogsClient } from "@aws-sdk/client-cloudwatch-logs";

import { runInsightsQuery } from "./insights.mjs";
import { parseTimeRange, TimeRangeParseError } from "./time-range.mjs";

export interface RawValues {
  "log-group"?: string[];
  time?: string;
  "query-file"?: string;
  limit?: number;
  region?: string;
  profile?: string;
  output?: string;
  quiet: boolean;
}

export async function runRaw(values: RawValues): Promise<void> {
  if (!values["query-file"]) {
    fail("--query-file/-f is required", 2);
  }
  const logGroups = flatten((values["log-group"] ?? []).map((s) => s.split(",")))
    .map((s) => s.trim())
    .filter(Boolean);
  if (logGroups.length === 0) {
    fail("--log-group/-g is required (repeat or comma-separate)", 2);
  }

  const source = values["query-file"] === "-" ? 0 : values["query-file"]!;
  const queryString = readFileSync(source, "utf8");

  const timeExpr = values.time ?? "1h";
  let range;
  try {
    range = parseTimeRange(timeExpr);
  } catch (err) {
    if (err instanceof TimeRangeParseError) fail(err.message, 2);
    throw err;
  }

  if (values.profile) {
    process.env.AWS_PROFILE = values.profile;
  }
  const region = values.region ?? process.env.AWS_REGION;
  const client = new CloudWatchLogsClient({ region });

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
    queryString,
    startTime: range.startTime,
    endTime: range.endTime,
    limit: values.limit,
    onProgress: ({ status }) => {
      if (values.quiet || status === lastStatus) return;
      lastStatus = String(status);
      process.stderr.write(`  status: ${status}\n`);
    },
  });

  const contents = result.rows.map((r) => JSON.stringify(r) + "\n").join("");
  if (values.output) {
    await new Promise<void>((resolve, reject) => {
      const stream = createWriteStream(values.output!, { encoding: "utf8" });
      stream.on("error", reject);
      stream.on("finish", resolve);
      stream.end(contents);
    });
  } else {
    process.stdout.write(contents);
  }

  if (!values.quiet) {
    const { recordsMatched, recordsScanned, bytesScanned } = result.statistics ?? {};
    process.stderr.write(
      `Done. ${result.rows.length} rows written (matched=${recordsMatched ?? "?"} ` +
        `scanned=${recordsScanned ?? "?"} bytes=${bytesScanned ?? "?"}).\n`,
    );
  }
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
