#!/usr/bin/env node

import { spawnSync } from "child_process";
import termkit from "terminal-kit";
import {
  countStatus,
  formatTime,
  metadataRows,
  truncate,
  wrapText,
  type Row,
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
}

interface Issue {
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
  comments: {
    nodes: Array<{
      body: string;
      createdAt: string;
      user: { name: string } | null;
    }>;
  };
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

type Mode = "list" | "issue";

function drawLabeledRows(startRow: number, maxRow: number, rows: Row[]): number {
  const labelW = Math.max(...rows.map((r) => r.label.length)) + 2;
  let row = startRow;
  for (const { label, value } of rows) {
    if (row > maxRow) break;
    term.moveTo(1, row);
    term.dim((label + ":").padEnd(labelW));
    term(truncate(value, Math.max(0, safeW() - labelW)));
    row++;
  }
  return row;
}

class App {
  notifications: Notification[];
  selected = 0;
  scrollOffset = 0;
  mode: Mode = "list";
  listStatus: string;
  issueStatus = "";
  issue: Issue | null = null;
  issueError: string | null = null;
  issueLoading = false;
  issueCache = new Map<string, Issue>();

  constructor(notifications: Notification[]) {
    this.notifications = notifications;
    this.listStatus = countStatus(notifications.length);
  }

  current(): Notification | undefined {
    return this.notifications[this.selected];
  }

  render(): void {
    if (this.mode === "list") this.renderList();
    else this.renderIssue();
  }

  renderList(): void {
    term.clear();

    term.moveTo(1, 1);
    term.bold.cyan("Linear Notifications");
    term(" ");
    term.dim(`— ${this.listStatus}`);

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
        term.inverse.bold(truncate(line, rowW).padEnd(rowW));
      } else {
        term(truncate(line, rowW));
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

    let row = top;

    term.moveTo(1, row);
    term.bold(truncate(n.title, w));
    row++;

    if (n.subtitle && row <= bottom) {
      term.moveTo(1, row);
      term.dim(truncate(n.subtitle, w));
      row++;
    }

    if (row <= bottom) row++; // blank line

    row = drawLabeledRows(row, bottom, metadataRows(n));

    if (n.comment?.body && row + 1 <= bottom) {
      row++; // blank
      term.moveTo(1, row);
      term.bold("Comment");
      row++;

      const bodyLines = wrapText(n.comment.body, w);
      const available = bottom - row + 1;
      const shown = bodyLines.slice(0, available);
      for (const line of shown) {
        if (row > bottom) break;
        term.moveTo(1, row);
        term(line);
        row++;
      }
      if (bodyLines.length > shown.length && row - 1 <= bottom) {
        term.moveTo(1, Math.min(row, bottom));
        const leftover = bodyLines.length - shown.length;
        term.dim(
          truncate(
            `… (${leftover} more line${leftover !== 1 ? "s" : ""})`,
            w
          )
        );
      }
    }
  }

  renderIssue(): void {
    term.clear();
    const n = this.current();
    if (!n) return;
    const w = safeW();
    const footerRow = term.height;
    const bodyBottom = footerRow - 1;

    const footerHelp = "b: browser  m: mark read  u/Enter/q: back";

    const drawFooter = () => {
      term.moveTo(1, footerRow);
      const text = this.issueStatus
        ? `${footerHelp}  ${this.issueStatus}`
        : footerHelp;
      term.dim(truncate(text, w));
    };

    if (this.issueLoading) {
      term.moveTo(1, 1);
      term.dim("Loading issue…");
      drawFooter();
      return;
    }

    if (this.issueError) {
      term.moveTo(1, 1);
      term.red(truncate(this.issueError, w));
      drawFooter();
      return;
    }

    if (this.issue) {
      this.renderIssueBody(this.issue, bodyBottom, w);
    } else {
      // No associated issue — fall back to notification details
      this.renderNotificationFallback(n, bodyBottom, w);
    }

    drawFooter();
  }

