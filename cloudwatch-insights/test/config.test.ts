import { test } from "node:test";
import { strict as assert } from "node:assert";
import { mkdtempSync, mkdirSync, readFileSync, realpathSync, writeFileSync, rmSync } from "fs";
import { tmpdir } from "os";
import { join } from "path";
import { spawnSync } from "child_process";

import {
  ConfigError,
  configPath,
  defaultQuery,
  ensureConfigFile,
  expandTemplate,
  findGitRoot,
  isValidEnvironmentName,
  loadSettings,
  parseSettings,
  resolveEnvConfig,
  resolveRepoDefaults,
} from "../src/config.js";

test("parseSettings: extracts defaults and per-repo configs", () => {
  const settings = parseSettings(
    [
      "[defaults]",
      'flatten-fields = ["@message"]',
      "",
      "[repo.installer-notification]",
      'group = "/{env}/team-icc"',
      'app = "installer-notification"',
      "",
      "[repo.another-service]",
      'group = "/prod/another"',
    ].join("\n"),
  );

  assert.deepEqual(settings.defaults, { flattenFields: ["@message"] });
  assert.deepEqual(settings.repos["installer-notification"], {
    group: "/{env}/team-icc",
    app: "installer-notification",
  });
  assert.deepEqual(settings.repos["another-service"], {
    group: "/prod/another",
  });
});

test("parseSettings: empty file yields empty defaults, repos, and environments", () => {
  const settings = parseSettings("");
  assert.deepEqual(settings.defaults, {});
  assert.deepEqual(settings.repos, {});
  assert.deepEqual(settings.environments, {});
});

test("parseSettings: ignores unknown keys and non-table values", () => {
  const settings = parseSettings(
    [
      "unused = 42",
      "[repo.foo]",
      'group = "/foo"',
      "extra = 99",
    ].join("\n"),
  );
  assert.deepEqual(settings.repos["foo"], { group: "/foo" });
});

test("parseSettings: rejects malformed TOML with a ConfigError", () => {
  assert.throws(() => parseSettings("[broken"), ConfigError);
});

test("parseSettings: parses [env.<name>] sections at the top level", () => {
  const settings = parseSettings(
    [
      "[env.prod]",
      'aws-profile = "company-prod"',
      'region = "us-east-1"',
      "",
      "[env.systest]",
      'aws-profile = "company-systest"',
    ].join("\n"),
  );
  assert.deepEqual(settings.environments, {
    prod: { awsProfile: "company-prod", region: "us-east-1" },
    systest: { awsProfile: "company-systest" },
  });
});

test("parseSettings: parses [repo.<name>.env.<env>] per-repo overrides", () => {
  const settings = parseSettings(
    [
      "[env.prod]",
      'aws-profile = "company-prod"',
      'region = "us-east-1"',
      "",
      "[repo.special-service.env.prod]",
      'aws-profile = "special-prod"',
    ].join("\n"),
  );
  assert.deepEqual(settings.repos["special-service"].envOverrides, {
    prod: { awsProfile: "special-prod" },
  });
});

test("parseSettings: silently drops env names that aren't lower kebab-case", () => {
  const settings = parseSettings(
    [
      "[env.PROD]",
      'aws-profile = "x"',
      "",
      "[env.us_east_1]",
      'aws-profile = "y"',
      "",
      "[env.prod]",
      'aws-profile = "ok"',
    ].join("\n"),
  );
  assert.deepEqual(Object.keys(settings.environments).sort(), ["prod"]);
});

test("isValidEnvironmentName: accepts/rejects per kebab-case rules", () => {
  for (const ok of ["prod", "p", "us-east-1", "a-b-c", "env1"]) {
    assert.equal(isValidEnvironmentName(ok), true, `${ok} should be valid`);
  }
  for (const bad of ["", "PROD", "us_east", "1prod", "-prod", "prod-", "a--b"]) {
    assert.equal(isValidEnvironmentName(bad), false, `${bad} should be invalid`);
  }
});

test("resolveEnvConfig: returns top-level entry when no per-repo override", () => {
  const settings = parseSettings(
    [
      "[env.prod]",
      'aws-profile = "company-prod"',
      'region = "us-east-1"',
    ].join("\n"),
  );
  assert.deepEqual(resolveEnvConfig(settings, null, "prod"), {
    awsProfile: "company-prod",
    region: "us-east-1",
  });
  assert.deepEqual(resolveEnvConfig(settings, "any-repo", "prod"), {
    awsProfile: "company-prod",
    region: "us-east-1",
  });
});

test("resolveEnvConfig: per-repo override merges per key on top of top-level", () => {
  const settings = parseSettings(
    [
      "[env.prod]",
      'aws-profile = "company-prod"',
      'region = "us-east-1"',
      "",
      "[repo.foo.env.prod]",
      'aws-profile = "foo-prod"',
    ].join("\n"),
  );
  assert.deepEqual(resolveEnvConfig(settings, "foo", "prod"), {
    awsProfile: "foo-prod",
    region: "us-east-1",
  });
});

