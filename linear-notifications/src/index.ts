#!/usr/bin/env node

import { spawnSync } from "child_process";
import termkit from "terminal-kit";
import {
  clampOffset,
  countStatus,
  documentLines,
  formatTime,
  issueLines,
  notificationLines,
  previewLines,
  scrollStatus,
  truncate,
  type DocumentLike,
  type IssueLike,
  type Line,
  type ThreadComment,
} from "./lib";

const term = termkit.terminal;

const NOTIFICATIONS_QUERY = `query {
  notifications(first: 50) {
    nodes {
      id
      type
      title
      subtitle
      createdAt
      readAt
      url
      actor { name }
      ... on IssueNotification {
        issue { identifier title }
        comment { body }
      }
      ... on ProjectNotification {
        project { name }
        comment { body }
      }
      ... on PullRequestNotification {
        pullRequest { title number url }
      }
      ... on DocumentNotification {
        documentId
        commentId
      }
    }
  }
}`;

interface Notification {
  id: string;
  type: string;
  title: string;
  subtitle: string | null;
  createdAt: string;
  readAt: string | null;
  url: string;
  actor: { name: string } | null;
  issue?: { identifier: string; title: string } | null;
  project?: { name: string } | null;
  pullRequest?: { title: string; number: number; url: string } | null;
  comment?: { body: string } | null;
  documentId?: string | null;
  commentId?: string | null;
}

type Issue = IssueLike;

interface DocumentDetail {
  document: DocumentLike | null;
  comment: ThreadComment | null;
}

function runLinearApi(query: string): any {
  const result = spawnSync("linear", ["api", query], { encoding: "utf8" });

  if (result.error) {
    throw new Error(`Failed to run linear CLI: ${result.error.message}`);
  }
  if (result.status !== 0) {
    throw new Error(
      `linear CLI exited with code ${result.status}: ${result.stderr}`
    );
  }

  const data = JSON.parse(result.stdout);
  if (data.errors) {
    throw new Error(`GraphQL error: ${JSON.stringify(data.errors)}`);
  }
  return data;
}

function fetchUnreadNotifications(): Notification[] {
  const data = runLinearApi(NOTIFICATIONS_QUERY);
  const nodes: Notification[] = data?.data?.notifications?.nodes ?? [];
  return nodes.filter((n) => n.readAt === null);
}

function fetchIssue(identifier: string): Issue {
  const query = `query { issue(id: "${identifier}") {
    identifier
    title
    description
    url
    createdAt
    state { name }
    priorityLabel
    assignee { name }
    creator { name }
    labels { nodes { name } }
    comments(first: 50) {
      nodes {
        body
        createdAt
        user { name }
      }
    }
  } }`;
  const data = runLinearApi(query);
  const issue = data?.data?.issue;
  if (!issue) throw new Error(`Issue ${identifier} not found`);
  return issue;
}

// Document notifications only carry ids, and the subtitle is an abridged copy
// of the comment — so fetch the document and the whole comment thread.
function fetchDocumentDetail(
  documentId: string,
  commentId: string | null | undefined
): DocumentDetail {
  const commentPart = commentId
    ? `comment(id: "${commentId}") {
        body
        createdAt
        user { name }
        parent { body createdAt user { name } }
        children(first: 50) { nodes { body createdAt user { name } } }
      }`
    : "";
  const query = `query {
    document(id: "${documentId}") {
      title
      url
      createdAt
      creator { name }
    }
    ${commentPart}
  }`;
  const data = runLinearApi(query);
  return {
    document: data?.data?.document ?? null,
    comment: data?.data?.comment ?? null,
  };
}

function markNotificationRead(id: string): void {
  const now = new Date().toISOString();
  const mutation = `mutation { notificationUpdate(id: "${id}", input: { readAt: "${now}" }) { success } }`;
  const data = runLinearApi(mutation);
  if (!data?.data?.notificationUpdate?.success) {
    throw new Error("notificationUpdate returned success=false");
  }
}

function openUrl(url: string): void {
  const platform = process.platform;
  const cmd =
    platform === "darwin" ? "open" : platform === "win32" ? "start" : "xdg-open";
  spawnSync(cmd, [url], { stdio: "ignore" });
}

function safeW(): number {
  return Math.max(0, term.width - 1);
}

