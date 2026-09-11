import { test } from "node:test";
import { strict as assert } from "node:assert";
import {
  clampOffset,
  countStatus,
  documentLines,
  formatTime,
  labeledLines,
  metadataRows,
  notificationLines,
  previewLines,
  scrollStatus,
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

const DOC_NOTIFICATION: NotificationLike = {
  type: "documentNewComment",
  createdAt: new Date(NOW.getTime() - 10 * 60_000).toISOString(),
  url: "https://linear.app/acme/document/spec#comment-abc",
  actor: { name: "Bob" },
  title: "Technical Spec",
  subtitle: "Bob replied: a rather long remark that would not fit on one line",
};

const text = (lines: { text: string }[]) => lines.map((l) => l.text);

test("labeledLines: pads labels to a common width", () => {
  const lines = labeledLines(
    [
      { label: "Type", value: "a" },
      { label: "Actor", value: "b" },
    ],
    40
  );
  assert.deepEqual(
    lines.map((l) => l.label),
    ["Type:  ", "Actor: "]
  );
  assert.deepEqual(text(lines), ["a", "b"]);
});

test("previewLines: uses the subtitle as the body when there is no comment", () => {
  const lines = previewLines(DOC_NOTIFICATION, 30, NOW);
  assert.ok(text(lines).includes("Summary"));
  const body = text(lines).slice(text(lines).indexOf("Summary") + 1);
  assert.equal(body.join(" "), DOC_NOTIFICATION.subtitle);
  for (const line of body) assert.ok(line.length <= 30);
});

test("previewLines: keeps the subtitle as a header when a comment body exists", () => {
  const n: NotificationLike = {
    ...DOC_NOTIFICATION,
    comment: { body: "the real comment" },
  };
  const lines = previewLines(n, 60, NOW);
  assert.ok(text(lines).includes("Comment"));
  assert.ok(text(lines).includes("the real comment"));
  assert.ok(text(lines).some((t) => t.startsWith("Bob replied:")));
});

test("documentLines: shows the whole comment body, unwrapped by truncation", () => {
  const body =
    "First paragraph that is quite long and needs wrapping.\n\nSecond paragraph.";
  const lines = documentLines(
    DOC_NOTIFICATION,
    {
      title: "Technical Spec",
      url: "https://linear.app/acme/document/spec",
      createdAt: NOW.toISOString(),
      creator: { name: "Alice" },
    },
    {
      body,
      createdAt: NOW.toISOString(),
      user: { name: "Bob" },
      parent: {
        body: "Why in the BFF?",
        createdAt: NOW.toISOString(),
        user: { name: "Simon" },
      },
      children: {
        nodes: [
          { body: "Agreed.", createdAt: NOW.toISOString(), user: { name: "Ada" } },
        ],
      },
    },
    40,
    NOW
  );
  const t = text(lines);
  assert.ok(t.some((l) => l.startsWith("In reply to Simon")));
  assert.ok(t.includes("│ Why in the BFF?"));
  assert.ok(t.some((l) => l.startsWith("Comment — Bob")));
  assert.ok(t.includes("Replies (1)"));
  assert.ok(t.includes("Agreed."));
  for (const line of t) assert.ok(line.length <= 40, `too long: "${line}"`);
  const start = t.indexOf("First paragraph that is quite long and");
  assert.ok(start > 0);
  assert.equal(
    t.slice(start, start + 4).join("\n"),
    "First paragraph that is quite long and\nneeds wrapping.\n\nSecond paragraph."
  );
});

test("documentLines: falls back to the subtitle when the comment is missing", () => {
  const lines = documentLines(DOC_NOTIFICATION, null, null, 30, NOW);
  const t = text(lines);
  assert.ok(t.includes("Summary"));
  assert.equal(t.slice(t.indexOf("Summary") + 1).join(" "), DOC_NOTIFICATION.subtitle);
});

test("notificationLines: wraps the subtitle rather than cropping it", () => {
  const lines = notificationLines(DOC_NOTIFICATION, 20, NOW);
  const t = text(lines);
  assert.equal(t.slice(t.indexOf("Summary") + 1).join(" "), DOC_NOTIFICATION.subtitle);
});

test("scrollStatus: null when everything fits, else a range", () => {
  assert.equal(scrollStatus(10, 0, 10), null);
  assert.equal(scrollStatus(120, 0, 20), "1–20/120");
  assert.equal(scrollStatus(120, 110, 20), "111–120/120");
});

test("clampOffset: keeps the last screen full", () => {
  assert.equal(clampOffset(120, 500, 20), 100);
  assert.equal(clampOffset(120, -5, 20), 0);
  assert.equal(clampOffset(5, 3, 20), 0);
});
