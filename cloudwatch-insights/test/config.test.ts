import { test } from "node:test";
import { strict as assert } from "node:assert";
import { mkdtempSync, mkdirSync, writeFileSync, rmSync } from "fs";
import { tmpdir } from "os";
import { join } from "path";
import { spawnSync } from "child_process";

import {
  applyEnvironment,
  ConfigError,
  configPath,
  defaultQueryForApp,
  findGitRoot,
  loadSettings,
  parseSettings,
  resolveRepoDefaults,
} from "../src/config";

test("parseSettings: extracts group and app from a named section", () => {
  const settings = parseSettings(
    [
      "[installer-notification]",
      'group = "/{env}/team-icc"',
      'app = "installer-notification"',
      "",
      "[another-service]",
      'group = "/prod/another"',
    ].join("\n"),
  );

  assert.deepEqual(settings.sections["installer-notification"], {
    group: "/{env}/team-icc",
    app: "installer-notification",
  });
  assert.deepEqual(settings.sections["another-service"], {
    group: "/prod/another",
  });
});

test("parseSettings: ignores unknown keys and non-table values", () => {
  const settings = parseSettings(
    [
      "unused = 42",
      "[repo]",
      'group = "/foo"',
      "extra = 99",
    ].join("\n"),
  );
  assert.deepEqual(settings.sections["repo"], { group: "/foo" });
  assert.ok(!("unused" in settings.sections));
});

test("parseSettings: rejects malformed TOML with a ConfigError", () => {
  assert.throws(() => parseSettings("[broken"), ConfigError);
});

test("applyEnvironment: substitutes {env} and passes through otherwise", () => {
  assert.equal(applyEnvironment("/prod/team", undefined), "/prod/team");
  assert.equal(applyEnvironment("/{env}/team", "systest"), "/systest/team");
  assert.equal(applyEnvironment("/{env}/team/{env}", "uat"), "/uat/team/uat");
});

test("applyEnvironment: errors when template uses {env} but env is missing", () => {
  assert.throws(() => applyEnvironment("/{env}/team", undefined), ConfigError);
});

test("defaultQueryForApp: builds a filter-by-app Insights query", () => {
  assert.equal(
    defaultQueryForApp("installer-notification"),
    'fields @timestamp, @message, app | filter app = "installer-notification" | sort @timestamp desc',
  );
});

test("defaultQueryForApp: escapes embedded quotes", () => {
  assert.equal(
    defaultQueryForApp('weird"name'),
    'fields @timestamp, @message, app | filter app = "weird\\"name" | sort @timestamp desc',
  );
});

test("configPath: honors CLOUDWATCH_INSIGHTS_CONFIG, XDG_CONFIG_HOME, HOME", () => {
  assert.equal(
    configPath({ CLOUDWATCH_INSIGHTS_CONFIG: "/tmp/c.toml" }),
    "/tmp/c.toml",
  );
  assert.equal(
    configPath({ XDG_CONFIG_HOME: "/xdg" }),
    "/xdg/cloudwatch-insights/settings.toml",
  );
});

test("findGitRoot + resolveRepoDefaults work against a real git repo", () => {
  const tmpRoot = mkdtempSync(join(tmpdir(), "cwi-test-"));
  try {
    const repoRoot = join(tmpRoot, "installer-notification");
    mkdirSync(repoRoot);
    const gitInit = spawnSync("git", ["init", "-q", "-b", "main"], { cwd: repoRoot });
    assert.equal(gitInit.status, 0, "git init failed");

    // Sanity: detect root
    assert.equal(findGitRoot(repoRoot), repoRoot);

    // Load minimal settings and resolve
    const settings = loadSettings(writeSettings(tmpRoot, [
      "[installer-notification]",
      'group = "/{env}/team-icc"',
      'app = "installer-notification"',
    ].join("\n")));

    const { sectionName, defaults } = resolveRepoDefaults(settings, repoRoot);
    assert.equal(sectionName, "installer-notification");
    assert.equal(defaults.group, "/{env}/team-icc");
    assert.equal(defaults.app, "installer-notification");
  } finally {
    rmSync(tmpRoot, { recursive: true, force: true });
  }
});

test("findGitRoot returns null outside a git repo", () => {
  const tmp = mkdtempSync(join(tmpdir(), "cwi-nogit-"));
  try {
    assert.equal(findGitRoot(tmp), null);
  } finally {
    rmSync(tmp, { recursive: true, force: true });
  }
});

function writeSettings(dir: string, contents: string): string {
  const path = join(dir, "settings.toml");
  writeFileSync(path, contents, "utf8");
  return path;
}
