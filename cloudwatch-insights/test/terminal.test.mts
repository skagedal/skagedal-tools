import { test } from "node:test";
import assert from "node:assert/strict";

import { hyperlink, openUrlCommand } from "../src/terminal.mjs";

test("hyperlink: wraps text in OSC 8 escape sequence with ST terminator", () => {
  const url = "https://example.com/foo";
  const text = "click me";
  const result = hyperlink(url, text);
  assert.equal(result, `\x1b]8;;${url}\x1b\\${text}\x1b]8;;\x1b\\`);
});

test("hyperlink: empty text still produces a well-formed sequence", () => {
  assert.equal(
    hyperlink("https://x", ""),
    "\x1b]8;;https://x\x1b\\\x1b]8;;\x1b\\",
  );
});

test("openUrlCommand: darwin uses `open`", () => {
  assert.deepEqual(openUrlCommand("https://x", "darwin"), {
    cmd: "open",
    args: ["https://x"],
  });
});

test("openUrlCommand: win32 uses cmd /c start with empty title", () => {
  assert.deepEqual(openUrlCommand("https://x", "win32"), {
    cmd: "cmd",
    args: ["/c", "start", "", "https://x"],
  });
});

test("openUrlCommand: linux/other uses xdg-open", () => {
  assert.deepEqual(openUrlCommand("https://x", "linux"), {
    cmd: "xdg-open",
    args: ["https://x"],
  });
  assert.deepEqual(openUrlCommand("https://x", "freebsd"), {
    cmd: "xdg-open",
    args: ["https://x"],
  });
});
