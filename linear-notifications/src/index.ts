#!/usr/bin/env node

import { spawnSync } from "child_process";

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
    throw new Error(
      `linear CLI exited with code ${result.status}: ${result.stderr}`
    );
  }

  const data = JSON.parse(result.stdout);
  const nodes: Notification[] = data?.data?.notifications?.nodes ?? [];
  return nodes.filter((n) => n.readAt === null);
}

function openUrl(url: string): void {
  const platform = process.platform;
  const cmd =
    platform === "darwin" ? "open" : platform === "win32" ? "start" : "xdg-open";
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
  if (maxLen <= 0) return "";
  if (str.length <= maxLen) return str;
  return str.slice(0, maxLen - 1) + "…";
}

// ANSI escape helpers
const ESC = "\x1b";
const HIDE_CURSOR = `${ESC}[?25l`;
const SHOW_CURSOR = `${ESC}[?25h`;
const RESET = `${ESC}[0m`;
const BOLD = `${ESC}[1m`;
const DIM = `${ESC}[2m`;
const REVERSE = `${ESC}[7m`;
const CYAN = `${ESC}[36m`;
const CLEAR_LINE = `${ESC}[2K`;
const UP = (n: number) => `${ESC}[${n}A`;

const MAX_VISIBLE_ITEMS = 10;

// The inline rendering area has a fixed height determined at startup:
//   1 header line + visibleItems lines + 1 help line
let areaHeight = 0;
let scrollOffset = 0;

function computeVisibleItems(count: number): number {
  return Math.min(count, MAX_VISIBLE_ITEMS);
}

/** Reserve `areaHeight` blank lines below current cursor, then move back up. */
function reserveArea(height: number): void {
  areaHeight = height;
  // Move cursor down by printing newlines (may scroll terminal), then back up
  process.stdout.write("\n".repeat(height) + UP(height));
}

/**
 * Render the entire inline area in-place.
 * After rendering, cursor is left at the top-left of the area so the next
 * render also starts there.
 */
function render(
  notifications: Notification[],
  selected: number,
  statusMsg: string
): void {
  const termCols = process.stdout.columns ?? 80;
  const visibleItems = computeVisibleItems(notifications.length);

  // Adjust scroll to keep selected in view
  if (selected < scrollOffset) {
    scrollOffset = selected;
  } else if (selected >= scrollOffset + visibleItems) {
    scrollOffset = selected - visibleItems + 1;
  }

  let out = "\r"; // go to column 1

  // ── Header ──────────────────────────────────────────────────────────────
  out += CLEAR_LINE;
  const headerText = `${BOLD}${CYAN}Linear Notifications${RESET} ${DIM}— ${statusMsg}${RESET}`;
  out += headerText + "\n";

  // ── Item lines ───────────────────────────────────────────────────────────
  for (let i = 0; i < visibleItems; i++) {
    out += CLEAR_LINE;
    const idx = i + scrollOffset;
    const n = notifications[idx];
    const isSelected = idx === selected;

    const timeStr = formatTime(n.createdAt);
    const timeWidth = 8;
    const prefix = isSelected ? "▶ " : "  ";
    const titleWidth = Math.max(0, termCols - timeWidth - prefix.length - 1);
    const title = truncate(n.title, titleWidth);
    const time = timeStr.padStart(timeWidth);
    const line = `${prefix}${title.padEnd(titleWidth)} ${time}`;

    if (isSelected) {
      out += REVERSE + BOLD + truncate(line, termCols).padEnd(termCols) + RESET;
    } else {
      out += line;
    }
    out += "\n";
  }

  // ── Help line ────────────────────────────────────────────────────────────
  out += CLEAR_LINE;
  out += DIM + "↑↓/jk: navigate  Enter: open  r: reload  q/Esc: quit" + RESET;

  // Move cursor back to the top of the area for the next render
  out += UP(areaHeight - 1) + "\r";

  process.stdout.write(out);
}

/** Clear all lines in the reserved area and leave cursor at the top. */
function clearArea(): void {
  let out = "\r";
  for (let i = 0; i < areaHeight; i++) {
    out += CLEAR_LINE;
    if (i < areaHeight - 1) out += "\n";
  }
  if (areaHeight > 1) {
    out += UP(areaHeight - 1);
  }
  out += "\r";
  process.stdout.write(out);
}

function exit(stdin: NodeJS.ReadStream): void {
  clearArea();
  process.stdout.write(SHOW_CURSOR);
  stdin.setRawMode(false);
  process.exit(0);
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

  const stdin = process.stdin;
  stdin.setRawMode(true);
  stdin.resume();
  stdin.setEncoding("utf8");

  process.stdout.write(HIDE_CURSOR);

  let selected = 0;
  let statusMsg = `${notifications.length} unread notification${notifications.length !== 1 ? "s" : ""}`;

  const visibleItems = computeVisibleItems(notifications.length);
  reserveArea(visibleItems + 2); // header + items + help
  render(notifications, selected, statusMsg);

  process.stdout.on("resize", () => {
    render(notifications, selected, statusMsg);
  });

  stdin.on("data", (key: string) => {
    // Ctrl-C
    if (key === "\x03") {
      exit(stdin);
      return;
    }

    // q or Escape
    if (key === "q" || key === "\x1b") {
      exit(stdin);
      return;
    }

    // r - reload
    if (key === "r") {
      statusMsg = "Reloading…";
      render(notifications, selected, statusMsg);
      try {
        notifications = fetchUnreadNotifications();
        if (notifications.length === 0) {
          clearArea();
          process.stdout.write(SHOW_CURSOR);
          stdin.setRawMode(false);
          process.stdout.write("No unread notifications.\n");
          process.exit(0);
        }
        selected = Math.min(selected, notifications.length - 1);
        statusMsg = `${notifications.length} unread notification${notifications.length !== 1 ? "s" : ""}`;
      } catch (err) {
        statusMsg = `Error reloading: ${err}`;
      }
      render(notifications, selected, statusMsg);
      return;
    }

    // Arrow up / k
    if (key === "\x1b[A" || key === "k") {
      selected = Math.max(0, selected - 1);
      render(notifications, selected, statusMsg);
      return;
    }

    // Arrow down / j
    if (key === "\x1b[B" || key === "j") {
      selected = Math.min(notifications.length - 1, selected + 1);
      render(notifications, selected, statusMsg);
      return;
    }

    // Enter
    if (key === "\r" || key === "\n") {
      const n = notifications[selected];
      if (n?.url) {
        clearArea();
        process.stdout.write(SHOW_CURSOR);
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
