import { test } from "node:test";
import { strict as assert } from "node:assert";
import {
  applyProfile,
  ConfigError,
  configPath,
  DEFAULT_CONFIG,
  parseConfig,
} from "../src/config.ts";

test("parseConfig: empty object falls back to defaults", () => {
  const cfg = parseConfig("{}");
  assert.deepEqual(cfg.fields, DEFAULT_CONFIG.fields);
  assert.equal(cfg.defaultField, DEFAULT_CONFIG.defaultField);
  assert.deepEqual(cfg.triggers, []);
  assert.deepEqual(cfg.profiles, []);
});

test("parseConfig: tolerates JSON5 features (comments, unquoted keys)", () => {
  const cfg = parseConfig(`{
    // comment
    default_field: "msg",
    fields: [
      { name: "lvl", from: "level" },
    ],
  }`);
  assert.equal(cfg.defaultField, "msg");
  assert.deepEqual(cfg.fields, [{ name: "lvl", from: ["level"] }]);
});

test("parseConfig: from defaults to [name] when omitted", () => {
  const cfg = parseConfig(`{ fields: [{ name: "level" }] }`);
  assert.deepEqual(cfg.fields, [{ name: "level", from: ["level"] }]);
});

test("parseConfig: triggers parse name, on_new_value, action, startup_delay_ms", () => {
  const cfg = parseConfig(`{
    triggers: [
      { name: "pod", on_new_value: ["podName", "pod"], action: "echo {value}", startup_delay_ms: 2000 },
      { on_new_value: "host", action: "echo host {value}" },
      { action: "no field, ignored" },
    ],
  }`);
  assert.equal(cfg.triggers.length, 2);
  assert.deepEqual(cfg.triggers[0], {
    name: "pod",
    onNewValue: ["podName", "pod"],
    action: "echo {value}",
    startupDelayMs: 2000,
  });
  assert.deepEqual(cfg.triggers[1], {
    name: "host",
    onNewValue: ["host"],
    action: "echo host {value}",
    startupDelayMs: 0,
  });
});

test("parseConfig: rejects non-object roots", () => {
  assert.throws(() => parseConfig("[1,2]"), ConfigError);
  assert.throws(() => parseConfig("garbage"), ConfigError);
});

test("applyProfile: replaces fields and defaultField, preserves triggers and profile list", () => {
  const cfg = parseConfig(`{
    fields: [{ name: "msg" }],
    triggers: [{ on_new_value: "host", action: "echo {value}" }],
    profiles: [
      { name: "stern", fields: [{ name: "pod", from: "podName" }], default_field: "log" },
    ],
  }`);
  const stern = applyProfile(cfg, "stern");
  assert.deepEqual(stern.fields, [{ name: "pod", from: ["podName"] }]);
  assert.equal(stern.defaultField, "log");
  assert.equal(stern.triggers.length, 1);
  assert.equal(stern.profiles.length, 1);
});

test("applyProfile: throws ConfigError for unknown profile name", () => {
  const cfg = parseConfig(`{ profiles: [{ name: "a" }] }`);
  assert.throws(() => applyProfile(cfg, "nope"), ConfigError);
});

test("configPath: respects LOG_VIEWER_CONFIG override", () => {
  assert.equal(
    configPath({ LOG_VIEWER_CONFIG: "/tmp/explicit.json5" } as NodeJS.ProcessEnv),
    "/tmp/explicit.json5"
  );
});

test("configPath: respects SKAGEDAL_TOOLS_HOME", () => {
  const path = configPath({ SKAGEDAL_TOOLS_HOME: "/tmp/skagedal" } as NodeJS.ProcessEnv);
  assert.equal(path, "/tmp/skagedal/log-viewer/config.json5");
});