type Mode = "list" | "detail";

type Detail =
  | { kind: "loading" }
  | { kind: "error"; message: string }
  | { kind: "issue"; issue: Issue }
  | { kind: "document"; detail: DocumentDetail }
  | { kind: "notification" };

function drawLine(row: number, line: Line): void {
  term.moveTo(1, row);
  if (line.label) term.dim.noFormat(line.label);
  switch (line.style) {
    case "title":
      term.bold.cyan.noFormat(line.text);
      break;
    case "bold":
      term.bold.noFormat(line.text);
      break;
    case "dim":
      term.dim.noFormat(line.text);
      break;
    case "red":
      term.red.noFormat(line.text);
      break;
    default:
      term.noFormat(line.text);
  }
}

/** Draws lines[offset…] into rows top…bottom. Returns lines left undrawn. */
function drawLines(
  lines: Line[],
  top: number,
  bottom: number,
  offset: number
): number {
  const height = bottom - top + 1;
  if (height <= 0) return lines.length;
  for (let i = 0; i < height; i++) {
    const line = lines[offset + i];
    if (!line) break;
    drawLine(top + i, line);
  }
  return Math.max(0, lines.length - offset - height);
}

class App {
  notifications: Notification[];
  selected = 0;
  scrollOffset = 0;
  mode: Mode = "list";
  listStatus: string;
  detailStatus = "";
  detail: Detail = { kind: "loading" };
  detailOffset = 0;
  detailTotal = 0;
  detailHeight = 0;
  issueCache = new Map<string, Issue>();
  documentCache = new Map<string, DocumentDetail>();

  constructor(notifications: Notification[]) {
    this.notifications = notifications;
    this.listStatus = countStatus(notifications.length);
  }

  current(): Notification | undefined {
    return this.notifications[this.selected];
  }

  render(): void {
    if (this.mode === "list") this.renderList();
    else this.renderDetail();
  }

  renderList(): void {
    term.clear();

    term.moveTo(1, 1);
    term.bold.cyan("Linear Notifications");
    term(" ");
    term.dim.noFormat(`— ${this.listStatus}`);

    const footerRow = term.height;
    const listTop = 3;
    const maxListHeight = Math.max(
      3,
      Math.min(10, Math.floor((term.height - listTop - 2) * 0.4))
    );
    const listHeight = Math.min(this.notifications.length, maxListHeight);
    const listBottom = listTop + listHeight - 1;
    const separatorRow = listBottom + 1;
    const previewTop = separatorRow + 1;
    const previewBottom = footerRow - 1;

    if (this.selected < this.scrollOffset) {
      this.scrollOffset = this.selected;
    } else if (this.selected >= this.scrollOffset + listHeight) {
      this.scrollOffset = this.selected - listHeight + 1;
    }

    const timeW = 8;
    const rowW = safeW();
    for (let i = 0; i < listHeight; i++) {
      const idx = i + this.scrollOffset;
      if (idx >= this.notifications.length) break;
      const n = this.notifications[idx];
      const isSelected = idx === this.selected;
      const prefix = isSelected ? "▶ " : "  ";
      const titleW = Math.max(0, rowW - timeW - prefix.length - 1);
      const title = truncate(n.title, titleW).padEnd(titleW);
      const time = formatTime(n.createdAt).padStart(timeW);
      const line = `${prefix}${title} ${time}`;

      term.moveTo(1, listTop + i);
      if (isSelected) {
        term.inverse.bold.noFormat(truncate(line, rowW).padEnd(rowW));
      } else {
        term.noFormat(truncate(line, rowW));
      }
    }

    if (separatorRow <= footerRow - 1) {
      term.moveTo(1, separatorRow);
      term.dim("─".repeat(rowW));
    }

    if (previewTop <= previewBottom) {
      this.renderPreview(previewTop, previewBottom);
    }

    term.moveTo(1, footerRow);
    term.dim(
      truncate(
        "↑↓/jk: navigate  Enter/o: open  b: browser  m: mark read  r: reload  q: quit",
        rowW
      )
    );
  }

