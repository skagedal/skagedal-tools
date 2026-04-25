import { test } from "node:test";
import { strict as assert } from "node:assert";
import { mkdtempSync, mkdirSync, readFileSync, writeFileSync, readlinkSync, rmSync } from "fs";
import { tmpdir } from "os";
import { join } from "path";

import {
  ensureCurrentInsights,
  loadQueryFile,
  parseQueryFile,
  resetCurrentInsights,
  runTimestamp,
  updateLatestSymlink,
  writeResults,
} from "../src/query-file.js";

test("parseQueryFile: no front-matter — whole file is body", () => {
  const { frontMatter, body } = parseQueryFile(
    "fields @timestamp, @message | sort @timestamp desc",
  );
  assert.deepEqual(frontMatter, {});
  assert.equal(body, "fields @timestamp, @message | sort @timestamp desc");
});

test("parseQueryFile: YAML front-matter with time and logGroup", () => {
  const { frontMatter, body } = parseQueryFile(
    [
      "---",
      "time: 5h",
      "logGroup: /prod/team",
      "---",
      "fields @timestamp, @message",
      "| sort @timestamp desc",
      "",
    ].join("\n"),
  );
  assert.deepEqual(frontMatter, {
    time: "5h",
    logGroup: "/prod/team",
  });
  assert.equal(body, "fields @timestamp, @message\n| sort @timestamp desc");
});

test("parseQueryFile: front-matter logGroup can be an array", () => {
  const { frontMatter } = parseQueryFile(
    "---\nlogGroup:\n  - /a\n  - /b\n---\nfields @timestamp",
  );
  assert.deepEqual(frontMatter.logGroup, ["/a", "/b"]);
});

test("parseQueryFile: unterminated front-matter treated as body", () => {
  const raw = "---\ntime: 5h\nfields @timestamp, @message";
  const { frontMatter, body } = parseQueryFile(raw);
  assert.deepEqual(frontMatter, {});
  assert.equal(body, raw);
});

test("parseQueryFile: empty front-matter block is valid (no keys)", () => {
  const { frontMatter, body } = parseQueryFile("---\n---\nfields @timestamp");
  assert.deepEqual(frontMatter, {});
  assert.equal(body, "fields @timestamp");
});

test("ensureCurrentInsights: seeds with the new default template + app filter", () => {
  const tmp = mkdtempSync(join(tmpdir(), "cwi-qf-"));
  try {
    const path = join(tmp, "current.insights");
    const created = ensureCurrentInsights(path, { app: "my-service" });
    assert.equal(created, true);
    const parsed = loadQueryFile(path);
    assert.match(parsed.body, /filter app = 'my-service'/);
    assert.match(parsed.body, /filter level in \['WARN', 'ERROR'\]/);
    assert.match(parsed.body, /limit 200/);
    assert.match(readFileSync(path, "utf8"), /^---\n/);
  } finally {
    rmSync(tmp, { recursive: true, force: true });
  }
});

test("ensureCurrentInsights: omits app filter when no app is configured", () => {
  const tmp = mkdtempSync(join(tmpdir(), "cwi-qf-noapp-"));
  try {
    const path = join(tmp, "current.insights");
    ensureCurrentInsights(path);
    const parsed = loadQueryFile(path);
    assert.doesNotMatch(parsed.body, /filter app/);
    assert.match(parsed.body, /filter level in \['WARN', 'ERROR'\]/);
  } finally {
    rmSync(tmp, { recursive: true, force: true });
  }
});

test("ensureCurrentInsights: pre-fills front-matter with provided values", () => {
  const tmp = mkdtempSync(join(tmpdir(), "cwi-qf-prefill-"));
  try {
    const path = join(tmp, "current.insights");
    ensureCurrentInsights(path, {
      app: "svc",
      frontMatter: { time: "5h", environment: "prod", logGroup: "/{env}/logs" },
    });
    const parsed = loadQueryFile(path);
    assert.deepEqual(parsed.frontMatter, {
      time: "5h",
      environment: "prod",
      logGroup: "/{env}/logs",
    });
  } finally {
    rmSync(tmp, { recursive: true, force: true });
  }
});

