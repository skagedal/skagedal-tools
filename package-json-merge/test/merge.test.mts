import { test } from "node:test";
import assert from "node:assert/strict";
import { mkdtempSync, readFileSync, writeFileSync } from "fs";
import { tmpdir } from "os";
import { join } from "path";

import { pickHigherRange, runMerge } from "../src/merge.mjs";

function makeCase(base: object, ours: object, theirs: object) {
  const dir = mkdtempSync(join(tmpdir(), "package-json-merge-"));
  const basePath = join(dir, "base.json");
  const oursPath = join(dir, "ours.json");
  const theirsPath = join(dir, "theirs.json");
  writeFileSync(basePath, JSON.stringify(base, null, 2) + "\n");
  writeFileSync(oursPath, JSON.stringify(ours, null, 2) + "\n");
  writeFileSync(theirsPath, JSON.stringify(theirs, null, 2) + "\n");
  return { basePath, oursPath, theirsPath };
}

function readMerged(path: string): unknown {
  return JSON.parse(readFileSync(path, "utf8"));
}

test("pickHigherRange: caret ranges", () => {
  assert.equal(pickHigherRange("^1.2.3", "^1.5.0"), "^1.5.0");
  assert.equal(pickHigherRange("^2.0.0", "^1.9.9"), "^2.0.0");
});

test("pickHigherRange: tilde and exact", () => {
  assert.equal(pickHigherRange("~1.2.3", "1.2.4"), "1.2.4");
  assert.equal(pickHigherRange("1.0.0", "1.0.0"), "1.0.0");
});

test("pickHigherRange: invalid input returns null", () => {
  assert.equal(pickHigherRange("workspace:*", "^1.0.0"), null);
  assert.equal(pickHigherRange("^1.0.0", "github:foo/bar"), null);
  assert.equal(pickHigherRange(42, "^1.0.0"), null);
});

test("both bumped same dep: higher version wins", () => {
  const { basePath, oursPath, theirsPath } = makeCase(
    { dependencies: { foo: "^1.0.0" } },
    { dependencies: { foo: "^1.5.0" } },
    { dependencies: { foo: "^1.2.0" } },
  );
  const result = runMerge(basePath, oursPath, theirsPath);
  assert.equal(result.conflicted, false);
  assert.deepEqual(readMerged(oursPath), { dependencies: { foo: "^1.5.0" } });
  assert.deepEqual(result.resolutions, [
    { section: "dependencies", name: "foo", ours: "^1.5.0", theirs: "^1.2.0", picked: "^1.5.0" },
  ]);
});

test("clean merge with no conflicts has no resolutions", () => {
  const { basePath, oursPath, theirsPath } = makeCase(
    { dependencies: { foo: "^1.0.0" } },
    { dependencies: { foo: "^2.0.0" } },
    { dependencies: { foo: "^1.0.0" } },
  );
  const result = runMerge(basePath, oursPath, theirsPath);
  assert.equal(result.conflicted, false);
  assert.deepEqual(result.resolutions, []);
});

test("only ours bumped: ours wins", () => {
  const { basePath, oursPath, theirsPath } = makeCase(
    { dependencies: { foo: "^1.0.0" } },
    { dependencies: { foo: "^2.0.0" } },
    { dependencies: { foo: "^1.0.0" } },
  );
  const result = runMerge(basePath, oursPath, theirsPath);
  assert.equal(result.conflicted, false);
  assert.deepEqual(readMerged(oursPath), { dependencies: { foo: "^2.0.0" } });
});

test("non-overlapping deps from both sides are unioned", () => {
  const { basePath, oursPath, theirsPath } = makeCase(
    { dependencies: { foo: "^1.0.0" } },
    { dependencies: { foo: "^1.0.0", bar: "^1.0.0" } },
    { dependencies: { foo: "^1.0.0", baz: "^2.0.0" } },
  );
  const result = runMerge(basePath, oursPath, theirsPath);
  assert.equal(result.conflicted, false);
  assert.deepEqual(readMerged(oursPath), {
    dependencies: { foo: "^1.0.0", bar: "^1.0.0", baz: "^2.0.0" },
  });
});

