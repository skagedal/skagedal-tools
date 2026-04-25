import { test } from "node:test";
import { strict as assert } from "node:assert";

import { flattenRow } from "../src/flatten.js";

test("flattenRow: flattens a JSON-encoded field into the row", () => {
  const row = {
    "@timestamp": "2026-04-25T12:00:00Z",
    "@message": '{"foo": "bar", "baz": "frotz"}',
  };
  assert.deepEqual(flattenRow(row, ["@message"]), {
    "@timestamp": "2026-04-25T12:00:00Z",
    foo: "bar",
    baz: "frotz",
  });
});

test("flattenRow: leaves the field untouched when it is not valid JSON", () => {
  const row = {
    "@timestamp": "2026-04-25T12:00:00Z",
    "@message": "not json at all",
  };
  assert.deepEqual(flattenRow(row, ["@message"]), { ...row });
});

test("flattenRow: leaves the field untouched when JSON is not a plain object", () => {
  const row = { "@message": '["a", "b"]' };
  assert.deepEqual(flattenRow(row, ["@message"]), { "@message": '["a", "b"]' });
});

test("flattenRow: preserves nested values (numbers, booleans, objects)", () => {
  const row = {
    "@message": '{"n": 1, "ok": true, "ctx": {"id": "x"}}',
  };
  assert.deepEqual(flattenRow(row, ["@message"]), {
    n: 1,
    ok: true,
    ctx: { id: "x" },
  });
});

test("flattenRow: when no fields requested, returns a shallow copy of the row", () => {
  const row = { a: "1", b: "2" };
  const out = flattenRow(row, []);
  assert.deepEqual(out, row);
  assert.notEqual(out, row, "should be a copy, not the same reference");
});

test("flattenRow: silently skips fields that are absent from the row", () => {
  const row = { kept: "yes" };
  assert.deepEqual(flattenRow(row, ["@message"]), { kept: "yes" });
});

test("flattenRow: flattened keys overwrite existing row keys on collision", () => {
  const row = {
    foo: "outer",
    "@message": '{"foo": "inner"}',
  };
  assert.deepEqual(flattenRow(row, ["@message"]), { foo: "inner" });
});
