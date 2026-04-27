import { test } from "node:test";
import { strict as assert } from "node:assert";

import {
  parseTimeRange,
  TimeRangeParseError,
  tryParseRelativeDurationSeconds,
} from "../src/time-range.mjs";

// Fixed "now" — 2026-04-22 at 15:30:00 local time.
const NOW = new Date(2026, 3, 22, 15, 30, 0);

function ms(ymd: [number, number, number], hms: [number, number, number, number] = [0, 0, 0, 0]): number {
  const [y, mo, d] = ymd;
  const [h, mi, s, milli] = hms;
  return new Date(y, mo - 1, d, h, mi, s, milli).getTime();
}

test("relative duration: hours", () => {
  const { startTime, endTime } = parseTimeRange("5h", NOW);
  assert.equal(endTime.getTime(), NOW.getTime());
  assert.equal(startTime.getTime(), NOW.getTime() - 5 * 3_600_000);
});

test("relative duration: minutes, seconds, ms, days, weeks", () => {
  for (const [input, msDelta] of [
    ["30m", 30 * 60_000],
    ["45s", 45 * 1_000],
    ["500ms", 500],
    ["2d", 2 * 86_400_000],
    ["1w", 7 * 86_400_000],
  ] as const) {
    const { startTime, endTime } = parseTimeRange(input, NOW);
    assert.equal(endTime.getTime(), NOW.getTime(), `${input} end`);
    assert.equal(startTime.getTime(), NOW.getTime() - msDelta, `${input} start`);
  }
});

test("time-only range with dots (today)", () => {
  const { startTime, endTime } = parseTimeRange("13.00-13.01", NOW);
  assert.equal(startTime.getTime(), ms([2026, 4, 22], [13, 0, 0, 0]));
  assert.equal(endTime.getTime(), ms([2026, 4, 22], [13, 1, 0, 0]));
});

test("time-only range with colons", () => {
  const { startTime, endTime } = parseTimeRange("09:00-10:00", NOW);
  assert.equal(startTime.getTime(), ms([2026, 4, 22], [9, 0, 0, 0]));
  assert.equal(endTime.getTime(), ms([2026, 4, 22], [10, 0, 0, 0]));
});

test("time-only range with seconds", () => {
  const { startTime, endTime } = parseTimeRange("13.00.30-13.01.15", NOW);
  assert.equal(startTime.getTime(), ms([2026, 4, 22], [13, 0, 30, 0]));
  assert.equal(endTime.getTime(), ms([2026, 4, 22], [13, 1, 15, 0]));
});

test("time-only range with milliseconds", () => {
  const { startTime, endTime } = parseTimeRange("09:15:00.000-09:15:00.500", NOW);
  assert.equal(startTime.getTime(), ms([2026, 4, 22], [9, 15, 0, 0]));
  assert.equal(endTime.getTime(), ms([2026, 4, 22], [9, 15, 0, 500]));
});

test("time-only range wraps past midnight", () => {
  const { startTime, endTime } = parseTimeRange("23.30-00.30", NOW);
  assert.equal(startTime.getTime(), ms([2026, 4, 22], [23, 30, 0, 0]));
  assert.equal(endTime.getTime(), ms([2026, 4, 23], [0, 30, 0, 0]));
});

test("day keyword + time range: yesterday", () => {
  const { startTime, endTime } = parseTimeRange("yesterday 17-18", NOW);
  assert.equal(startTime.getTime(), ms([2026, 4, 21], [17, 0, 0, 0]));
  assert.equal(endTime.getTime(), ms([2026, 4, 21], [18, 0, 0, 0]));
});

test("day keyword + time range: today with seconds", () => {
  const { startTime, endTime } = parseTimeRange("today 13.00-13.01.30", NOW);
  assert.equal(startTime.getTime(), ms([2026, 4, 22], [13, 0, 0, 0]));
  assert.equal(endTime.getTime(), ms([2026, 4, 22], [13, 1, 30, 0]));
});

test("bare day keyword means the whole day", () => {
  const { startTime, endTime } = parseTimeRange("yesterday", NOW);
  assert.equal(startTime.getTime(), ms([2026, 4, 21]));
  assert.equal(endTime.getTime(), ms([2026, 4, 22]));
});

test("explicit ISO range using '/'", () => {
  const { startTime, endTime } = parseTimeRange(
    "2026-04-22T13:00:00Z/2026-04-22T14:00:00Z",
    NOW,
  );
  assert.equal(startTime.toISOString(), "2026-04-22T13:00:00.000Z");
  assert.equal(endTime.toISOString(), "2026-04-22T14:00:00.000Z");
});

test("explicit range using '..' with space-separated date and time", () => {
  const { startTime, endTime } = parseTimeRange(
    "2026-04-22 13:00..2026-04-22 14:00",
    NOW,
  );
  assert.equal(startTime.getTime(), ms([2026, 4, 22], [13, 0, 0, 0]));
  assert.equal(endTime.getTime(), ms([2026, 4, 22], [14, 0, 0, 0]));
});

test("explicit range using ' to ' with bare times", () => {
  const { startTime, endTime } = parseTimeRange("13.00 to 14.00", NOW);
  assert.equal(startTime.getTime(), ms([2026, 4, 22], [13, 0, 0, 0]));
  assert.equal(endTime.getTime(), ms([2026, 4, 22], [14, 0, 0, 0]));
});

test("natural-language fallback (chrono-node)", () => {
  // "from X to Y" is the idiomatic form chrono understands.
  const { startTime, endTime } = parseTimeRange(
    "from 2026-04-22 13:00 to 2026-04-22 14:00",
    NOW,
  );
  assert.ok(startTime.getTime() < endTime.getTime());
  assert.equal(endTime.getTime() - startTime.getTime(), 3_600_000);
});

test("empty input is rejected", () => {
  assert.throws(() => parseTimeRange("", NOW), TimeRangeParseError);
  assert.throws(() => parseTimeRange("   ", NOW), TimeRangeParseError);
});

test("garbage input is rejected", () => {
  assert.throws(() => parseTimeRange("not a time", NOW), TimeRangeParseError);
});

test("tryParseRelativeDurationSeconds: whole-second durations", () => {
  assert.equal(tryParseRelativeDurationSeconds("1h"), 3600);
  assert.equal(tryParseRelativeDurationSeconds("30m"), 1800);
  assert.equal(tryParseRelativeDurationSeconds("45s"), 45);
  assert.equal(tryParseRelativeDurationSeconds("2d"), 172800);
  assert.equal(tryParseRelativeDurationSeconds(" 5h "), 18000);
});

test("tryParseRelativeDurationSeconds: rejects sub-second and non-duration input", () => {
  assert.equal(tryParseRelativeDurationSeconds("500ms"), null);
  assert.equal(tryParseRelativeDurationSeconds("yesterday 17-18"), null);
  assert.equal(tryParseRelativeDurationSeconds("13.00-13.01"), null);
  assert.equal(tryParseRelativeDurationSeconds(""), null);
});
