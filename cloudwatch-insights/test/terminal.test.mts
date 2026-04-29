import { test } from "node:test";
import assert from "node:assert/strict";

import { colorize, hyperlink, openUrlCommand, styledLink } from "../src/terminal.mjs";

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

test("colorize: prepends SGR codes and resets", () => {
  assert.equal(
    colorize("hi", { color: "cyan", underline: true }),
    "\x1b[4;36mhi\x1b[0m",
  );
});

test("colorize: with no styling returns the text unchanged", () => {
  assert.equal(colorize("hi", {}), "hi");
});

test("styledLink: combines OSC 8 hyperlink with SGR styling", () => {
  const url = "https://example.com/q";
  const result = styledLink(url, "Open", { color: "cyan", underline: true });
  // The visible text "Open" sits inside both the OSC 8 sequence and the SGR
  // style; terminals that ignore OSC 8 still see the styled text.
  assert.equal(
    result,
    `\x1b]8;;${url}\x1b\\\x1b[4;36mOpen\x1b[0m\x1b]8;;\x1b\\`,
  );
});
