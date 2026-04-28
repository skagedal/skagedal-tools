/**
 * `cloudwatch-insights parse-link` — inverse of the `link` subcommand.
 *
 * Default behavior: write the query body, log groups, time and (optional)
 * region into `current.insights` for the current slot, so a subsequent
 * `cloudwatch-insights query` would re-run the same query.
 *
 * With `--as-raw`: print a self-contained `cloudwatch-insights raw …`
 * shell command (a heredoc piping the query body into stdin) and do not
 * touch any state files. Useful for sharing a one-off invocation.
 */

import { mkdirSync, writeFileSync } from "fs";
import { dirname } from "path";
import { stringify as stringifyToml } from "smol-toml";

import {
  parseConsoleLink,
  parseLogGroupArn,
  RisonValue,
} from "./console-link.mjs";
import { currentInsightsPath } from "./query-file.mjs";

export interface ParseLinkValues {
  url?: string;
  "as-raw": boolean;
  output?: string;
  quiet: boolean;
}

export interface ParsedLinkState {
  region: string;
  logGroups: string[];
  time: string;
  query: string;
}

export async function runParseLink(
  values: ParseLinkValues,
  positionals: string[],
): Promise<void> {
  const url = await resolveUrl(values, positionals);
  const state = parseLinkToState(url);

  if (values["as-raw"]) {
    process.stdout.write(buildRawCommand(state) + "\n");
    return;
  }

  const path = values.output ?? currentInsightsPath();
  writeInsightsFile(path, state);
  if (!values.quiet) {
    process.stderr.write(`Wrote ${path}\n`);
    process.stderr.write(`  region:     ${state.region}\n`);
    process.stderr.write(`  log groups: ${state.logGroups.join(", ")}\n`);
    process.stderr.write(`  time:       ${state.time}\n`);
  }
}

/** Decode an AWS Console link into the values needed to recreate the query. */
export function parseLinkToState(url: string): ParsedLinkState {
  const { region, queryDetail } = parseConsoleLink(url);

  const editorString = queryDetail.editorString;
  if (typeof editorString !== "string") {
    throw new Error("queryDetail.editorString is missing or not a string");
  }

  const source = queryDetail.source;
  if (!Array.isArray(source) || source.length === 0) {
    throw new Error("queryDetail.source is missing or empty");
  }
  const logGroups = source.map((entry, i) => {
    if (typeof entry !== "string") {
      throw new Error(`queryDetail.source[${i}] is not a string`);
    }
    // The Console emits either full log-group ARNs (when queryBy=logGroupArn)
    // or bare log-group names (when queryBy=logGroupName). Accept both.
    if (entry.startsWith("arn:")) {
      const parsed = parseLogGroupArn(entry);
      if (!parsed) {
        throw new Error(`queryDetail.source[${i}] is not a log-group ARN: ${entry}`);
      }
      return parsed.logGroupName;
    }
    return entry;
  });

  return {
    region,
    logGroups,
    time: timeFromQueryDetail(queryDetail),
    query: editorString,
  };
}

function timeFromQueryDetail(detail: Record<string, RisonValue>): string {
  const timeType = detail.timeType;
  if (timeType === "RELATIVE") {
    const start = detail.start;
    if (typeof start !== "number") {
      throw new Error("RELATIVE time has non-numeric start");
    }
    const unit = detail.unit;
    const seconds = unit === "milliseconds" ? Math.round(-start / 1000) : -start;
    return formatRelativeDuration(seconds);
  }
  if (timeType === "ABSOLUTE") {
    const start = detail.start;
    const end = detail.end;
    if (typeof start !== "number" || typeof end !== "number") {
      throw new Error("ABSOLUTE time has non-numeric start/end");
    }
    return `${new Date(start).toISOString()}/${new Date(end).toISOString()}`;
  }
  throw new Error(`unknown timeType: ${JSON.stringify(timeType)}`);
}

/**
 * Format a whole number of seconds as the largest exact unit understood by
 * the time-range parser: w/d/h/m, falling back to s when nothing divides
 * evenly. (1h beats 60m beats 3600s.)
 */
export function formatRelativeDuration(totalSeconds: number): string {
  if (!Number.isFinite(totalSeconds) || totalSeconds < 0) {
    throw new Error(`invalid duration: ${totalSeconds}`);
  }
  if (totalSeconds === 0) return "0s";
  const units: Array<[string, number]> = [
    ["w", 7 * 86_400],
    ["d", 86_400],
    ["h", 3_600],
    ["m", 60],
    ["s", 1],
  ];
  for (const [suffix, secs] of units) {
    if (totalSeconds % secs === 0) {
      return `${totalSeconds / secs}${suffix}`;
    }
  }
  return `${totalSeconds}s`;
}

function writeInsightsFile(path: string, state: ParsedLinkState): void {
  mkdirSync(dirname(path), { recursive: true });
  const fm: Record<string, unknown> = {
    time: state.time,
    "log-group": state.logGroups.length === 1 ? state.logGroups[0] : state.logGroups,
  };
  const contents = `${stringifyToml(fm)}\n---\n${state.query}\n`;
  writeFileSync(path, contents, "utf8");
}

/**
 * Build a single-line shell command that runs the same query via `raw`,
 * piping the query body in via stdin (a quoted heredoc, so $… and backslash
 * escapes inside the query are preserved verbatim).
 */
export function buildRawCommand(state: ParsedLinkState): string {
  const args = ["cloudwatch-insights", "raw"];
  args.push("-r", shellQuote(state.region));
  for (const lg of state.logGroups) {
    args.push("-g", shellQuote(lg));
  }
  args.push("-t", shellQuote(state.time));
  args.push("-f", "-");
  // Quoted heredoc ('EOF') so the body is not subject to shell expansion.
  return `${args.join(" ")} <<'EOF'\n${state.query}\nEOF`;
}

function shellQuote(s: string): string {
  if (s === "") return "''";
  // Safe set: alphanumeric plus a handful of innocuous punctuation that the
  // shell never expands.
  if (/^[A-Za-z0-9_./:@%+,=-]+$/.test(s)) return s;
  return `'${s.replace(/'/g, "'\\''")}'`;
}

async function resolveUrl(
  values: ParseLinkValues,
  positionals: string[],
): Promise<string> {
  if (values.url && positionals.length > 0) {
    fail("pass either --url or a positional URL, not both", 2);
  }
  let url = values.url ?? positionals[0];
  if (!url) {
    // Read from stdin as a fallback so users can pipe pbpaste / xclip into us.
    if (!process.stdin.isTTY) {
      url = await readAllStdin();
    }
  }
  if (!url) {
    fail("no URL given — pass it as the first argument, --url, or via stdin", 2);
  }
  return url.trim();
}

async function readAllStdin(): Promise<string> {
  return new Promise((resolve, reject) => {
    let data = "";
    process.stdin.setEncoding("utf8");
    process.stdin.on("data", (chunk) => {
      data += chunk;
    });
    process.stdin.on("end", () => resolve(data));
    process.stdin.on("error", reject);
  });
}

function fail(message: string, code = 1): never {
  process.stderr.write(`error: ${message}\n`);
  process.exit(code);
}
