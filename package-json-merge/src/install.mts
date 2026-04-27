import { existsSync, mkdirSync, readFileSync, writeFileSync } from "fs";
import { spawnSync } from "child_process";
import { dirname, join } from "path";
import { homedir } from "os";

const DRIVER_NAME = "skagedal-package-json";
const DRIVER_DESCRIPTION = "package.json semver-aware merge";
const DRIVER_COMMAND = "package-json-merge merge %O %A %B %P";
const ATTRIBUTES_LINE = `package.json merge=${DRIVER_NAME}`;

export interface InstallOptions {
  global: boolean;
}

export function install(opts: InstallOptions): void {
  setGitConfig("merge." + DRIVER_NAME + ".name", DRIVER_DESCRIPTION, opts.global);
  setGitConfig("merge." + DRIVER_NAME + ".driver", DRIVER_COMMAND, opts.global);

  const attributesPath = resolveAttributesPath(opts.global);
  ensureLineInFile(attributesPath, ATTRIBUTES_LINE);

  process.stderr.write(
    `Installed merge driver "${DRIVER_NAME}" ${opts.global ? "globally" : "in this repository"}.\n`,
  );
  process.stderr.write(`  attributes: ${attributesPath}\n`);
}

export function uninstall(opts: InstallOptions): void {
  unsetGitSection("merge." + DRIVER_NAME, opts.global);

  const attributesPath = resolveAttributesPath(opts.global);
  removeLineFromFile(attributesPath, ATTRIBUTES_LINE);

  process.stderr.write(
    `Removed merge driver "${DRIVER_NAME}" ${opts.global ? "globally" : "from this repository"}.\n`,
  );
}

function setGitConfig(key: string, value: string, global: boolean): void {
  const args = ["config"];
  if (global) args.push("--global");
  args.push(key, value);
  const r = spawnSync("git", args, { stdio: "inherit" });
  if (r.status !== 0) throw new Error(`git ${args.join(" ")} failed`);
}

function unsetGitSection(section: string, global: boolean): void {
  const args = ["config"];
  if (global) args.push("--global");
  args.push("--remove-section", section);
  // Allow non-zero exit (section may not exist).
  spawnSync("git", args, { stdio: "ignore" });
}

function resolveAttributesPath(global: boolean): string {
  if (!global) {
    const top = gitTopLevel();
    return join(top, ".git", "info", "attributes");
  }
  const fromConfig = spawnSync("git", ["config", "--global", "core.attributesFile"], {
    encoding: "utf8",
  });
  if (fromConfig.status === 0) {
    const path = fromConfig.stdout.trim();
    if (path) return expandHome(path);
  }
  const xdg = process.env.XDG_CONFIG_HOME;
  const base = xdg && xdg.length > 0 ? xdg : join(homedir(), ".config");
  return join(base, "git", "attributes");
}

function gitTopLevel(): string {
  const r = spawnSync("git", ["rev-parse", "--show-toplevel"], { encoding: "utf8" });
  if (r.status !== 0) throw new Error("not inside a git repository");
  return r.stdout.trim();
}

function expandHome(p: string): string {
  if (p === "~" || p.startsWith("~/")) return join(homedir(), p.slice(1));
  return p;
}

function ensureLineInFile(path: string, line: string): void {
  mkdirSync(dirname(path), { recursive: true });
  let content = "";
  if (existsSync(path)) content = readFileSync(path, "utf8");
  const lines = content.split("\n");
  if (lines.some((l) => l.trim() === line)) return;
  if (content.length > 0 && !content.endsWith("\n")) content += "\n";
  content += line + "\n";
  writeFileSync(path, content);
}

function removeLineFromFile(path: string, line: string): void {
  if (!existsSync(path)) return;
  const content = readFileSync(path, "utf8");
  const filtered = content
    .split("\n")
    .filter((l) => l.trim() !== line)
    .join("\n");
  writeFileSync(path, filtered);
}
