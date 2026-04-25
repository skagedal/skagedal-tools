// Pure helpers used by the TUI. Extracted so they can be unit-tested without
// pulling in terminal-kit or the linear CLI.

export interface Row {
  label: string;
  value: string;
}

export interface NotificationLike {
  type: string;
  createdAt: string;
  url: string;
  actor: { name: string } | null;
  issue?: { identifier: string; title: string } | null;
  project?: { name: string } | null;
  pullRequest?: { title: string; number: number; url: string } | null;
}

export function formatTime(iso: string, now: Date = new Date()): string {
  const date = new Date(iso);
  const diffMs = now.getTime() - date.getTime();
  const diffMins = Math.floor(diffMs / 60000);
  if (diffMins < 60) return `${diffMins}m ago`;
  const diffHours = Math.floor(diffMins / 60);
  if (diffHours < 24) return `${diffHours}h ago`;
  const diffDays = Math.floor(diffHours / 24);
  return `${diffDays}d ago`;
}

export function truncate(str: string, maxLen: number): string {
  if (maxLen <= 0) return "";
  if (str.length <= maxLen) return str;
  return str.slice(0, maxLen - 1) + "…";
}

export function wrapText(text: string, cols: number): string[] {
  const out: string[] = [];
  for (const paragraph of text.split("\n")) {
    if (paragraph.length === 0) {
      out.push("");
      continue;
    }
    const words = paragraph.split(/\s+/);
    let current = "";
    for (const word of words) {
      if (word.length === 0) continue;
      if (current.length === 0) {
        current = word;
      } else if (current.length + 1 + word.length <= cols) {
        current += " " + word;
      } else {
        out.push(current);
        current = word;
      }
    }
    if (current.length > 0) out.push(current);
  }
  return out;
}

export function countStatus(count: number): string {
  return `${count} unread notification${count !== 1 ? "s" : ""}`;
}

export function metadataRows(
  n: NotificationLike,
  now: Date = new Date()
): Row[] {
  const rows: Row[] = [
    { label: "Type", value: n.type },
    { label: "Actor", value: n.actor?.name ?? "—" },
    { label: "Time", value: `${formatTime(n.createdAt, now)} (${n.createdAt})` },
  ];
  if (n.issue)
    rows.push({
      label: "Issue",
      value: `${n.issue.identifier} — ${n.issue.title}`,
    });
  if (n.project) rows.push({ label: "Project", value: n.project.name });
  if (n.pullRequest)
    rows.push({
      label: "PR",
      value: `#${n.pullRequest.number} ${n.pullRequest.title}`,
    });
  rows.push({ label: "URL", value: n.url });
  return rows;
}
