import type { LogEntry, ManagedField } from "./types";
import { pickField } from "./util";

/**
 * Subsequence ("fuzzy") match: every character of `needle` must appear in
 * `haystack` in order, case-insensitively. Empty needle matches anything.
 * Mirrors the TUI implementation in src/tui/filter.ts.
 */
export function fuzzyMatch(needle: string, haystack: string): boolean {
  if (needle.length === 0) return true;
  const n = needle.toLowerCase();
  const h = haystack.toLowerCase();
  let hi = 0;
  for (let ni = 0; ni < n.length; ni++) {
    const c = n.charCodeAt(ni);
    while (hi < h.length && h.charCodeAt(hi) !== c) hi++;
    if (hi >= h.length) return false;
    hi++;
  }
  return true;
}

/** Build the searchable string for an entry — the rendered values of every
 *  visible column, joined with spaces. */
export function entryHaystack(entry: LogEntry, fields: ManagedField[]): string {
  if (fields.length === 0) return "";
  let out = "";
  let first = true;
  for (const field of fields) {
    if (!field.visible) continue;
    if (!first) out += " ";
    first = false;
    out += pickField(entry.data, field.from);
  }
  return out;
}
