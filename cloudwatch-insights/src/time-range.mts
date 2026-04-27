/**
 * Flexible time-range parser.
 *
 * Primary ("shorthand") forms, evaluated in order:
 *
 *   1. Relative duration from now: "5h", "30m", "45s", "500ms", "2d", "1w".
 *
 *   2. Day keyword + time range on that day: "yesterday 17-18",
 *      "today 13.00-13.01.30", "yesterday 09:15:00.000-09:15:00.500".
 *
 *   3. Time-only range on today: "13.00-13.01", "13:00-13:01",
 *      "09.15.30-09.15.31", "09.15.30.000-09.15.30.500".
 *
 *   4. Explicit range with an explicit separator between the endpoints.
 *      Supported separators: "/", "..", and " to ". Either side may be a
 *      bare time (interpreted as today), a full date, or a full ISO
 *      datetime. Example: "2026-04-22T13:00:00Z/2026-04-22T14:00:00Z".
 *
 * As a final fallback we hand the whole expression to chrono-node, which
 * supports a wide range of natural-language dates and ranges (e.g.
 * "last Monday from 9am to 5pm", "Apr 22 13:00 to 14:00"). Chrono results
 * without an explicit end instant are rejected — we need a real range.
 */

import * as chrono from "chrono-node";

export interface TimeRange {
  /** Inclusive start of the range. */
  startTime: Date;
  /** Exclusive end of the range. */
  endTime: Date;
}

const MS_PER_UNIT: Record<string, number> = {
  ms: 1,
  s: 1_000,
  m: 60_000,
  h: 3_600_000,
  d: 86_400_000,
  w: 7 * 86_400_000,
};

const DURATION_RE = /^(\d+(?:\.\d+)?)(ms|s|m|h|d|w)$/;

// Time with "." or ":" as separator. Captures H, M?, S?, MS?.
const TIME_RE = /^(\d{1,2})(?:[.:](\d{1,2})(?:[.:](\d{1,2})(?:\.(\d{1,3}))?)?)?$/;

const DAY_KEYWORDS = new Set(["today", "yesterday"]);

export class TimeRangeParseError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "TimeRangeParseError";
  }
}

/**
 * Parse a time-range expression.
 *
 * @param input The user-supplied range string.
 * @param now   Reference "now" (for testability). Defaults to the current time.
 */
export function parseTimeRange(input: string, now: Date = new Date()): TimeRange {
  const trimmed = input.trim();
  if (!trimmed) {
    throw new TimeRangeParseError("empty time range");
  }

  const duration = tryParseDuration(trimmed, now);
  if (duration) return duration;

  const dayKeyword = tryParseDayKeywordRange(trimmed, now);
  if (dayKeyword) return dayKeyword;

  const timeOnly = tryParseTimeOnlyRange(trimmed, now);
  if (timeOnly) return timeOnly;

  const explicit = tryParseExplicitRange(trimmed, now);
  if (explicit) return explicit;

  const fromChrono = tryParseWithChrono(trimmed, now);
  if (fromChrono) return fromChrono;

  throw new TimeRangeParseError(`could not parse time range: ${JSON.stringify(input)}`);
}

/**
 * If `input` is a relative-duration shorthand ("5h", "30m", ...) that is
 * a whole number of seconds, return that number of seconds; otherwise null.
 * Used by the `link` subcommand to emit the Console's RELATIVE time form
 * when the user typed a duration and otherwise fall back to ABSOLUTE.
 */
export function tryParseRelativeDurationSeconds(input: string): number | null {
  const match = DURATION_RE.exec(input.trim());
  if (!match) return null;
  const amount = parseFloat(match[1]);
  const unit = match[2];
  const ms = amount * MS_PER_UNIT[unit];
  if (ms % 1000 !== 0) return null;
  return ms / 1000;
}

function tryParseDuration(input: string, now: Date): TimeRange | null {
  const match = DURATION_RE.exec(input);
  if (!match) return null;
  const amount = parseFloat(match[1]);
  const unit = match[2];
  const ms = amount * MS_PER_UNIT[unit];
  return { startTime: new Date(now.getTime() - ms), endTime: now };
}

