/**
 * Per-repo ".insights" query files and their associated run outputs.
 *
 * Directory layout under SKAGEDAL_TOOLS_HOME/cloudwatch-insights/:
 *
 *   latest-run.jsonl                     symlink → most recently written results file
 *   queries/
 *     <slot>/
 *       current.insights                 YAML front-matter + query body
 *       results/
 *         run-<timestamp>.jsonl          one row per line
 *
 * <slot> is the git repo's directory name, or "_default" when run outside
 * a git repository.
 */

import { readFileSync, existsSync, mkdirSync, writeFileSync, lstatSync, unlinkSync, symlinkSync } from "fs";
import { basename, dirname, join } from "path";
import { parse as parseYaml } from "yaml";

import { configPath, defaultQueryForApp, findGitRoot } from "./config.js";

export const FALLBACK_QUERY = "fields @timestamp, @message | sort @timestamp desc";

export interface FrontMatter {
  time?: string;
  environment?: string;
  logGroup?: string | string[];
  limit?: number;
}

export interface QueryFile {
  frontMatter: FrontMatter;
  body: string;
}

/** The skagedal-tools directory used for this tool (same base as configPath). */
export function toolBaseDir(): string {
  return dirname(configPath());
}

/** Slot name used for the queries subtree — repo basename or "_default". */
export function currentSlot(cwd: string = process.cwd()): string {
  const root = findGitRoot(cwd);
  return root ? basename(root) : "_default";
}

export function currentInsightsPath(cwd: string = process.cwd()): string {
  return join(toolBaseDir(), "queries", currentSlot(cwd), "current.insights");
}

export function resultsDir(cwd: string = process.cwd()): string {
  return join(toolBaseDir(), "queries", currentSlot(cwd), "results");
}

export function latestRunPath(): string {
  return join(toolBaseDir(), "latest-run.jsonl");
}

/**
 * Ensure a current.insights file exists for the current slot, seeding it
 * with a template that uses `app` for the default filter when provided.
 * Returns whether a new file was written.
 */
export function ensureCurrentInsights(path: string, app?: string): boolean {
  if (existsSync(path)) return false;
  mkdirSync(dirname(path), { recursive: true });
  writeFileSync(path, seedContent(app), { encoding: "utf8", flag: "wx" });
  return true;
}

export function loadQueryFile(path: string): QueryFile {
  const raw = readFileSync(path, "utf8");
  return parseQueryFile(raw);
}

/**
 * Parse a .insights file: optional YAML front-matter fenced by "---"
 * lines, followed by the query body. If there is no front-matter, the
 * entire file is treated as the body.
 */
export function parseQueryFile(contents: string): QueryFile {
  const stripped = contents.replace(/^﻿/, "");
  const lines = stripped.split(/\r?\n/);
  if (lines.length === 0 || lines[0].trim() !== "---") {
    return { frontMatter: {}, body: stripped.trim() };
  }
  let endIdx = -1;
  for (let i = 1; i < lines.length; i++) {
    if (lines[i].trim() === "---") {
      endIdx = i;
      break;
    }
  }
  if (endIdx === -1) {
    return { frontMatter: {}, body: stripped.trim() };
  }
  const yamlText = lines.slice(1, endIdx).join("\n");
  const body = lines.slice(endIdx + 1).join("\n").trim();
  const parsed = parseYaml(yamlText);
  const frontMatter =
    parsed && typeof parsed === "object" && !Array.isArray(parsed)
      ? (parsed as FrontMatter)
      : {};
  return { frontMatter, body };
}

/** Build a filesystem-safe ISO-ish timestamp (no colons): 2026-04-23T14-30-45Z. */
export function runTimestamp(now: Date = new Date()): string {
  const iso = now.toISOString(); // 2026-04-23T14:30:45.123Z
  return iso.replace(/\.\d+Z$/, "Z").replace(/:/g, "-");
}

export function runResultPath(cwd: string = process.cwd(), now: Date = new Date()): string {
  return join(resultsDir(cwd), `run-${runTimestamp(now)}.jsonl`);
}

export interface WriteResultsOptions {
  path: string;
  rows: Array<Record<string, string>>;
}

/** Write rows as JSONL, creating the parent directory as needed. */
export function writeResults({ path, rows }: WriteResultsOptions): void {
  mkdirSync(dirname(path), { recursive: true });
  const contents = rows.map((r) => JSON.stringify(r)).join("\n") + (rows.length ? "\n" : "");
  writeFileSync(path, contents, "utf8");
}

/**
 * Point latest-run.jsonl at `target`. Replaces any existing symlink or
 * regular file at the path so the tool stays in a consistent state after
 * every run.
 */
export function updateLatestSymlink(target: string): string {
  const linkPath = latestRunPath();
  mkdirSync(dirname(linkPath), { recursive: true });
  try {
    const stat = lstatSync(linkPath);
    if (stat.isSymbolicLink() || stat.isFile()) {
      unlinkSync(linkPath);
    }
  } catch {
    // nothing to remove
  }
  symlinkSync(target, linkPath);
  return linkPath;
}

function seedContent(app?: string): string {
  const query = app ? defaultQueryForApp(app) : FALLBACK_QUERY;
  return `---
# Front-matter (YAML). Optional defaults for this query.
# Examples:
#   time: 5h
#   environment: systest
#   logGroup: /my/group
#   limit: 100
---
${query}
`;
}
