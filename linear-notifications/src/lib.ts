// Pure helpers used by the TUI. Extracted so they can be unit-tested without
// pulling in terminal-kit or the linear CLI.

export interface Row {
  label: string;
  value: string;
}

// A single rendered line. `label` is drawn dimmed just before `text`, so the
// metadata blocks keep their two-tone look while still being plain data.
export type Style = "plain" | "bold" | "dim" | "title" | "red";

export interface Line {
  text: string;
  style?: Style;
  label?: string;
}

export interface CommentLike {
  body: string;
  createdAt: string;
  user: { name: string } | null;
}

export interface ThreadComment extends CommentLike {
  parent?: CommentLike | null;
  children?: { nodes: CommentLike[] } | null;
}

export interface DocumentLike {
  title: string;
  url: string;
  createdAt: string;
  creator: { name: string } | null;
}

export interface IssueLike {
  identifier: string;
  title: string;
  description: string | null;
  url: string;
  createdAt: string;
  state: { name: string } | null;
  priorityLabel: string | null;
  assignee: { name: string } | null;
  creator: { name: string } | null;
  labels: { nodes: Array<{ name: string }> };
  comments: { nodes: CommentLike[] };
}

export interface NotificationLike {
  type: string;
  createdAt: string;
  url: string;
  actor: { name: string } | null;
  title?: string;
  subtitle?: string | null;
  issue?: { identifier: string; title: string } | null;
  project?: { name: string } | null;
  pullRequest?: { title: string; number: number; url: string } | null;
  comment?: { body: string } | null;
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

export function labeledLines(rows: Row[], width: number): Line[] {
  if (rows.length === 0) return [];
  const labelW = Math.max(...rows.map((r) => r.label.length)) + 2;
  return rows.map(({ label, value }) => ({
    label: (label + ":").padEnd(labelW),
    text: truncate(value, Math.max(0, width - labelW)),
  }));
}

function byline(c: CommentLike, now: Date): string {
  return `${c.user?.name ?? "?"} · ${formatTime(c.createdAt, now)}`;
}

/** A comment as it appears in a list: dim byline, then the whole body. */
export function commentLines(
  c: CommentLike,
  width: number,
  now: Date = new Date()
): Line[] {
  const lines: Line[] = [
    { text: truncate(`— ${byline(c, now)}`, width), style: "dim" },
  ];
  for (const line of wrapText(c.body, width)) lines.push({ text: line });
  lines.push({ text: "" });
  return lines;
}

/** The preview shown under the notification list. */
export function previewLines(
  n: NotificationLike,
  width: number,
  now: Date = new Date()
): Line[] {
  const lines: Line[] = [
    { text: truncate(n.title ?? "", width), style: "bold" },
  ];

  // Document notifications carry the comment text in the subtitle, so use that
  // as the body rather than cropping it to a single line.
  const body = n.comment?.body ?? n.subtitle ?? "";
  const bodyIsComment = Boolean(n.comment?.body);
  if (n.subtitle && bodyIsComment) {
    lines.push({ text: truncate(n.subtitle, width), style: "dim" });
  }

  lines.push({ text: "" });
  lines.push(...labeledLines(metadataRows(n, now), width));

  if (body) {
    lines.push({ text: "" });
    lines.push({ text: bodyIsComment ? "Comment" : "Summary", style: "bold" });
    for (const line of wrapText(body, width)) lines.push({ text: line });
  }

  return lines;
}

/** The detail view for a notification that points at an issue. */
export function issueLines(
  issue: IssueLike,
  width: number,
  now: Date = new Date()
): Line[] {
  const lines: Line[] = [
    {
      text: truncate(`${issue.identifier}  ${issue.title}`, width),
      style: "title",
    },
    { text: "" },
  ];

  const meta: Row[] = [];
  if (issue.state) meta.push({ label: "State", value: issue.state.name });
  if (issue.priorityLabel)
    meta.push({ label: "Priority", value: issue.priorityLabel });
  if (issue.assignee)
    meta.push({ label: "Assignee", value: issue.assignee.name });
  if (issue.creator) meta.push({ label: "Creator", value: issue.creator.name });
  meta.push({
    label: "Created",
    value: `${formatTime(issue.createdAt, now)} (${issue.createdAt})`,
  });
  const labelNames = issue.labels?.nodes?.map((l) => l.name) ?? [];
  if (labelNames.length)
    meta.push({ label: "Labels", value: labelNames.join(", ") });
  meta.push({ label: "URL", value: issue.url });
  lines.push(...labeledLines(meta, width));

  if (issue.description) {
    lines.push({ text: "" }, { text: "Description", style: "bold" });
    for (const line of wrapText(issue.description, width))
      lines.push({ text: line });
  }

  const comments = issue.comments?.nodes ?? [];
  if (comments.length) {
    lines.push(
      { text: "" },
      { text: `Comments (${comments.length})`, style: "bold" }
    );
    for (const c of comments) lines.push(...commentLines(c, width, now));
  }

  return lines;
}

/** The detail view for a comment on a document. */
export function documentLines(
  n: NotificationLike,
  doc: DocumentLike | null,
  comment: ThreadComment | null,
  width: number,
  now: Date = new Date()
): Line[] {
  const lines: Line[] = [
    { text: truncate(doc?.title ?? n.title ?? "", width), style: "title" },
    { text: "" },
  ];

  // The title line is already the document title and the notification URL is
  // the deep link to the comment, so no extra document row is needed.
  lines.push(...labeledLines(metadataRows(n, now), width));

  if (comment?.parent) {
    lines.push(
      { text: "" },
      {
        text: truncate(`In reply to ${byline(comment.parent, now)}`, width),
        style: "dim",
      }
    );
    for (const line of wrapText(comment.parent.body, Math.max(1, width - 2)))
      lines.push({ text: truncate(`│ ${line}`, width), style: "dim" });
  }

  if (comment) {
    lines.push(
      { text: "" },
      { text: truncate(`Comment — ${byline(comment, now)}`, width), style: "bold" }
    );
    for (const line of wrapText(comment.body, width)) lines.push({ text: line });

    const replies = comment.children?.nodes ?? [];
    if (replies.length) {
      lines.push(
        { text: "" },
        { text: `Replies (${replies.length})`, style: "bold" }
      );
      for (const r of replies) lines.push(...commentLines(r, width, now));
    }
  } else if (n.subtitle) {
    lines.push({ text: "" }, { text: "Summary", style: "bold" });
    for (const line of wrapText(n.subtitle, width)) lines.push({ text: line });
  }

  return lines;
}

/** The detail view for a notification with nothing else to fetch. */
export function notificationLines(
  n: NotificationLike,
  width: number,
  now: Date = new Date()
): Line[] {
  const lines: Line[] = [
    { text: truncate(n.title ?? "", width), style: "title" },
    { text: "" },
  ];
  lines.push(...labeledLines(metadataRows(n, now), width));

  const body = n.comment?.body ?? n.subtitle ?? "";
  if (body) {
    lines.push({ text: "" });
    lines.push({
      text: n.comment?.body ? "Comment" : "Summary",
      style: "bold",
    });
    for (const line of wrapText(body, width)) lines.push({ text: line });
  }

  return lines;
}

/** Footer hint like "12–34/120" when the content does not fit on screen. */
export function scrollStatus(
  total: number,
  offset: number,
  height: number
): string | null {
  if (total <= height) return null;
  const first = offset + 1;
  const last = Math.min(total, offset + height);
  return `${first}–${last}/${total}`;
}

export function clampOffset(
  total: number,
  offset: number,
  height: number
): number {
  return Math.max(0, Math.min(offset, total - height));
}