  renderPreview(top: number, bottom: number): void {
    const n = this.current();
    if (!n) return;
    const w = safeW();

    const lines = previewLines(n, w);
    const leftover = drawLines(lines, top, bottom, 0);
    if (leftover > 0) {
      // Padded so it covers whatever the last preview line had already drawn.
      drawLine(bottom, {
        text: truncate(
          `… (${leftover} more line${leftover !== 1 ? "s" : ""} — press o)`,
          w
        ).padEnd(w),
        style: "dim",
      });
    }
  }

  detailLines(w: number): Line[] {
    const n = this.current();
    if (!n) return [];
    switch (this.detail.kind) {
      case "loading":
        return [{ text: "Loading…", style: "dim" }];
      case "error":
        return [{ text: truncate(this.detail.message, w), style: "red" }];
      case "issue":
        return issueLines(this.detail.issue, w);
      case "document":
        return documentLines(
          n,
          this.detail.detail.document,
          this.detail.detail.comment,
          w
        );
      case "notification":
        return notificationLines(n, w);
    }
  }

  renderDetail(): void {
    term.clear();
    if (!this.current()) return;
    const w = safeW();
    const footerRow = term.height;
    const bodyBottom = footerRow - 1;
    const height = Math.max(1, bodyBottom);

    const lines = this.detailLines(w);
    this.detailTotal = lines.length;
    this.detailHeight = height;
    this.detailOffset = clampOffset(lines.length, this.detailOffset, height);
    drawLines(lines, 1, bodyBottom, this.detailOffset);

    const scroll = scrollStatus(lines.length, this.detailOffset, height);
    const parts = ["b: browser  m: mark read  u/Enter/q: back"];
    if (scroll) parts.push(`↑↓/jk: scroll  ${scroll}`);
    if (this.detailStatus) parts.push(this.detailStatus);
    term.moveTo(1, footerRow);
    term.dim.noFormat(truncate(parts.join("  "), w));
  }

  scrollDetail(delta: number): void {
    this.detailOffset = clampOffset(
      this.detailTotal,
      this.detailOffset + delta,
      this.detailHeight
    );
    this.render();
  }

  openCurrent(): void {
    const n = this.current();
    if (!n) return;

    this.mode = "detail";
    this.detailStatus = "";
    this.detailOffset = 0;

    if (n.issue) {
      this.loadIssue(n.issue.identifier);
    } else if (n.documentId) {
      this.loadDocument(n.documentId, n.commentId);
    } else {
      this.detail = { kind: "notification" };
      this.render();
    }
  }

  loadIssue(identifier: string): void {
    const cached = this.issueCache.get(identifier);
    if (cached) {
      this.detail = { kind: "issue", issue: cached };
      this.render();
      return;
    }

    this.detail = { kind: "loading" };
    this.render();

    try {
      const issue = fetchIssue(identifier);
      this.issueCache.set(identifier, issue);
      this.detail = { kind: "issue", issue };
    } catch (err) {
      this.detail = { kind: "error", message: `Error loading issue: ${err}` };
    }
    this.render();
  }

  loadDocument(documentId: string, commentId: string | null | undefined): void {
    const key = `${documentId}:${commentId ?? ""}`;
    const cached = this.documentCache.get(key);
    if (cached) {
      this.detail = { kind: "document", detail: cached };
      this.render();
      return;
    }

    this.detail = { kind: "loading" };
    this.render();

    try {
      const detail = fetchDocumentDetail(documentId, commentId);
      this.documentCache.set(key, detail);
      this.detail = { kind: "document", detail };
    } catch (err) {
      this.detail = { kind: "error", message: `Error loading document: ${err}` };
    }
    this.render();
  }

  backToList(): void {
    this.mode = "list";
    this.detail = { kind: "loading" };
    this.detailOffset = 0;
    this.render();
  }

  detailUrl(): string | undefined {
    const n = this.current();
    if (this.detail.kind === "issue") return this.detail.issue.url;
    return n?.url;
  }

  removeCurrent(): boolean {
    const removed = this.notifications.splice(this.selected, 1)[0];
    if (removed?.issue) this.issueCache.delete(removed.issue.identifier);
    if (this.notifications.length === 0) return false;
    this.selected = Math.min(this.selected, this.notifications.length - 1);
    this.listStatus = countStatus(this.notifications.length);
    return true;
  }

