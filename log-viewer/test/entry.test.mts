import { test } from "node:test";
import { strict as assert } from "node:assert";
import {
  parseLine,
  renderColumns,
  renderField,
  renderValueDetailed,
  stringify,
} from "../src/entry.mts";
import type { Config } from "../src/config.mts";

test("parseLine: valid JSON object passes through unwrapped", () => {
  const e = parseLine(`{"level":"info","msg":"hi"}`, 0, "message");
  assert.equal(e.wrapped, false);
  assert.equal(e.id, 0);
  assert.deepEqual(e.data, { level: "info", msg: "hi" });
});

test("parseLine: non-JSON line is wrapped under defaultField", () => {
  const e = parseLine("plain log line", 5, "message");
  assert.equal(e.wrapped, true);
  assert.equal(e.id, 5);
  assert.deepEqual(e.data, { message: "plain log line" });
});

test("parseLine: bare scalars are valid JSON but get wrapped", () => {
  const e = parseLine("42", 0, "message");
  assert.equal(e.wrapped, true);
  assert.deepEqual(e.data, { message: 42 });
});

test("parseLine: trailing CR is stripped", () => {
  const e = parseLine(`{"a":1}\r`, 0, "message");
  assert.equal(e.raw, `{"a":1}`);
  assert.deepEqual(e.data, { a: 1 });
});

test("renderField: returns first non-empty candidate", () => {
  const e = parseLine(`{"msg":"","message":"hello"}`, 0, "message");
  assert.equal(
    renderField(e, { name: "m", from: ["msg", "message"] }),
    "hello"
  );
});

test("renderField: returns empty string when no candidate matches", () => {
  const e = parseLine(`{}`, 0, "message");
  assert.equal(renderField(e, { name: "x", from: ["a", "b"] }), "");
});

test("stringify: stringifies primitives without quotes, objects as JSON", () => {
  assert.equal(stringify("hi"), "hi");
  assert.equal(stringify(42), "42");
  assert.equal(stringify(true), "true");
  assert.equal(stringify({ a: 1 }), `{"a":1}`);
});

test("renderValueDetailed: pretty-prints objects, leaves strings raw", () => {
  assert.equal(renderValueDetailed("hi"), "hi");
  assert.equal(renderValueDetailed(null), "");
  assert.equal(renderValueDetailed({ a: 1 }), `{\n  "a": 1\n}`);
});

test("renderColumns: maps each configured field", () => {
  const e = parseLine(
    `{"@timestamp":"2026-01-01","level":"warn","msg":"oh no"}`,
    0,
    "message"
  );
  const config: Config = {
    fields: [
      { name: "time", from: ["@timestamp"] },
      { name: "level", from: ["level"] },
      { name: "message", from: ["message", "msg"] },
    ],
    defaultField: "message",
    triggers: [],
    profiles: [],
  };
  assert.deepEqual(renderColumns(e, config), [
    { name: "time", value: "2026-01-01" },
    { name: "level", value: "warn" },
    { name: "message", value: "oh no" },
  ]);
});