test("resolveEnvConfig: returns empty when no top-level [env.<env>] and no override", () => {
  const settings = parseSettings("");
  assert.deepEqual(resolveEnvConfig(settings, "foo", "prod"), {});
});

test("parseSettings: filters non-string entries from flatten-fields", () => {
  const settings = parseSettings(
    [
      "[defaults]",
      'flatten-fields = ["@message", 42, "context"]',
    ].join("\n"),
  );
  assert.deepEqual(settings.defaults.flattenFields, ["@message", "context"]);
});

test("expandTemplate: substitutes {{ name }} placeholders with whitespace tolerance", () => {
  assert.equal(expandTemplate("/prod/team", { env: "x" }), "/prod/team");
  assert.equal(
    expandTemplate("/{{ env }}/team", { env: "systest" }),
    "/systest/team",
  );
  assert.equal(
    expandTemplate("/{{env}}/{{  env  }}", { env: "uat" }),
    "/uat/uat",
  );
  assert.equal(
    expandTemplate("/{{ env }}/{{ app }}", { env: "prod", app: "svc" }),
    "/prod/svc",
  );
});

test("expandTemplate: errors when a referenced placeholder has no value", () => {
  assert.throws(() => expandTemplate("/{{ env }}/team", {}), ConfigError);
  assert.throws(
    () => expandTemplate("filter app = '{{ app }}'", { env: "prod" }),
    ConfigError,
  );
});

test("defaultQuery: keeps {{ app }} placeholder unexpanded when app is configured", () => {
  assert.equal(
    defaultQuery("my-service"),
    [
      "fields @timestamp, @message",
      "| sort @timestamp desc",
      "| filter app = '{{ app }}'",
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

test("resolveRepoDefaults: per-repo values override defaults; unspecified keys inherit", () => {
  const tmpRoot = realpathSync(mkdtempSync(join(tmpdir(), "cwi-test-")));
  try {
    const repoRoot = join(tmpRoot, "installer-notification");
    mkdirSync(repoRoot);
    assert.equal(
      spawnSync("git", ["init", "-q", "-b", "main"], { cwd: repoRoot }).status,
      0,
      "git init failed",
    );

    assert.equal(findGitRoot(repoRoot), repoRoot);

    const settings = loadSettings(writeSettings(tmpRoot, [
      "[defaults]",
      'flatten-fields = ["@message"]',
      "",
      "[repo.installer-notification]",
      'group = "/{env}/team-icc"',
      'app = "installer-notification"',
    ].join("\n")));

    const { sectionName, defaults } = resolveRepoDefaults(settings, repoRoot);
    assert.equal(sectionName, "installer-notification");
    assert.equal(defaults.group, "/{env}/team-icc");
    assert.equal(defaults.app, "installer-notification");
    assert.deepEqual(defaults.flattenFields, ["@message"]);
  } finally {
    rmSync(tmpRoot, { recursive: true, force: true });
  }
});

test("resolveRepoDefaults: per-repo flatten-fields fully replaces the defaults array", () => {
  const tmpRoot = realpathSync(mkdtempSync(join(tmpdir(), "cwi-replace-")));
  try {
    const repoRoot = join(tmpRoot, "my-service");
    mkdirSync(repoRoot);
    assert.equal(
      spawnSync("git", ["init", "-q", "-b", "main"], { cwd: repoRoot }).status,
      0,
    );

    const settings = loadSettings(writeSettings(tmpRoot, [
      "[defaults]",
      'flatten-fields = ["@message"]',
      "",
      "[repo.my-service]",
      'flatten-fields = ["context"]',
    ].join("\n")));

    const { defaults } = resolveRepoDefaults(settings, repoRoot);
    assert.deepEqual(defaults.flattenFields, ["context"]);
  } finally {
    rmSync(tmpRoot, { recursive: true, force: true });
  }
});

test("resolveRepoDefaults: returns defaults-only when no matching [repo.<name>]", () => {
  const tmpRoot = realpathSync(mkdtempSync(join(tmpdir(), "cwi-nomatch-")));
  try {
    const repoRoot = join(tmpRoot, "unknown-repo");
    mkdirSync(repoRoot);
    assert.equal(
      spawnSync("git", ["init", "-q", "-b", "main"], { cwd: repoRoot }).status,
      0,
    );

    const settings = loadSettings(writeSettings(tmpRoot, [
      "[defaults]",
      'flatten-fields = ["@message"]',
    ].join("\n")));

    const { sectionName, defaults } = resolveRepoDefaults(settings, repoRoot);
    assert.equal(sectionName, "unknown-repo");
    assert.deepEqual(defaults.flattenFields, ["@message"]);
    assert.equal(defaults.group, undefined);
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
    assert.match(contents1, /# \[repo\.my-service\]/);
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
      '[repo.my-service]\ngroup = "/prod/team"\n',
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