function tryParseDayKeywordRange(input: string, now: Date): TimeRange | null {
  const spaceIdx = input.indexOf(" ");
  if (spaceIdx < 0) {
    // A bare day keyword means the full day.
    const word = input.toLowerCase();
    if (!DAY_KEYWORDS.has(word)) return null;
    const base = resolveDayKeyword(word, now);
    return {
      startTime: base,
      endTime: new Date(base.getTime() + MS_PER_UNIT.d),
    };
  }

  const word = input.slice(0, spaceIdx).toLowerCase();
  if (!DAY_KEYWORDS.has(word)) return null;
  const rest = input.slice(spaceIdx + 1).trim();
  const base = resolveDayKeyword(word, now);

  const range = parseTimeRangeString(rest, base);
  if (range) return range;

  // Let chrono handle "yesterday at 5pm to 6pm" etc.
  return null;
}

function tryParseTimeOnlyRange(input: string, now: Date): TimeRange | null {
  const base = startOfDay(now);
  return parseTimeRangeString(input, base);
}

/** Parse "TIME-TIME" where each TIME uses dots/colons (no dashes of its own). */
function parseTimeRangeString(input: string, base: Date): TimeRange | null {
  const dashIdx = input.indexOf("-");
  if (dashIdx <= 0) return null;
  const leftRaw = input.slice(0, dashIdx).trim();
  const rightRaw = input.slice(dashIdx + 1).trim();
  const left = parseTimeOfDay(leftRaw);
  const right = parseTimeOfDay(rightRaw);
  if (left === null || right === null) return null;
  const startTime = new Date(base.getTime() + left);
  let endTime = new Date(base.getTime() + right);
  // Allow wrap past midnight (e.g. "23.30-00.30").
  if (endTime.getTime() <= startTime.getTime()) {
    endTime = new Date(endTime.getTime() + MS_PER_UNIT.d);
  }
  return { startTime, endTime };
}

/** Parse "HH", "HH:MM", "HH.MM.SS", "HH.MM.SS.mmm" etc. Returns ms-of-day or null. */
function parseTimeOfDay(input: string): number | null {
  const match = TIME_RE.exec(input);
  if (!match) return null;
  const h = parseInt(match[1], 10);
  const m = match[2] ? parseInt(match[2], 10) : 0;
  const s = match[3] ? parseInt(match[3], 10) : 0;
  const ms = match[4] ? parseInt(match[4].padEnd(3, "0"), 10) : 0;
  if (h > 23 || m > 59 || s > 59) return null;
  return h * MS_PER_UNIT.h + m * MS_PER_UNIT.m + s * MS_PER_UNIT.s + ms;
}

function tryParseExplicitRange(input: string, now: Date): TimeRange | null {
  const separators = [" to ", "..", "/"];
  for (const sep of separators) {
    const idx = input.indexOf(sep);
    if (idx < 0) continue;
    const left = input.slice(0, idx).trim();
    const right = input.slice(idx + sep.length).trim();
    return {
      startTime: parseInstant(left, now),
      endTime: parseInstant(right, now),
    };
  }
  return null;
}

function tryParseWithChrono(input: string, now: Date): TimeRange | null {
  const results = chrono.parse(input, now);
  if (results.length === 0) return null;
  const first = results[0];
  if (!first.end) return null;
  return { startTime: first.start.date(), endTime: first.end.date() };
}

/**
 * Parse a single instant (used for each side of an explicit range).
 * Accepts bare times, day keywords, ISO dates/datetimes, or anything
 * chrono-node can understand.
 */
function parseInstant(input: string, now: Date): Date {
  const timeOfDay = parseTimeOfDay(input);
  if (timeOfDay !== null) {
    return new Date(startOfDay(now).getTime() + timeOfDay);
  }

  const lower = input.toLowerCase();
  if (DAY_KEYWORDS.has(lower)) {
    return resolveDayKeyword(lower, now);
  }

  // "YYYY-MM-DD HH:MM[:SS[.mmm]]" → turn the space into "T" so Date parses it.
  let normalized = input;
  if (/^\d{4}-\d{2}-\d{2} /.test(normalized)) {
    normalized = normalized.replace(" ", "T");
  }

  const direct = new Date(normalized);
  if (!isNaN(direct.getTime())) {
    return direct;
  }

  const chronoResult = chrono.parseDate(input, now);
  if (chronoResult) return chronoResult;

  throw new TimeRangeParseError(`could not parse instant: ${JSON.stringify(input)}`);
}

function startOfDay(d: Date): Date {
  return new Date(d.getFullYear(), d.getMonth(), d.getDate());
}

function resolveDayKeyword(word: string, now: Date): Date {
  const base = startOfDay(now);
  if (word === "today") return base;
  if (word === "yesterday") {
    return new Date(base.getTime() - MS_PER_UNIT.d);
  }
  throw new TimeRangeParseError(`unknown day keyword: ${word}`);
}
