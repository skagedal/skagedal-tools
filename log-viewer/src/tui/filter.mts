import type { FieldConfig } from "../config.mjs";
import type { LogEntry } from "../entry.mjs";
import { renderField } from "../entry.mjs";

/**
 * Subsequence ("fuzzy") match: every character of `needle` must appear in
 * `haystack` in order, case-insensitively. Empty needle matches anything.
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

/** Build the searchable string for an entry — currently the rendered values
 *  of every visible column, joined with spaces. */
export function entryHaystack(entry: LogEntry, fields: FieldConfig[]): string {
  if (fields.length === 0) return "";
  let out = "";
  for (let i = 0; i < fields.length; i++) {
    if (i > 0) out += " ";
    out += renderField(entry, fields[i]!);
  }
  return out;
}

/** Emacs-style C-w: delete the word to the left of the cursor, including any
 *  whitespace between the cursor and that word. */
export function killWordLeft(text: string, cursor: number): { text: string; cursor: number } {
  if (cursor === 0) return { text, cursor };
  let i = cursor;
  while (i > 0 && /\s/.test(text[i - 1]!)) i--;
  while (i > 0 && !/\s/.test(text[i - 1]!)) i--;
  return { text: text.slice(0, i) + text.slice(cursor), cursor: i };
}

/** Drop control characters that ink may forward as `input` (e.g. stray
 *  escape sequences) so they don't end up in the filter buffer. */
export function stripUnprintable(input: string): string {
  let out = "";
  for (const ch of input) {
    const code = ch.codePointAt(0)!;
    if (code === 0x7f) continue; // DEL
    if (code < 0x20) continue;   // C0 controls
    out += ch;
  }
  return out;
}
