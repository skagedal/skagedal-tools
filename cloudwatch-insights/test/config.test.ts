import { test } from "node:test";
import { strict as assert } from "node:assert";
import { mkdtempSync, mkdirSync, readFileSync, realpathSync, writeFileSync, rmSync } from "fs";
import { tmpdir } from "os";
import { join } from "path";
import { spawnSync } from "child_process";

import {
  applyEnvironment,
  ConfigError,
  configPath,
  defaultQuery,
  ensureConfigFile,
  findGitRoot,
  loadSettings,
  parseSettings,
  resolveRepoDefaults,
} from "../src/config.js";

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

test("defaultQuery: substitutes {app} placeholder", () => {
  assert.equal(
    defaultQuery("my-service"),
    [
      "fields @timestamp, @message",
      "| sort @timestamp desc",
      "| filter app = my-service",
      "| filter level in ['WARN', 'ERROR']",
      "| limit 200",
    ].join("\n"),
  );
});

test("defaultQuery: omits the app filter line when no app is given", () => {
  assert.equal(
    defaultQuery(),
    [
      "fields @timestamp, @message",
      "| sort @timestamp desc",
      "| filter level in ['WARN', 'ERROR']",
      "| limit 200",
    ].join("\n"),
  );
});

test("configPath: honors CLOUDWATCH_INSIGHTS_CONFIG and SKAGEDAL_TOOLS_HOME", () => {
  assert.equal(
    configPath({ CLOUDWATCH_INSIGHTS_CONFIG: "/tmp/c.toml" }),
    "/tmp/c.toml",
  );
  assert.equal(
    configPath({ SKAGEDAL_TOOLS_HOME: "/custom" }),
    "/custom/cloudwatch-insights/settings.toml",
  );
});

test("findGitRoot + resolveRepoDefaults work against a real git repo", () => {
  const tmpRoot = realpathSync(mkdtempSync(join(tmpdir(), "cwi-test-")));
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

test("ensureConfigFile: creates file + placeholder on first call, is idempotent on later calls", () => {
  const tmpRoot = mkdtempSync(join(tmpdir(), "cwi-ensure-"));
  try {
    const repoRoot = join(tmpRoot, "my-service");
    mkdirSync(repoRoot);
    assert.equal(
      spawnSync("git", ["init", "-q", "-b", "main"], { cwd: repoRoot }).status,
      0,
    );
    const settingsPath = join(tmpRoot, "settings.toml");

    const first = ensureConfigFile(settingsPath, repoRoot);
    assert.equal(first.fileCreated, true);
    assert.equal(first.addedSection, "my-service");

    const contents1 = readFileSync(settingsPath, "utf8");
    assert.match(contents1, /# \[my-service\]/);
    assert.match(contents1, /# group = /);
    assert.match(contents1, /# app   = "my-service"/);

    const second = ensureConfigFile(settingsPath, repoRoot);
    assert.equal(second.fileCreated, false);
    assert.equal(second.addedSection, null, "placeholder must not be duplicated");
    assert.equal(readFileSync(settingsPath, "utf8"), contents1);
  } finally {
    rmSync(tmpRoot, { recursive: true, force: true });
  }
});

test("ensureConfigFile: does nothing special when run outside a git repo", () => {
  const tmpRoot = mkdtempSync(join(tmpdir(), "cwi-ensure-nogit-"));
  try {
    const settingsPath = join(tmpRoot, "settings.toml");
    const result = ensureConfigFile(settingsPath, tmpRoot);
    assert.equal(result.fileCreated, true);
    assert.equal(result.addedSection, null);
    const contents = readFileSync(settingsPath, "utf8");
    assert.doesNotMatch(contents, /^\[/m, "no non-example section should be appended");
  } finally {
    rmSync(tmpRoot, { recursive: true, force: true });
  }
});

test("ensureConfigFile: skips appending when the repo already has a real section", () => {
  const tmpRoot = mkdtempSync(join(tmpdir(), "cwi-ensure-existing-"));
  try {
    const repoRoot = join(tmpRoot, "my-service");
    mkdirSync(repoRoot);
    assert.equal(
      spawnSync("git", ["init", "-q", "-b", "main"], { cwd: repoRoot }).status,
      0,
    );
    const settingsPath = writeSettings(
      tmpRoot,
      '[my-service]\ngroup = "/prod/team"\n',
    );

    const before = readFileSync(settingsPath, "utf8");
    const result = ensureConfigFile(settingsPath, repoRoot);
    assert.equal(result.fileCreated, false);
    assert.equal(result.addedSection, null);
    assert.equal(readFileSync(settingsPath, "utf8"), before);
  } finally {
    rmSync(tmpRoot, { recursive: true, force: true });
  }
});