  renderIssueBody(issue: Issue, bottom: number, w: number): void {
    let row = 1;

    term.moveTo(1, row);
    term.bold.cyan(truncate(`${issue.identifier}  ${issue.title}`, w));
    row += 2;

    const meta: Row[] = [];
    if (issue.state) meta.push({ label: "State", value: issue.state.name });
    if (issue.priorityLabel)
      meta.push({ label: "Priority", value: issue.priorityLabel });
    if (issue.assignee)
      meta.push({ label: "Assignee", value: issue.assignee.name });
    if (issue.creator)
      meta.push({ label: "Creator", value: issue.creator.name });
    meta.push({
      label: "Created",
      value: `${formatTime(issue.createdAt)} (${issue.createdAt})`,
    });
    const labelNames = issue.labels?.nodes?.map((l) => l.name) ?? [];
    if (labelNames.length)
      meta.push({ label: "Labels", value: labelNames.join(", ") });
    meta.push({ label: "URL", value: issue.url });

    row = drawLabeledRows(row, bottom, meta);

    if (issue.description && row + 1 <= bottom) {
      row++; // blank
      term.moveTo(1, row);
      term.bold("Description");
      row++;
      const lines = wrapText(issue.description, w);
      for (const line of lines) {
        if (row > bottom) return;
        term.moveTo(1, row);
        term(line);
        row++;
      }
    }

    const comments = issue.comments?.nodes ?? [];
    if (comments.length && row + 1 <= bottom) {
      row++;
      term.moveTo(1, row);
      term.bold(`Comments (${comments.length})`);
      row++;
      for (const c of comments) {
        if (row > bottom) return;
        term.moveTo(1, row);
        term.dim(
          truncate(
            `— ${c.user?.name ?? "?"} · ${formatTime(c.createdAt)}`,
            w
          )
        );
        row++;
        const lines = wrapText(c.body, w);
        for (const line of lines) {
          if (row > bottom) return;
          term.moveTo(1, row);
          term(line);
          row++;
        }
        if (row > bottom) return;
        row++; // blank between comments
      }
    }
  }

  renderNotificationFallback(n: Notification, bottom: number, w: number): void {
    let row = 1;
    term.moveTo(1, row);
    term.bold.cyan(truncate(n.title, w));
    row++;
    if (n.subtitle) {
      term.moveTo(1, row);
      term.dim(truncate(n.subtitle, w));
      row++;
    }
    row++;
    row = drawLabeledRows(row, bottom, metadataRows(n));

    if (n.comment?.body && row + 1 <= bottom) {
      row++;
      term.moveTo(1, row);
      term.bold("Comment");
      row++;
      const lines = wrapText(n.comment.body, w);
      for (const line of lines) {
        if (row > bottom) return;
        term.moveTo(1, row);
        term(line);
        row++;
      }
    }
  }

  openCurrent(): void {
    const n = this.current();
    if (!n) return;

    this.mode = "issue";
    this.issueStatus = "";
    this.issue = null;
    this.issueError = null;

    if (!n.issue) {
      // No associated issue — show fallback immediately
      this.issueLoading = false;
      this.render();
      return;
    }

    const cached = this.issueCache.get(n.issue.identifier);
    if (cached) {
      this.issue = cached;
      this.issueLoading = false;
      this.render();
      return;
    }

    this.issueLoading = true;
    this.render();

    try {
      const issue = fetchIssue(n.issue.identifier);
      this.issueCache.set(n.issue.identifier, issue);
      this.issue = issue;
      this.issueError = null;
    } catch (err) {
      this.issueError = `Error loading issue: ${err}`;
    } finally {
      this.issueLoading = false;
    }
    this.render();
  }

  backToList(): void {
    this.mode = "list";
    this.issue = null;
    this.issueError = null;
    this.issueLoading = false;
    this.render();
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
      handleIssueKey(app, key);
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

function handleIssueKey(app: App, key: string): void {
  switch (key) {
    case "u":
    case "q":
    case "ESCAPE":
    case "ENTER":
      app.backToList();
      return;

    case "b": {
      const n = app.current();
      const url = app.issue?.url ?? n?.url;
      if (url) {
        openUrl(url);
        app.issueStatus = "Opened in browser";
        app.render();
      }
      return;
    }

    case "m": {
      const n = app.current();
      if (!n) return;
      app.issueStatus = "Marking as read…";
      app.render();
      try {
        markNotificationRead(n.id);
        if (!app.removeCurrent()) {
          quit(0, "No unread notifications.\n");
          return;
        }
        app.backToList();
      } catch (err) {
        app.issueStatus = `Error marking as read: ${err}`;
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
