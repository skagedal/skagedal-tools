#!/usr/bin/env node

import { spawnSync } from "child_process";
import termkit from "terminal-kit";

const term = termkit.terminal;

const GRAPHQL_QUERY = `query {
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
  const data = runLinearApi(GRAPHQL_QUERY);
  const nodes: Notification[] = data?.data?.notifications?.nodes ?? [];
  return nodes.filter((n) => n.readAt === null);
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

function formatTime(iso: string): string {
  const date = new Date(iso);
  const now = new Date();
  const diffMs = now.getTime() - date.getTime();
  const diffMins = Math.floor(diffMs / 60000);
  if (diffMins < 60) return `${diffMins}m ago`;
  const diffHours = Math.floor(diffMins / 60);
  if (diffHours < 24) return `${diffHours}h ago`;
  const diffDays = Math.floor(diffHours / 24);
  return `${diffDays}d ago`;
}

function truncate(str: string, maxLen: number): string {
  if (maxLen <= 0) return "";
  if (str.length <= maxLen) return str;
  return str.slice(0, maxLen - 1) + "…";
}

function wrapText(text: string, cols: number): string[] {
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

function countStatus(count: number): string {
  return `${count} unread notification${count !== 1 ? "s" : ""}`;
}

type Mode = "list" | "details";

class App {
  notifications: Notification[];
  selected = 0;
  scrollOffset = 0;
  mode: Mode = "list";
  listStatus: string;
  detailsStatus = "";

  constructor(notifications: Notification[]) {
    this.notifications = notifications;
    this.listStatus = countStatus(notifications.length);
  }

  current(): Notification | undefined {
    return this.notifications[this.selected];
  }

  render(): void {
    if (this.mode === "list") this.renderList();
    else this.renderDetails();
  }

  renderList(): void {
    term.clear();
    term.moveTo(1, 1);
    term.bold.cyan("Linear Notifications");
    term(" ");
    term.dim(`— ${this.listStatus}`);

    const listTop = 3;
    const listBottom = term.height - 1;
    const maxItems = Math.max(1, listBottom - listTop + 1);

    if (this.selected < this.scrollOffset) {
      this.scrollOffset = this.selected;
    } else if (this.selected >= this.scrollOffset + maxItems) {
      this.scrollOffset = this.selected - maxItems + 1;
    }

    const timeW = 8;
    const visible = Math.min(maxItems, this.notifications.length - this.scrollOffset);
    for (let i = 0; i < visible; i++) {
      const idx = i + this.scrollOffset;
      const n = this.notifications[idx];
      const isSelected = idx === this.selected;
      const prefix = isSelected ? "▶ " : "  ";
      const titleW = Math.max(0, term.width - timeW - prefix.length - 1);
      const title = truncate(n.title, titleW).padEnd(titleW);
      const time = formatTime(n.createdAt).padStart(timeW);
      const line = `${prefix}${title} ${time}`;

      term.moveTo(1, listTop + i);
      const rowWidth = Math.max(0, term.width - 1);
      if (isSelected) {
        term.inverse.bold(truncate(line, rowWidth).padEnd(rowWidth));
      } else {
        term(truncate(line, rowWidth));
      }
    }

    term.moveTo(1, term.height);
    term.dim(
      truncate(
        "↑↓/jk: navigate  Enter: details  b: browser  m: mark read  r: reload  q: quit",
        Math.max(0, term.width - 1)
      )
    );
  }

  renderDetails(): void {
    const n = this.current();
    if (!n) return;

    term.clear();
    term.moveTo(1, 1);
    term.bold.cyan(truncate(n.title, term.width));

    let row = 2;
    if (n.subtitle) {
      term.moveTo(1, row);
      term.dim(truncate(n.subtitle, term.width));
      row++;
    }
    row++; // blank line

    const rows: Array<[string, string]> = [
      ["Type", n.type],
      ["Actor", n.actor?.name ?? "—"],
      ["Time", `${formatTime(n.createdAt)} (${n.createdAt})`],
    ];
    if (n.issue)
      rows.push(["Issue", `${n.issue.identifier} — ${n.issue.title}`]);
    if (n.project) rows.push(["Project", n.project.name]);
    if (n.pullRequest)
      rows.push(["PR", `#${n.pullRequest.number} ${n.pullRequest.title}`]);
    rows.push(["URL", n.url]);

    const labelW = Math.max(...rows.map(([l]) => l.length)) + 2;
    for (const [label, value] of rows) {
      term.moveTo(1, row);
      term.dim((label + ":").padEnd(labelW));
      term(truncate(value, Math.max(0, term.width - labelW)));
      row++;
    }

    if (n.comment?.body) {
      row++; // blank line
      term.moveTo(1, row);
      term.bold("Comment");
      row++;

      const footerRow = term.height;
      const maxBodyRows = Math.max(0, footerRow - row - 1);
      const bodyLines = wrapText(n.comment.body, term.width);
      const shown = bodyLines.slice(0, maxBodyRows);
      for (const line of shown) {
        term.moveTo(1, row);
        term(line);
        row++;
      }
      if (bodyLines.length > shown.length) {
        term.moveTo(1, row);
        term.dim(
          `… (${bodyLines.length - shown.length} more line${
            bodyLines.length - shown.length !== 1 ? "s" : ""
          })`
        );
      }
    }

    term.moveTo(1, term.height);
    const footerW = Math.max(0, term.width - 1);
    const help = "b: browser  m: mark read  Enter/q: back";
    const footer = this.detailsStatus
      ? `${help}  ${this.detailsStatus}`
      : help;
    term.dim(truncate(footer, footerW));
  }

  enterDetails(): void {
    this.mode = "details";
    this.detailsStatus = "";
    this.render();
  }

  leaveDetails(): void {
    this.mode = "list";
    this.render();
  }

  removeCurrent(): boolean {
    this.notifications.splice(this.selected, 1);
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
      this.selected = Math.min(this.selected, Math.max(0, this.notifications.length - 1));
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
      handleDetailsKey(app, key);
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
      if (app.current()) app.enterDetails();
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

function handleDetailsKey(app: App, key: string): void {
  switch (key) {
    case "q":
    case "ESCAPE":
    case "ENTER":
      app.leaveDetails();
      return;

    case "b": {
      const n = app.current();
      if (n?.url) {
        openUrl(n.url);
        app.detailsStatus = "Opened in browser";
        app.render();
      }
      return;
    }

    case "m": {
      const n = app.current();
      if (!n) return;
      app.detailsStatus = "Marking as read…";
      app.render();
      try {
        markNotificationRead(n.id);
        if (!app.removeCurrent()) {
          quit(0, "No unread notifications.\n");
          return;
        }
        app.leaveDetails();
      } catch (err) {
        app.detailsStatus = `Error marking as read: ${err}`;
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