  reload(): void {
    this.listStatus = "Reloading…";
    this.render();
    try {
      this.notifications = fetchUnreadNotifications();
      this.issueCache.clear();
      this.documentCache.clear();
      this.selected = Math.min(
        this.selected,
        Math.max(0, this.notifications.length - 1)
      );
      this.listStatus = countStatus(this.notifications.length);
    } catch (err) {
      this.listStatus = `Error reloading: ${err}`;
    }
    this.render();
  }
}

function quit(code: number = 0, finalMessage?: string): void {
  term.hideCursor(false);
  term.grabInput(false);
  term.fullscreen(false);
  if (finalMessage) {
    process.stdout.write(finalMessage);
  }
  process.exit(code);
}

async function main(): Promise<void> {
  let notifications: Notification[];

  try {
    notifications = fetchUnreadNotifications();
  } catch (err) {
    process.stderr.write(`Error fetching notifications: ${err}\n`);
    process.exit(1);
  }

  if (notifications.length === 0) {
    process.stdout.write("No unread notifications.\n");
    process.exit(0);
  }

  const app = new App(notifications);

  term.fullscreen(true);
  term.grabInput(true);
  term.hideCursor(true);

  app.render();

  term.on("resize", () => {
    app.render();
  });

  term.on("key", (key: string) => {
    if (key === "CTRL_C") {
      quit(0);
      return;
    }

    if (app.mode === "list") {
      handleListKey(app, key);
    } else {
      handleDetailKey(app, key);
    }
  });
}

function handleListKey(app: App, key: string): void {
  switch (key) {
    case "q":
    case "ESCAPE":
      quit(0);
      return;

    case "UP":
    case "k":
      app.selected = Math.max(0, app.selected - 1);
      app.render();
      return;

    case "DOWN":
    case "j":
      app.selected = Math.min(app.notifications.length - 1, app.selected + 1);
      app.render();
      return;

    case "ENTER":
    case "o":
      if (app.current()) app.openCurrent();
      return;

    case "b": {
      const n = app.current();
      if (n?.url) {
        openUrl(n.url);
        app.listStatus = `Opened in browser — ${countStatus(app.notifications.length)}`;
        app.render();
      }
      return;
    }

    case "m": {
      const n = app.current();
      if (!n) return;
      app.listStatus = "Marking as read…";
      app.render();
      try {
        markNotificationRead(n.id);
        if (!app.removeCurrent()) {
          quit(0, "No unread notifications.\n");
          return;
        }
      } catch (err) {
        app.listStatus = `Error marking as read: ${err}`;
      }
      app.render();
      return;
    }

    case "r":
      app.reload();
      if (app.notifications.length === 0) {
        quit(0, "No unread notifications.\n");
      }
      return;
  }
}

function handleDetailKey(app: App, key: string): void {
  switch (key) {
    case "u":
    case "q":
    case "ESCAPE":
    case "ENTER":
      app.backToList();
      return;

    case "UP":
    case "k":
      app.scrollDetail(-1);
      return;

    case "DOWN":
    case "j":
      app.scrollDetail(1);
      return;

    case "PAGE_UP":
      app.scrollDetail(-Math.max(1, app.detailHeight - 1));
      return;

    case "PAGE_DOWN":
    case " ":
      app.scrollDetail(Math.max(1, app.detailHeight - 1));
      return;

    case "g":
    case "HOME":
      app.scrollDetail(-app.detailTotal);
      return;

    case "G":
    case "END":
      app.scrollDetail(app.detailTotal);
      return;

    case "b": {
      const url = app.detailUrl();
      if (url) {
        openUrl(url);
        app.detailStatus = "Opened in browser";
        app.render();
      }
      return;
    }

    case "m": {
      const n = app.current();
      if (!n) return;
      app.detailStatus = "Marking as read…";
      app.render();
      try {
        markNotificationRead(n.id);
        if (!app.removeCurrent()) {
          quit(0, "No unread notifications.\n");
          return;
        }
        app.backToList();
      } catch (err) {
        app.detailStatus = `Error marking as read: ${err}`;
        app.render();
      }
      return;
    }
  }
}

main().catch((err) => {
  term.hideCursor(false);
  term.grabInput(false);
  term.fullscreen(false);
  process.stderr.write(`Fatal error: ${err}\n`);
  process.exit(1);
});