test("dep added on both sides at different versions: higher wins", () => {
  const { basePath, oursPath, theirsPath } = makeCase(
    {},
    { dependencies: { foo: "^1.2.0" } },
    { dependencies: { foo: "^1.5.0" } },
  );
  const result = runMerge(basePath, oursPath, theirsPath);
  assert.equal(result.conflicted, false);
  assert.deepEqual(readMerged(oursPath), { dependencies: { foo: "^1.5.0" } });
});

test("dep removed by one side, modified by other: conflict", () => {
  const { basePath, oursPath, theirsPath } = makeCase(
    { dependencies: { foo: "^1.0.0" } },
    { dependencies: { foo: "^2.0.0" } },
    { dependencies: {} },
  );
  const result = runMerge(basePath, oursPath, theirsPath);
  assert.equal(result.conflicted, true);
});

test("dep removed by one side, untouched by other: removal applied", () => {
  const { basePath, oursPath, theirsPath } = makeCase(
    { dependencies: { foo: "^1.0.0", bar: "^1.0.0" } },
    { dependencies: { foo: "^1.0.0" } },
    { dependencies: { foo: "^1.0.0", bar: "^1.0.0" } },
  );
  const result = runMerge(basePath, oursPath, theirsPath);
  assert.equal(result.conflicted, false);
  assert.deepEqual(readMerged(oursPath), { dependencies: { foo: "^1.0.0" } });
});

test("non-version conflict on the same dep falls back", () => {
  const { basePath, oursPath, theirsPath } = makeCase(
    { dependencies: { foo: "^1.0.0" } },
    { dependencies: { foo: "workspace:*" } },
    { dependencies: { foo: "github:other/repo" } },
  );
  const result = runMerge(basePath, oursPath, theirsPath);
  assert.equal(result.conflicted, true);
});

test("conflicting non-dep top-level keys fall back", () => {
  const { basePath, oursPath, theirsPath } = makeCase(
    { name: "pkg", version: "1.0.0" },
    { name: "pkg", version: "1.1.0" },
    { name: "pkg", version: "1.2.0" },
  );
  const result = runMerge(basePath, oursPath, theirsPath);
  assert.equal(result.conflicted, true);
});

test("non-conflicting changes across different top-level keys merge cleanly", () => {
  const { basePath, oursPath, theirsPath } = makeCase(
    { name: "pkg", version: "1.0.0", dependencies: { foo: "^1.0.0" } },
    { name: "pkg", version: "1.1.0", dependencies: { foo: "^1.0.0" } },
    { name: "pkg", version: "1.0.0", dependencies: { foo: "^1.5.0" } },
  );
  const result = runMerge(basePath, oursPath, theirsPath);
  assert.equal(result.conflicted, false);
  assert.deepEqual(readMerged(oursPath), {
    name: "pkg",
    version: "1.1.0",
    dependencies: { foo: "^1.5.0" },
  });
});

test("devDependencies and peerDependencies also use semver-max", () => {
  const { basePath, oursPath, theirsPath } = makeCase(
    {
      devDependencies: { tsx: "^4.0.0" },
      peerDependencies: { react: "^18.0.0" },
    },
    {
      devDependencies: { tsx: "^4.5.0" },
      peerDependencies: { react: "^18.2.0" },
    },
    {
      devDependencies: { tsx: "^4.3.0" },
      peerDependencies: { react: "^19.0.0" },
    },
  );
  const result = runMerge(basePath, oursPath, theirsPath);
  assert.equal(result.conflicted, false);
  assert.deepEqual(readMerged(oursPath), {
    devDependencies: { tsx: "^4.5.0" },
    peerDependencies: { react: "^19.0.0" },
  });
});

test("indentation of ours is preserved", () => {
  const dir = mkdtempSync(join(tmpdir(), "package-json-merge-"));
  const basePath = join(dir, "base.json");
  const oursPath = join(dir, "ours.json");
  const theirsPath = join(dir, "theirs.json");
  writeFileSync(basePath, JSON.stringify({ dependencies: { foo: "^1.0.0" } }, null, 2) + "\n");
  writeFileSync(oursPath, JSON.stringify({ dependencies: { foo: "^1.5.0" } }, null, 4) + "\n");
  writeFileSync(theirsPath, JSON.stringify({ dependencies: { foo: "^1.2.0" } }, null, 2) + "\n");
  const result = runMerge(basePath, oursPath, theirsPath);
  assert.equal(result.conflicted, false);
  const merged = readFileSync(oursPath, "utf8");
  assert.match(merged, /\n {4}"dependencies"/);
});
