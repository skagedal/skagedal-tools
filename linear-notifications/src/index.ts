#!/usr/bin/env node

import { execSync, spawnSync } from "child_process";
import * as readline from "readline";

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
}

function fetchUnreadNotifications(): Notification[] {
  const result = spawnSync("linear", ["api", GRAPHQL_QUERY], {
    encoding: "utf8",
  });

  if (result.error) {
    throw new Error(`Failed to run linear CLI: ${result.error.message}`);
  }
  if (result.status !== 0) {
    throw new Error(`linear CLI exited with code ${result.status}: ${result.stderr}`);
  }

  const data = JSON.parse(result.stdout);
  const nodes: Notification[] = data?.data?.notifications?.nodes ?? [];
  return nodes.filter((n) => n.readAt === null);
}

function openUrl(url: string): void {
  const platform = process.platform;
  const cmd = platform === "darwin" ? "open" : platform === "win32" ? "start" : "xdg-open";
  spawnSync(cmd, [url], { stdio: "inherit" });
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
  if (str.length <= maxLen) return str;
  return str.slice(0, maxLen - 1) + "…";
}

// ANSI escape helpers
const ESC = "\x1b";
const CLEAR_SCREEN = `${ESC}[2J${ESC}[H`;
const HIDE_CURSOR = `${ESC}[?25l`;
const SHOW_CURSOR = `${ESC}[?25h`;
const RESET = `${ESC}[0m`;
const BOLD = `${ESC}[1m`;
const DIM = `${ESC}[2m`;
const REVERSE = `${ESC}[7m`;
const CYAN = `${ESC}[36m`;
const YELLOW = `${ESC}[33m`;
const GREEN = `${ESC}[32m`;

function moveTo(row: number, col: number): string {
  return `${ESC}[${row};${col}H`;
}

function render(notifications: Notification[], selected: number, statusMsg: string): void {
  const { rows, columns } = process.stdout;
  const termRows = rows ?? 24;
  const termCols = columns ?? 80;

  let out = CLEAR_SCREEN;

  // Header
  out += moveTo(1, 1);
  out += BOLD + CYAN;
  out += truncate("Linear Unread Notifications", termCols);
  out += RESET;

  // Status line (row 2)
  out += moveTo(2, 1);
  out += DIM;
  out += truncate(statusMsg, termCols);
  out += RESET;

  // Separator
  out += moveTo(3, 1);
  out += DIM + "─".repeat(termCols) + RESET;

  // List area: rows 4 to termRows-2
  const listTop = 4;
  const listBottom = termRows - 2;
  const listHeight = listBottom - listTop + 1;

  // Scroll offset to keep selected item visible
  const scrollOffset = Math.max(0, selected - listHeight + 1);

  for (let i = 0; i < listHeight; i++) {
    const idx = i + scrollOffset;
    out += moveTo(listTop + i, 1);

    if (idx >= notifications.length) {
      out += " ".repeat(termCols);
      continue;
    }

    const n = notifications[idx];
    const isSelected = idx === selected;
    const timeStr = formatTime(n.createdAt);
    const timeWidth = 8;
    const titleWidth = termCols - timeWidth - 2;

    const title = truncate(n.title, titleWidth);
    const time = timeStr.padStart(timeWidth);

    const line = ` ${title.padEnd(titleWidth)} ${time}`;

    if (isSelected) {
      out += REVERSE + BOLD;
      out += truncate(line, termCols).padEnd(termCols);
      out += RESET;
    } else {
      out += line;
    }
  }

  // Bottom separator
  out += moveTo(termRows - 1, 1);
  out += DIM + "─".repeat(termCols) + RESET;

  // Help line
  out += moveTo(termRows, 1);
  out += DIM;
  out += truncate("Enter: open  r: reload  q/Esc: quit", termCols);
  out += RESET;

  process.stdout.write(out);
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

  // Enter raw mode
  const stdin = process.stdin;
  stdin.setRawMode(true);
  stdin.resume();
  stdin.setEncoding("utf8");

  process.stdout.write(HIDE_CURSOR);

  let selected = 0;
  let statusMsg = `${notifications.length} unread notification${notifications.length !== 1 ? "s" : ""}`;

  function redraw(): void {
    render(notifications, selected, statusMsg);
  }

  function exit(): void {
    process.stdout.write(SHOW_CURSOR + CLEAR_SCREEN);
    stdin.setRawMode(false);
    process.exit(0);
  }

  redraw();

  process.stdout.on("resize", () => {
    redraw();
  });

  stdin.on("data", (key: string) => {
    // Ctrl-C
    if (key === "\x03") {
      exit();
      return;
    }

    // q or Escape
    if (key === "q" || key === "\x1b") {
      exit();
      return;
    }

    // r - reload
    if (key === "r") {
      statusMsg = "Reloading…";
      redraw();
      try {
        notifications = fetchUnreadNotifications();
        if (notifications.length === 0) {
          process.stdout.write(SHOW_CURSOR + CLEAR_SCREEN);
          stdin.setRawMode(false);
          process.stdout.write("No unread notifications.\n");
          process.exit(0);
        }
        selected = Math.min(selected, notifications.length - 1);
        statusMsg = `${notifications.length} unread notification${notifications.length !== 1 ? "s" : ""}`;
      } catch (err) {
        statusMsg = `Error: ${err}`;
      }
      redraw();
      return;
    }

    // Arrow up / k
    if (key === "\x1b[A" || key === "k") {
      selected = Math.max(0, selected - 1);
      redraw();
      return;
    }

    // Arrow down / j
    if (key === "\x1b[B" || key === "j") {
      selected = Math.min(notifications.length - 1, selected + 1);
      redraw();
      return;
    }

    // Enter
    if (key === "\r" || key === "\n") {
      const n = notifications[selected];
      if (n?.url) {
        process.stdout.write(SHOW_CURSOR + CLEAR_SCREEN);
        stdin.setRawMode(false);
        openUrl(n.url);
        process.exit(0);
      }
      return;
    }
  });
}

main().catch((err) => {
  process.stderr.write(`Fatal error: ${err}\n`);
  process.exit(1);
});
