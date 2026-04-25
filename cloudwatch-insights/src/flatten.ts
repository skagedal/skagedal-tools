import { QueryRow } from "./insights.js";

/**
 * For each field listed in `fieldsToFlatten`, parse its value as a JSON
 * object and merge the resulting keys into the row, removing the original
 * field. If the value is missing, isn't valid JSON, or doesn't decode to a
 * plain object, the field is left untouched.
 *
 * On key collision, the flattened keys win — the assumption is that the
 * caller wants to surface the inner structure as the row's primary shape.
 */
export function flattenRow(
  row: QueryRow,
  fieldsToFlatten: readonly string[],
): Record<string, unknown> {
  if (fieldsToFlatten.length === 0) return { ...row };

  const out: Record<string, unknown> = { ...row };
  for (const field of fieldsToFlatten) {
    const raw = out[field];
    if (typeof raw !== "string") continue;
    const parsed = tryParseJson(raw);
    if (parsed === undefined || !isPlainObject(parsed)) continue;

    delete out[field];
    for (const [key, value] of Object.entries(parsed)) {
      out[key] = value;
    }
  }
  return out;
}

function tryParseJson(raw: string): unknown {
  try {
    return JSON.parse(raw);
  } catch {
    return undefined;
  }
}

function isPlainObject(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
