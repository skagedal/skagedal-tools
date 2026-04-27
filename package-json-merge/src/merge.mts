import { readFileSync, writeFileSync } from "fs";
import { spawnSync } from "child_process";
import semver from "semver";

const DEP_KEYS = new Set([
  "dependencies",
  "devDependencies",
  "peerDependencies",
  "optionalDependencies",
]);

export interface Resolution {
  section: string;
  name: string;
  ours: string;
  theirs: string;
  picked: string;
}

export interface MergeOutcome {
  conflicted: boolean;
  reason?: string;
  resolutions?: Resolution[];
}

export function runMerge(basePath: string, oursPath: string, theirsPath: string): MergeOutcome {
  const oursRaw = readFileSync(oursPath, "utf8");
  const baseRaw = readFileSafe(basePath);
  const theirsRaw = readFileSafe(theirsPath);

  if (baseRaw === null || theirsRaw === null) {
    return fallback(basePath, oursPath, theirsPath, "missing base or theirs file");
  }

  let base: unknown, ours: unknown, theirs: unknown;
  try {
    base = JSON.parse(baseRaw);
    ours = JSON.parse(oursRaw);
    theirs = JSON.parse(theirsRaw);
  } catch (err) {
    return fallback(basePath, oursPath, theirsPath, `JSON parse failed: ${(err as Error).message}`);
  }

  if (!isObject(base) || !isObject(ours) || !isObject(theirs)) {
    return fallback(basePath, oursPath, theirsPath, "top-level value is not a JSON object");
  }

  const resolutions: Resolution[] = [];
  const result = mergeObject(base, ours, theirs, resolutions);
  if (result.conflicted) {
    return fallback(basePath, oursPath, theirsPath, "structural conflict outside of dependencies");
  }

  const indent = detectIndent(oursRaw);
  const trailingNewline = oursRaw.endsWith("\n") ? "\n" : "";
  writeFileSync(oursPath, JSON.stringify(result.value, null, indent) + trailingNewline);
  return { conflicted: false, resolutions };
}

interface ValueResult {
  value: unknown;
  conflicted: boolean;
}

function mergeValue(
  base: unknown,
  ours: unknown,
  theirs: unknown,
  parentKey: string | undefined,
  resolutions: Resolution[],
): ValueResult {
  if (deepEqual(ours, theirs)) return { value: ours, conflicted: false };
  if (deepEqual(ours, base)) return { value: theirs, conflicted: false };
  if (deepEqual(theirs, base)) return { value: ours, conflicted: false };

  // Both diverged from base.
  if (parentKey && DEP_KEYS.has(parentKey) && isObject(ours) && isObject(theirs)) {
    return mergeDeps(asObject(base), ours, theirs, parentKey, resolutions);
  }
  if (isObject(ours) && isObject(theirs)) {
    return mergeObject(asObject(base), ours, theirs, resolutions);
  }
  return { value: ours, conflicted: true };
}

function mergeObject(
  base: Record<string, unknown>,
  ours: Record<string, unknown>,
  theirs: Record<string, unknown>,
  resolutions: Resolution[],
): ValueResult {
  const merged: Record<string, unknown> = {};
  let conflicted = false;
  for (const k of unionKeys(ours, theirs)) {
    const inOurs = k in ours;
    const inTheirs = k in theirs;
    const inBase = k in base;

    if (inOurs && inTheirs) {
      const r = mergeValue(base[k], ours[k], theirs[k], k, resolutions);
      if (r.conflicted) conflicted = true;
      merged[k] = r.value;
    } else if (inOurs) {
      // theirs deleted (or never had it)
      if (inBase && !deepEqual(base[k], ours[k])) {
        conflicted = true;
        merged[k] = ours[k];
      } else if (!inBase) {
        merged[k] = ours[k];
      }
      // else: theirs deleted unchanged value -> drop
    } else {
      // inTheirs only
      if (inBase && !deepEqual(base[k], theirs[k])) {
        conflicted = true;
        merged[k] = theirs[k];
      } else if (!inBase) {
        merged[k] = theirs[k];
      }
    }
  }
  return { value: merged, conflicted };
}

