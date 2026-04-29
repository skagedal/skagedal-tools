/**
 * `cloudwatch-insights paste-link` — inverse of the `copy-link` subcommand.
 *
 * Default behavior: read a CloudWatch Console URL from the system pasteboard
 * and write the query body, log groups, time and (optional) region into
 * `current.insights` for the current slot, so a subsequent
 * `cloudwatch-insights query` would re-run the same query.
 *
 * A positional argument overrides the pasteboard read; -p/--prompt asks for
 * the URL on stdin instead.
 *
 * With `--as-raw`: print a self-contained `cloudwatch-insights raw …`
 * shell command (a heredoc piping the query body into stdin) and do not
 * touch any state files. Useful for sharing a one-off invocation.
 */

import { mkdirSync, writeFileSync } from "fs";
import { dirname } from "path";
import { createInterface } from "readline";
import { stringify as stringifyToml } from "smol-toml";

import {
  parseConsoleLink,
  parseLogGroupArn,
  RisonValue,
} from "./console-link.mjs";
import { currentInsightsPath } from "./query-file.mjs";
import { readFromPasteboard, PasteboardError } from "./pasteboard.mjs";

export interface PasteLinkValues {
  "as-raw": boolean;
  output?: string;
  prompt: boolean;
  quiet: boolean;
}

export interface ParsedLinkState {
  region: string;
  logGroups: string[];
  time: string;
  query: string;
}

export async function runPasteLink(
  values: PasteLinkValues,
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
  values: PasteLinkValues,
  positionals: string[],
): Promise<string> {
  if (positionals.length > 1) {
    fail("paste-link takes at most one positional URL", 2);
  }

  if (positionals.length === 1) {
    if (values.prompt) {
      fail("--prompt cannot be combined with a positional URL", 2);
    }
    return positionals[0].trim();
  }

  if (values.prompt) {
    return await promptForUrl();
  }

  let url: string;
  try {
    url = await readFromPasteboard();
  } catch (err) {
    if (err instanceof PasteboardError) {
      fail(`${err.message}\n  hint: pass the URL as a positional argument, or use --prompt`, 1);
    }
    throw err;
  }
  url = url.trim();
  if (!url) {
    fail("pasteboard is empty — pass the URL as a positional argument, or use --prompt", 2);
  }
  return url;
}

async function promptForUrl(): Promise<string> {
  const rl = createInterface({ input: process.stdin, output: process.stderr });
  try {
    return await new Promise<string>((resolve) => {
      rl.question("URL: ", (answer) => resolve(answer.trim()));
    });
  } finally {
    rl.close();
  }
}

function fail(message: string, code = 1): never {
  process.stderr.write(`error: ${message}\n`);
  process.exit(code);
}
