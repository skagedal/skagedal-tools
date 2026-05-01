import type { ManagedField } from "./types";

export function pickField(data: Record<string, unknown>, candidates: string[]): string {
  for (const key of candidates) {
    const value = data[key];
    if (value === undefined || value === null || value === "") continue;
    return typeof value === "string" ? value : safeStringify(value);
  }
  return "";
}

export function stringifyValue(value: unknown): string {
  if (typeof value === "string") return value;
  if (typeof value === "number" || typeof value === "boolean") return String(value);
  return safeStringify(value);
}

export function renderValueDetailed(value: unknown): string {
  if (value === null || value === undefined) return "";
  if (typeof value === "string") return value;
  if (typeof value === "number" || typeof value === "boolean") return String(value);
  return safeStringifyPretty(value);
}

function safeStringify(value: unknown): string {
  try {
    return JSON.stringify(value);
  } catch {
    return String(value);
  }
}

function safeStringifyPretty(value: unknown): string {
  try {
    return JSON.stringify(value, null, 2);
  } catch {
    return String(value);
  }
}

export function classifyLevel(level: string): "error" | "warn" | "info" | "debug" | "other" {
  const v = level.toLowerCase();
  if (v.includes("error") || v === "err" || v === "fatal") return "error";
  if (v.includes("warn")) return "warn";
  if (v.includes("info")) return "info";
  if (v.includes("debug") || v.includes("trace")) return "debug";
  return "other";
}

export function isKeyVisible(fields: ManagedField[], key: string): boolean {
  const f = fields.find((f) => f.from.includes(key));
  return f ? f.visible : false;
}

export function copyText(text: string): Promise<boolean> {
  if (!navigator.clipboard?.writeText) return Promise.resolve(false);
  return navigator.clipboard.writeText(text).then(
    () => true,
    () => false,
  );
}
