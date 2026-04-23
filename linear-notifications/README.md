# linear-notifications

Interactive CLI for viewing and opening unread [Linear](https://linear.app) notifications in the terminal.

## Requirements

- [Node.js](https://nodejs.org) (v18+)
- The [`linear`](https://github.com/schpet/linear-cli) CLI, authenticated and available on your `PATH`

## Installation

```sh
pnpm link --global
```

This builds the TypeScript source and installs the `linear-notifications` command globally.

> **Note:** `pnpm install -g .` is broken in pnpm v10 (resolves `.` relative to the global store instead of the current directory). Use `pnpm link --global` instead.

## Usage

```sh
linear-notifications
```

If there are no unread notifications, the command exits immediately. Otherwise a full-screen interactive UI appears, split between a list of notifications on top and a preview of the currently-selected notification below:

```
Linear Notifications — 3 unread notifications
▶ ICC-820 Expose installation report submissions as flat tables    5m ago
  ICC-719 Update API rate limiting strategy                       2h ago
  ICC-701 Fix authentication bug in mobile app                    1d ago
────────────────────────────────────────────────────────────────────────
ICC-820 Expose installation report submissions as flat tables
Alice commented

Type:    IssueNotification
Actor:   Alice
Time:    5m ago (2025-11-01T12:34:56Z)
Issue:   ICC-820 — Expose installation report submissions as flat tables
URL:     https://linear.app/…

Comment:
> Could we also expose the per-row timestamps while we're here?
────────────────────────────────────────────────────────────────────────
↑↓/jk: navigate  Enter/o: open  b: browser  m: mark read  r: reload  q: quit
```

Pressing `Enter` or `o` opens the full issue view for the selected notification, showing the issue's state, priority, assignee, description, and comments. From either view, `b` opens the notification URL in your browser without quitting, and `m` marks it as read and removes it from the list.

### Key bindings

#### List view

| Key | Action |
|-----|--------|
| `↑` / `k` | Move selection up |
| `↓` / `j` | Move selection down |
| `Enter` / `o` | Open the full issue view for the selected notification |
| `b` | Open selected notification URL in the browser (stay in app) |
| `m` | Mark selected notification as read |
| `r` | Reload the notification list |
| `q` / `Esc` | Quit |

#### Issue view

| Key | Action |
|-----|--------|
| `b` | Open the issue URL in the browser |
| `m` | Mark the notification as read and return to the list |
| `u` / `Enter` / `q` / `Esc` | Return to the list |

## Development

```sh
pnpm build   # compile TypeScript to dist/
pnpm start   # run the compiled output
```