test("ensureCurrentInsights: pre-filled logGroup may be an array", () => {
  const tmp = mkdtempSync(join(tmpdir(), "cwi-qf-prefill-arr-"));
  try {
    const path = join(tmp, "current.insights");
    ensureCurrentInsights(path, {
      frontMatter: { logGroup: ["/a", "/b"] },
    });
    const parsed = loadQueryFile(path);
    assert.deepEqual(parsed.frontMatter.logGroup, ["/a", "/b"]);
  } finally {
    rmSync(tmp, { recursive: true, force: true });
  }
});

test("ensureCurrentInsights: empty front-matter still produces a valid file", () => {
  const tmp = mkdtempSync(join(tmpdir(), "cwi-qf-empty-fm-"));
  try {
    const path = join(tmp, "current.insights");
    ensureCurrentInsights(path);
    const parsed = loadQueryFile(path);
    assert.deepEqual(parsed.frontMatter, {});
    assert.match(parsed.body, /^fields @timestamp/);
  } finally {
    rmSync(tmp, { recursive: true, force: true });
  }
});

test("ensureCurrentInsights: returns false when file already exists (idempotent)", () => {
  const tmp = mkdtempSync(join(tmpdir(), "cwi-qf-exist-"));
  try {
    const path = join(tmp, "current.insights");
    writeFileSync(path, "---\ntime: 1h\n---\nfields @timestamp\n", "utf8");
    const before = readFileSync(path, "utf8");
    const created = ensureCurrentInsights(path, { app: "other-app" });
    assert.equal(created, false);
    assert.equal(readFileSync(path, "utf8"), before);
  } finally {
    rmSync(tmp, { recursive: true, force: true });
  }
});

test("resetCurrentInsights: overwrites an existing file with the default template", () => {
  const tmp = mkdtempSync(join(tmpdir(), "cwi-qf-reset-"));
  try {
    const path = join(tmp, "current.insights");
    writeFileSync(path, "---\n---\nuser-modified query", "utf8");
    resetCurrentInsights(path, { app: "my-service" });
    const parsed = loadQueryFile(path);
    assert.doesNotMatch(parsed.body, /user-modified/);
    assert.match(parsed.body, /filter app = 'my-service'/);
  } finally {
    rmSync(tmp, { recursive: true, force: true });
  }
});

test("runTimestamp: produces a filesystem-safe ISO-ish string", () => {
  const ts = runTimestamp(new Date("2026-04-23T14:30:45.123Z"));
  assert.equal(ts, "2026-04-23T14-30-45Z");
});

test("writeResults + updateLatestSymlink: writes JSONL and points the symlink", () => {
  const tmp = mkdtempSync(join(tmpdir(), "cwi-qf-results-"));
  try {
    process.env.SKAGEDAL_TOOLS_HOME = tmp;

    const resultPath = join(tmp, "cloudwatch-insights", "queries", "repo", "results", "run-x.jsonl");
    mkdirSync(join(tmp, "cloudwatch-insights"), { recursive: true });
    writeResults({
      path: resultPath,
      rows: [{ "@timestamp": "2026", "@message": "hello" }],
    });
    assert.equal(
      readFileSync(resultPath, "utf8"),
      '{"@timestamp":"2026","@message":"hello"}\n',
    );

    const link = updateLatestSymlink(resultPath);
    assert.equal(readlinkSync(link), resultPath);

    // Second run should replace the symlink, not fail.
    const other = join(tmp, "cloudwatch-insights", "queries", "repo", "results", "run-y.jsonl");
    writeResults({ path: other, rows: [] });
    updateLatestSymlink(other);
    assert.equal(readlinkSync(link), other);
  } finally {
    delete process.env.SKAGEDAL_TOOLS_HOME;
    rmSync(tmp, { recursive: true, force: true });
  }
});
