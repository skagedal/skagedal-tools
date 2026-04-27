#!/usr/bin/env node

import { cli, define } from "gunshi";
import { runMerge } from "./merge.mjs";
import { install, uninstall } from "./install.mjs";

const DESCRIPTION =
  "Git merge driver for package.json. When both branches bump the same dependency, " +
  "the higher semver range wins. Other conflicts fall through to git's default merge.";

const mergeCmd = define({
  name: "merge",
  description: "Run the merge (called by git: %O %A %B %P)",
  args: {
    base: { type: "positional", description: "ancestor file (%O)" },
    ours: { type: "positional", description: "current file (%A) — written in place" },
    theirs: { type: "positional", description: "other file (%B)" },
    path: {
      type: "positional",
      description: "original pathname (%P, optional)",
      required: false,
    },
  },
  run: (ctx) => {
    const { base, ours, theirs } = ctx.values as {
      base: string;
      ours: string;
      theirs: string;
    };
    const result = runMerge(base, ours, theirs);
    if (result.conflicted) {
      if (result.reason) {
        process.stderr.write(`package-json-merge: falling back (${result.reason})\n`);
      }
      process.exit(1);
    }
    for (const r of result.resolutions ?? []) {
      process.stderr.write(
        `package-json-merge: resolved ${r.name} (${r.ours} + ${r.theirs} → ${r.picked}) in ${r.section}\n`,
      );
    }
  },
});

const installCmd = define({
  name: "install",
  description: "Wire up the merge driver via git config and gitattributes",
  args: {
    global: {
      type: "boolean",
      short: "g",
      default: false,
      description: "install globally (~/.gitconfig + global attributes file) instead of in the current repo",
    },
  },
  run: (ctx) => {
    install({ global: ctx.values.global });
  },
});

const uninstallCmd = define({
  name: "uninstall",
  description: "Remove the merge driver wiring",
  args: {
    global: {
      type: "boolean",
      short: "g",
      default: false,
      description: "remove the global wiring instead of the per-repo wiring",
    },
  },
  run: (ctx) => {
    uninstall({ global: ctx.values.global });
  },
});

const main = define({
  name: "package-json-merge",
  description: DESCRIPTION,
  args: {},
  run: () => {
    process.stderr.write(
      "Usage: package-json-merge <merge|install|uninstall>\n" +
        "Run `package-json-merge --help` for details.\n",
    );
    process.exit(2);
  },
});

try {
  await cli(process.argv.slice(2), main, {
    name: "package-json-merge",
    version: "0.1.0",
    description: DESCRIPTION,
    renderHeader: null,
    subCommands: {
      merge: mergeCmd,
      install: installCmd,
      uninstall: uninstallCmd,
    },
  });
} catch (err) {
  const message = err instanceof Error ? err.message : String(err);
  process.stderr.write(`error: ${message}\n`);
  process.exit(1);
}
