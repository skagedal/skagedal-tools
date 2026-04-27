/**
 * Per-repo ".insights" query files and their associated run outputs.
 *
 * Directory layout under SKAGEDAL_TOOLS_HOME/cloudwatch-insights/:
 *
 *   latest-run.jsonl                     symlink → most recently written results file
 *   queries/
 *     <slot>/
 *       current.insights                 TOML front-matter + query body
 *       results/
 *         run-<timestamp>.jsonl          one row per line
 *
 * <slot> is the git repo's directory name, or "_default" when run outside
 * a git repository.
 */

import { readFileSync, existsSync, mkdirSync, writeFileSync, lstatSync, unlinkSync, symlinkSync } from "fs";
import { basename, dirname, join } from "path";
import { parse as parseToml, stringify as stringifyToml } from "smol-toml";

import { configPath, defaultQuery, findGitRoot } from "./config.mjs";

export interface FrontMatter {
  time?: string;
  env?: string;
  app?: string;
  "log-group"?: string | string[];
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
 * Values pre-filled into the seeded front-matter so the user can see exactly
 * what will be executed, and edit them before saving.
 */
export interface SeedFrontMatter {
  time?: string;
  env?: string;
  app?: string;
  "log-group"?: string | string[];
}

export interface SeedOptions {
  app?: string;
  frontMatter?: SeedFrontMatter;
}

/**
 * Ensure a current.insights file exists for the current slot, seeding it
 * with a template that uses `app` for the default filter when provided and
 * pre-fills the TOML front-matter with the supplied values. Returns whether
 * a new file was written.
 */
export function ensureCurrentInsights(path: string, options: SeedOptions = {}): boolean {
  if (existsSync(path)) return false;
  mkdirSync(dirname(path), { recursive: true });
  writeFileSync(path, seedContent(options), { encoding: "utf8", flag: "wx" });
  return true;
}

/**
 * Unconditionally reset current.insights to the default template (used
 * by `query --new`). Returns the path that was written.
 */
export function resetCurrentInsights(path: string, options: SeedOptions = {}): string {
  mkdirSync(dirname(path), { recursive: true });
  writeFileSync(path, seedContent(options), "utf8");
  return path;
}

export function loadQueryFile(path: string): QueryFile {
  const raw = readFileSync(path, "utf8");
  return parseQueryFile(raw);
}

/**
 * Parse a .insights file: optional TOML front-matter followed by a single
 * "---" separator line and then the query body. If there is no separator,
 * the entire file is treated as the body.
 */
export function parseQueryFile(contents: string): QueryFile {
  const stripped = contents.replace(/^﻿/, "");
  const lines = stripped.split(/\r?\n/);
  const sepIdx = lines.findIndex((l) => l.trim() === "---");
  if (sepIdx === -1) {
    return { frontMatter: {}, body: stripped.trim() };
  }
  const tomlText = lines.slice(0, sepIdx).join("\n");
  const body = lines.slice(sepIdx + 1).join("\n").trim();
  const parsed = parseToml(tomlText);
  return { frontMatter: parsed as FrontMatter, body };
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
  rows: Array<Record<string, unknown>>;
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

function seedContent({ app, frontMatter = {} }: SeedOptions): string {
  return `${renderFrontMatter(frontMatter)}---
${defaultQuery(app)}
`;
}

function renderFrontMatter(fm: SeedFrontMatter): string {
  const obj: Record<string, unknown> = {};
  if (fm.time !== undefined) obj.time = fm.time;
  if (fm.env !== undefined) obj.env = fm.env;
  if (fm.app !== undefined) obj.app = fm.app;
  if (fm["log-group"] !== undefined) obj["log-group"] = fm["log-group"];
  if (Object.keys(obj).length === 0) return "";
  return stringifyToml(obj) + "\n";
}
