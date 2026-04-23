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
const CLEAR_SCREEN = `${ESC}[2J`;
const MOVE_HOME = `${ESC}[H`;
const ALT_SCREEN_ON = `${ESC}[?1049h`;
const ALT_SCREEN_OFF = `${ESC}[?1049l`;
const UP = (n: number) => `${ESC}[${n}A`;
const MOVE_TO = (row: number, col: number) => `${ESC}[${row};${col}H`;

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
function renderList(
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
  out +=
    DIM +
    "↑↓/jk: navigate  Enter: details  b: browser  m: mark read  r: reload  q: quit" +
    RESET;

  // Move cursor back to the top of the area for the next render
  out += UP(areaHeight - 1) + "\r";

  process.stdout.write(out);
}

function renderDetails(n: Notification, statusMsg: string): void {
  const termCols = process.stdout.columns ?? 80;
  const termRows = process.stdout.rows ?? 24;

  let out = CLEAR_SCREEN + MOVE_HOME;

  out += `${BOLD}${CYAN}${truncate(n.title, termCols)}${RESET}\n`;
  if (n.subtitle) out += `${DIM}${truncate(n.subtitle, termCols)}${RESET}\n`;
  out += "\n";

  const actor = n.actor?.name ?? "—";
  const rows: Array<[string, string]> = [
    ["Type", n.type],
    ["Actor", actor],
    ["Time", `${formatTime(n.createdAt)} (${n.createdAt})`],
  ];
  if (n.issue) rows.push(["Issue", `${n.issue.identifier} — ${n.issue.title}`]);
  if (n.project) rows.push(["Project", n.project.name]);
  if (n.pullRequest)
    rows.push(["PR", `#${n.pullRequest.number} ${n.pullRequest.title}`]);
  rows.push(["URL", n.url]);

  const labelWidth = Math.max(...rows.map(([l]) => l.length));
  for (const [label, value] of rows) {
    const padded = (label + ":").padEnd(labelWidth + 2);
    out += `${DIM}${padded}${RESET}${truncate(value, termCols - labelWidth - 2)}\n`;
  }

  if (n.comment?.body) {
    out += "\n" + BOLD + "Comment" + RESET + "\n";
    const lines = wrapText(n.comment.body, termCols);
    // Leave 2 rows for the footer + blank line
    const maxBodyRows = Math.max(0, termRows - 4 - rows.length - (n.subtitle ? 1 : 0) - 4);
    for (const line of lines.slice(0, maxBodyRows)) {
      out += line + "\n";
    }
    if (lines.length > maxBodyRows) {
      out += DIM + `… (${lines.length - maxBodyRows} more line${lines.length - maxBodyRows !== 1 ? "s" : ""})` + RESET + "\n";
    }
  }

  // Footer pinned to last row
  out += MOVE_TO(termRows, 1) + CLEAR_LINE;
  const help = "b: browser  m: mark read  Enter/q: back";
  out += DIM + help + RESET;
  if (statusMsg) {
    out += `  ${CYAN}${statusMsg}${RESET}`;
  }

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
  let statusMsg = makeCountStatus(notifications.length);
  let mode: "list" | "details" = "list";
  let detailsStatus = "";

  const unreadCountStatus = () => makeCountStatus(notifications.length);

  function currentNotification(): Notification | null {
    return notifications[selected] ?? null;
  }

  function renderCurrent(): void {
    if (mode === "list") {
      renderList(notifications, selected, statusMsg);
    } else {
      const n = currentNotification();
      if (n) renderDetails(n, detailsStatus);
    }
  }

  function enterDetails(): void {
    mode = "details";
    detailsStatus = "";
    process.stdout.write(ALT_SCREEN_ON);
    renderCurrent();
  }

  function leaveDetails(): void {
    mode = "list";
    process.stdout.write(ALT_SCREEN_OFF);
    renderCurrent();
  }

  function exitApp(): void {
    if (mode === "details") {
      process.stdout.write(ALT_SCREEN_OFF);
    }
    clearArea();
    process.stdout.write(SHOW_CURSOR);
    stdin.setRawMode(false);
    process.exit(0);
  }

  function removeCurrentAndRefresh(): void {
    notifications.splice(selected, 1);
    if (notifications.length === 0) {
      // No more notifications — exit cleanly
      if (mode === "details") {
        process.stdout.write(ALT_SCREEN_OFF);
      }
      clearArea();
      process.stdout.write(SHOW_CURSOR);
      stdin.setRawMode(false);
      process.stdout.write("No unread notifications.\n");
      process.exit(0);
    }
    selected = Math.min(selected, notifications.length - 1);
    statusMsg = unreadCountStatus();
  }

  const visibleItems = computeVisibleItems(notifications.length);
  reserveArea(visibleItems + 2); // header + items + help
  renderCurrent();

  process.stdout.on("resize", () => {
    renderCurrent();
  });

  stdin.on("data", (key: string) => {
    // Ctrl-C always exits
    if (key === "\x03") {
      exitApp();
      return;
    }

    if (mode === "list") {
      handleListKey(key);
    } else {
      handleDetailsKey(key);
    }
  });

  function handleListKey(key: string): void {
    // q or Escape
    if (key === "q" || key === "\x1b") {
      exitApp();
      return;
    }

    // r - reload
    if (key === "r") {
      statusMsg = "Reloading…";
      renderCurrent();
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
        statusMsg = unreadCountStatus();
      } catch (err) {
        statusMsg = `Error reloading: ${err}`;
      }
      renderCurrent();
      return;
    }

    // Arrow up / k
    if (key === "\x1b[A" || key === "k") {
      selected = Math.max(0, selected - 1);
      renderCurrent();
      return;
    }

    // Arrow down / j
    if (key === "\x1b[B" || key === "j") {
      selected = Math.min(notifications.length - 1, selected + 1);
      renderCurrent();
      return;
    }

    // Enter - show details in full-screen
    if (key === "\r" || key === "\n") {
      if (currentNotification()) {
        enterDetails();
      }
      return;
    }

    // b - open in browser (stay in program)
    if (key === "b") {
      const n = currentNotification();
      if (n?.url) {
        openUrl(n.url);
        statusMsg = `Opened in browser — ${unreadCountStatus()}`;
        renderCurrent();
      }
      return;
    }

    // m - mark selected as read
    if (key === "m") {
      const n = currentNotification();
      if (!n) return;
      statusMsg = "Marking as read…";
      renderCurrent();
      try {
        markNotificationRead(n.id);
        removeCurrentAndRefresh();
      } catch (err) {
        statusMsg = `Error marking as read: ${err}`;
      }
      renderCurrent();
      return;
    }
  }

  function handleDetailsKey(key: string): void {
    // Enter / q / Esc - back to list
    if (key === "\r" || key === "\n" || key === "q" || key === "\x1b") {
      leaveDetails();
      return;
    }

    // b - open in browser (stay in details view)
    if (key === "b") {
      const n = currentNotification();
      if (n?.url) {
        openUrl(n.url);
        detailsStatus = "Opened in browser";
        renderCurrent();
      }
      return;
    }

    // m - mark as read, then return to list
    if (key === "m") {
      const n = currentNotification();
      if (!n) return;
      detailsStatus = "Marking as read…";
      renderCurrent();
      try {
        markNotificationRead(n.id);
        removeCurrentAndRefresh();
        leaveDetails();
      } catch (err) {
        detailsStatus = `Error marking as read: ${err}`;
        renderCurrent();
      }
      return;
    }
  }
}

function makeCountStatus(count: number): string {
  return `${count} unread notification${count !== 1 ? "s" : ""}`;
}

main().catch((err) => {
  process.stderr.write(`Fatal error: ${err}\n`);
  process.exit(1);
});
