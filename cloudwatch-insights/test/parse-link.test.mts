import { test } from "node:test";
import assert from "node:assert/strict";
import { mkdtempSync, readFileSync, rmSync } from "fs";
import { tmpdir } from "os";
import { join } from "path";

import {
  buildRawCommand,
  formatRelativeDuration,
  parseLinkToState,
} from "../src/parse-link.mjs";
import {
  buildConsoleLink,
  buildQueryDetail,
  logGroupArn,
} from "../src/console-link.mjs";
import { parseQueryFile } from "../src/query-file.mjs";
import { writeFileSync } from "fs";

function buildLink(opts: {
  region: string;
  accountId: string;
  logGroups: string[];
  query: string;
  time: { kind: "relative"; secondsBack: number } | { kind: "absolute"; startMs: number; endMs: number };
}): string {
  const arns = opts.logGroups.map((lg) =>
    logGroupArn({ region: opts.region, accountId: opts.accountId, logGroupName: lg }),
  );
  const detail = buildQueryDetail({
    query: opts.query,
    logGroupArns: arns,
    time: opts.time,
  });
  return buildConsoleLink({ region: opts.region, queryDetail: detail });
}

test("formatRelativeDuration: picks the largest exact unit", () => {
  assert.equal(formatRelativeDuration(60), "1m");
  assert.equal(formatRelativeDuration(3600), "1h");
  assert.equal(formatRelativeDuration(86_400), "1d");
  assert.equal(formatRelativeDuration(7 * 86_400), "1w");
  assert.equal(formatRelativeDuration(2 * 3600), "2h");
  assert.equal(formatRelativeDuration(90), "90s"); // not divisible by 60
  assert.equal(formatRelativeDuration(45), "45s");
  assert.equal(formatRelativeDuration(0), "0s");
});

test("parseLinkToState: relative time, single log group", () => {
  const url = buildLink({
    region: "eu-north-1",
    accountId: "361629632765",
    logGroups: ["/eks/uat/team-icc"],
    query: "fields @timestamp, @message\n| limit 200",
    time: { kind: "relative", secondsBack: 3600 },
  });
  const state = parseLinkToState(url);
  assert.equal(state.region, "eu-north-1");
  assert.deepEqual(state.logGroups, ["/eks/uat/team-icc"]);
  assert.equal(state.time, "1h");
  assert.equal(state.query, "fields @timestamp, @message\n| limit 200");
});

test("parseLinkToState: absolute time emits ISO range with '/' separator", () => {
  const url = buildLink({
    region: "eu-north-1",
    accountId: "1",
    logGroups: ["/a", "/b"],
    query: "fields @timestamp",
    time: { kind: "absolute", startMs: 1700000000000, endMs: 1700003600000 },
  });
  const state = parseLinkToState(url);
  assert.deepEqual(state.logGroups, ["/a", "/b"]);
  assert.equal(
    state.time,
    `${new Date(1700000000000).toISOString()}/${new Date(1700003600000).toISOString()}`,
  );
});

test("parseLinkToState: rejects URL with no log groups", () => {
  // Build a URL by hand and strip the source key
  const detail = buildQueryDetail({
    query: "fields @timestamp",
    logGroupArns: ["arn:aws:logs:eu-north-1:1:log-group:/x"],
    time: { kind: "relative", secondsBack: 60 },
  });
  delete detail.source;
  const url = buildConsoleLink({ region: "eu-north-1", queryDetail: detail });
  assert.throws(() => parseLinkToState(url), /source/);
});

test("buildRawCommand: emits a single-line command + heredoc body", () => {
  const cmd = buildRawCommand({
    region: "eu-north-1",
    logGroups: ["/eks/uat/team-icc"],
    time: "1h",
    query: "fields @timestamp\n| limit 200",
  });
  assert.equal(
    cmd,
    "cloudwatch-insights raw -r eu-north-1 -g /eks/uat/team-icc -t 1h -f - <<'EOF'\n" +
      "fields @timestamp\n| limit 200\nEOF",
  );
});

test("buildRawCommand: shell-quotes values with spaces or special chars", () => {
  const cmd = buildRawCommand({
    region: "eu-north-1",
    logGroups: ["/with space", "/another"],
    time: "2026-04-22T13:00:00Z/2026-04-22T14:00:00Z",
    query: "fields @timestamp",
  });
  assert.match(cmd, /-g '\/with space' -g \/another /);
  // ISO time has only safe chars (alnum, :/.+-Z) so it stays unquoted
  assert.match(cmd, /-t 2026-04-22T13:00:00Z\/2026-04-22T14:00:00Z /);
});

test("buildRawCommand: escapes single-quotes inside values", () => {
  const cmd = buildRawCommand({
    region: "x",
    logGroups: ["/it's"],
    time: "1h",
    query: "q",
  });
  assert.match(cmd, /-g '\/it'\\''s'/);
});

test("end-to-end: link → parse-link → recreated current.insights", () => {
  const url = buildLink({
    region: "eu-north-1",
    accountId: "361629632765",
    logGroups: ["/eks/uat/team-icc"],
    query: "fields @timestamp, @message\n| sort @timestamp desc\n| limit 200",
    time: { kind: "relative", secondsBack: 5 * 3600 },
  });
  const state = parseLinkToState(url);

  const tmp = mkdtempSync(join(tmpdir(), "cwi-parse-link-"));
  try {
    const path = join(tmp, "current.insights");
    // Mirror what runParseLink writes (front-matter + body), so we test the
    // shape of the file the user would edit and re-run via `query`.
    const fm = `time = "${state.time}"\nlog-group = "${state.logGroups[0]}"\n`;
    writeFileSync(path, `${fm}---\n${state.query}\n`, "utf8");

    const parsed = parseQueryFile(readFileSync(path, "utf8"));
    assert.equal(parsed.frontMatter.time, "5h");
    assert.equal(parsed.frontMatter["log-group"], "/eks/uat/team-icc");
    assert.equal(parsed.body, state.query);
  } finally {
    rmSync(tmp, { recursive: true, force: true });
  }
});
