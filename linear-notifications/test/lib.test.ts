import { test } from "node:test";
import { strict as assert } from "node:assert";
import {
  countStatus,
  formatTime,
  metadataRows,
  truncate,
  wrapText,
  type NotificationLike,
} from "../src/lib";

const NOW = new Date("2026-04-26T12:00:00.000Z");

test("formatTime: minutes when under an hour", () => {
  const t = new Date(NOW.getTime() - 17 * 60_000).toISOString();
  assert.equal(formatTime(t, NOW), "17m ago");
});

test("formatTime: hours when under a day", () => {
  const t = new Date(NOW.getTime() - 5 * 3600_000).toISOString();
  assert.equal(formatTime(t, NOW), "5h ago");
});

test("formatTime: days when over a day", () => {
  const t = new Date(NOW.getTime() - 3 * 24 * 3600_000).toISOString();
  assert.equal(formatTime(t, NOW), "3d ago");
});

test("truncate: returns original when short enough", () => {
  assert.equal(truncate("hello", 10), "hello");
});

test("truncate: clips and appends ellipsis", () => {
  assert.equal(truncate("hello world", 8), "hello w…");
  assert.equal(truncate("hello world", 8).length, 8);
});

test("truncate: returns empty when maxLen is zero or negative", () => {
  assert.equal(truncate("hello", 0), "");
  assert.equal(truncate("hello", -3), "");
});

test("wrapText: wraps within column width", () => {
  const lines = wrapText("the quick brown fox jumps", 10);
  for (const line of lines) {
    assert.ok(line.length <= 10, `line too long: "${line}"`);
  }
  assert.equal(lines.join(" "), "the quick brown fox jumps");
});

test("wrapText: preserves blank paragraphs", () => {
  const lines = wrapText("a\n\nb", 10);
  assert.deepEqual(lines, ["a", "", "b"]);
});

test("wrapText: words longer than cols stay on their own line", () => {
  const lines = wrapText("hi supercalifragilistic ok", 10);
  assert.deepEqual(lines, ["hi", "supercalifragilistic", "ok"]);
});

test("countStatus: pluralises correctly", () => {
  assert.equal(countStatus(0), "0 unread notifications");
  assert.equal(countStatus(1), "1 unread notification");
  assert.equal(countStatus(7), "7 unread notifications");
});

test("metadataRows: emits Type, Actor, Time, URL by default", () => {
  const n: NotificationLike = {
    type: "issueAssignedToYou",
    createdAt: new Date(NOW.getTime() - 30 * 60_000).toISOString(),
    url: "https://linear.app/x",
    actor: { name: "Alice" },
  };
  const rows = metadataRows(n, NOW);
  const labels = rows.map((r) => r.label);
  assert.deepEqual(labels, ["Type", "Actor", "Time", "URL"]);
  assert.equal(rows[0].value, "issueAssignedToYou");
  assert.equal(rows[1].value, "Alice");
  assert.ok(rows[2].value.startsWith("30m ago ("));
  assert.equal(rows[3].value, "https://linear.app/x");
});

test("metadataRows: includes Issue/Project/PR rows when present", () => {
  const n: NotificationLike = {
    type: "x",
    createdAt: NOW.toISOString(),
    url: "https://linear.app/x",
    actor: null,
    issue: { identifier: "ABC-1", title: "An issue" },
    project: { name: "Proj" },
    pullRequest: { title: "Fix it", number: 42, url: "https://gh/pr/42" },
  };
  const rows = metadataRows(n, NOW);
  const find = (label: string) => rows.find((r) => r.label === label)?.value;
  assert.equal(find("Issue"), "ABC-1 — An issue");
  assert.equal(find("Project"), "Proj");
  assert.equal(find("PR"), "#42 Fix it");
  assert.equal(find("Actor"), "—");
});