function mergeDeps(
  base: Record<string, unknown>,
  ours: Record<string, unknown>,
  theirs: Record<string, unknown>,
  section: string,
  resolutions: Resolution[],
): ValueResult {
  const merged: Record<string, unknown> = {};
  let conflicted = false;
  for (const name of unionKeys(ours, theirs)) {
    const oursV = ours[name];
    const theirsV = theirs[name];
    const baseV = base[name];
    const inOurs = name in ours;
    const inTheirs = name in theirs;
    const inBase = name in base;

    if (inOurs && inTheirs) {
      if (deepEqual(oursV, theirsV)) {
        merged[name] = oursV;
      } else if (deepEqual(oursV, baseV)) {
        merged[name] = theirsV;
      } else if (deepEqual(theirsV, baseV)) {
        merged[name] = oursV;
      } else {
        const higher = pickHigherRange(oursV, theirsV);
        if (higher === null) {
          conflicted = true;
          merged[name] = oursV;
        } else {
          merged[name] = higher;
          resolutions.push({
            section,
            name,
            ours: String(oursV),
            theirs: String(theirsV),
            picked: higher,
          });
        }
      }
    } else if (inOurs) {
      if (inBase && !deepEqual(baseV, oursV)) {
        conflicted = true;
        merged[name] = oursV;
      } else if (!inBase) {
        merged[name] = oursV;
      }
    } else {
      if (inBase && !deepEqual(baseV, theirsV)) {
        conflicted = true;
        merged[name] = theirsV;
      } else if (!inBase) {
        merged[name] = theirsV;
      }
    }
  }
  return { value: merged, conflicted };
}

export function pickHigherRange(a: unknown, b: unknown): string | null {
  if (typeof a !== "string" || typeof b !== "string") return null;
  const aRange = semver.validRange(a);
  const bRange = semver.validRange(b);
  if (!aRange || !bRange) return null;
  const aMin = semver.minVersion(aRange);
  const bMin = semver.minVersion(bRange);
  if (!aMin || !bMin) return null;
  return semver.gte(aMin, bMin) ? a : b;
}

function unionKeys(a: object, b: object): string[] {
  const out: string[] = [];
  const seen = new Set<string>();
  for (const k of Object.keys(a)) {
    out.push(k);
    seen.add(k);
  }
  for (const k of Object.keys(b)) {
    if (!seen.has(k)) {
      out.push(k);
      seen.add(k);
    }
  }
  return out;
}

function isObject(v: unknown): v is Record<string, unknown> {
  return typeof v === "object" && v !== null && !Array.isArray(v);
}

function asObject(v: unknown): Record<string, unknown> {
  return isObject(v) ? v : {};
}

function deepEqual(a: unknown, b: unknown): boolean {
  if (a === b) return true;
  if (typeof a !== typeof b) return false;
  if (a === null || b === null) return false;
  if (Array.isArray(a)) {
    if (!Array.isArray(b) || a.length !== b.length) return false;
    return a.every((v, i) => deepEqual(v, b[i]));
  }
  if (typeof a === "object" && typeof b === "object") {
    const ao = a as Record<string, unknown>;
    const bo = b as Record<string, unknown>;
    const aKeys = Object.keys(ao);
    const bKeys = Object.keys(bo);
    if (aKeys.length !== bKeys.length) return false;
    return aKeys.every((k) => k in bo && deepEqual(ao[k], bo[k]));
  }
  return false;
}

function readFileSafe(path: string): string | null {
  try {
    return readFileSync(path, "utf8");
  } catch {
    return null;
  }
}

function detectIndent(content: string): number {
  const m = content.match(/\n([ \t]+)"/);
  if (!m) return 2;
  const ws = m[1];
  if (ws.includes("\t")) return 2;
  return ws.length;
}

function fallback(
  basePath: string,
  oursPath: string,
  theirsPath: string,
  reason: string,
): MergeOutcome {
  spawnSync(
    "git",
    [
      "merge-file",
      "-L",
      "ours",
      "-L",
      "base",
      "-L",
      "theirs",
      oursPath,
      basePath,
      theirsPath,
    ],
    { stdio: "inherit" },
  );
  return { conflicted: true, reason };
}
