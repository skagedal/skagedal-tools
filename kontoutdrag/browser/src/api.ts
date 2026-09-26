// The server's endpoints; see src/web.rs.

import { Data } from "./data";

export interface Version {
  data: string;
  comments: string;
  /** Set when the rules on disk failed to load; the last good data stays. */
  error: string | null;
}

async function get<T>(path: string): Promise<T> {
  const r = await fetch(path, { cache: "no-store" });
  if (!r.ok) throw new Error(`${path}: HTTP ${r.status}`);
  return r.json();
}

export const fetchData = () => get<Data>("/api/data");
export const fetchVersion = () => get<Version>("/api/version");

export async function fetchComments(): Promise<Map<string, string>> {
  const body = await get<{ comments: { key: string; comment: string }[] }>("/api/comments");
  return new Map(body.comments.map((c) => [c.key, c.comment]));
}

export async function saveComment(key: string, comment: string): Promise<void> {
  const r = await fetch("/api/comment", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ key, comment }),
  });
  if (!r.ok) {
    const body = await r.json().catch(() => ({}));
    throw new Error(body.error ?? `HTTP ${r.status}`);
  }
}
