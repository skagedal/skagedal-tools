import { test } from "node:test";
import { strict as assert } from "node:assert";
import { entryHaystack, fuzzyMatch, killWordLeft, stripUnprintable } from "../src/tui/filter.ts";
import { parseLine } from "../src/entry.ts";

test("fuzzyMatch: empty needle matches anything", () => {
  assert.equal(fuzzyMatch("", ""), true);
  assert.equal(fuzzyMatch("", "anything"), true);
});

test("fuzzyMatch: exact substring matches", () => {
  assert.equal(fuzzyMatch("err", "level=error msg=oops"), true);
});

test("fuzzyMatch: subsequence (non-contiguous) matches", () => {
  assert.equal(fuzzyMatch("eor", "level=error"), true);
});

test("fuzzyMatch: case-insensitive", () => {
  assert.equal(fuzzyMatch("ERR", "level=error"), true);
  assert.equal(fuzzyMatch("err", "LEVEL=ERROR"), true);
});

test("fuzzyMatch: order matters", () => {
  // "rre" requires r,r,e in order — "error" has e,r,r,o,r so r,r,e fails.
  assert.equal(fuzzyMatch("rre", "error"), false);
});

test("fuzzyMatch: returns false when chars are missing", () => {
  assert.equal(fuzzyMatch("xyz", "level=error"), false);
});

test("killWordLeft: deletes the word before the cursor", () => {
  assert.deepEqual(killWordLeft("hello world", 11), { text: "hello ", cursor: 6 });
});

test("killWordLeft: also eats the whitespace right before the cursor", () => {
  assert.deepEqual(killWordLeft("hello world ", 12), { text: "hello ", cursor: 6 });
});

test("killWordLeft: cursor at column 0 is a no-op", () => {
  assert.deepEqual(killWordLeft("hello", 0), { text: "hello", cursor: 0 });
});

test("killWordLeft: only deletes left of cursor", () => {
  // cursor in the middle of "hello world": "hello |world"
  assert.deepEqual(killWordLeft("hello world", 6), { text: "world", cursor: 0 });
});

test("stripUnprintable: drops C0 controls and DEL, keeps printable text", () => {
  assert.equal(stripUnprintable("hello"), "hello");
  assert.equal(stripUnprintable("abcd"), "abcd");
  // Non-ASCII unicode is preserved.
  assert.equal(stripUnprintable("hëllo"), "hëllo");
});

test("entryHaystack: joins rendered field values with spaces", () => {
  const entry = parseLine(`{"level":"error","msg":"oops","extra":"ignored"}`, 0, "message");
  const fields = [
    { name: "level", from: ["level"] },
    { name: "msg", from: ["msg"] },
  ];
  assert.equal(entryHaystack(entry, fields), "error oops");
});

test("entryHaystack: skips fields whose source key is missing", () => {
  const entry = parseLine(`{"level":"info"}`, 0, "message");
  const fields = [
    { name: "time", from: ["time"] },
    { name: "level", from: ["level"] },
  ];
  // The "time" field renders as the empty string when absent.
  assert.equal(entryHaystack(entry, fields), " info");
});
