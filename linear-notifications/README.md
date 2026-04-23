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

If there are no unread notifications, the command exits immediately. Otherwise an inline interactive list appears:

```
Linear Notifications — 3 unread notifications
▶ ICC-820 Expose installation report submissions as flat tables    5m ago
  ICC-719 Update API rate limiting strategy                       2h ago
  ICC-701 Fix authentication bug in mobile app                    1d ago
↑↓/jk: navigate  Enter: details  b: browser  m: mark read  r: reload  q: quit
```

Pressing `Enter` opens a full-screen details view for the selected notification. From either the list or the details view, `b` opens the notification URL in your browser without quitting, and `m` marks it as read and removes it from the list.

### Key bindings

#### List view

| Key | Action |
|-----|--------|
| `↑` / `k` | Move selection up |
| `↓` / `j` | Move selection down |
| `Enter` | Show selected notification in full-screen details view |
| `b` | Open selected notification URL in the browser (stay in app) |
| `m` | Mark selected notification as read |
| `r` | Reload the notification list |
| `q` / `Esc` | Quit |

#### Details view

| Key | Action |
|-----|--------|
| `b` | Open the notification URL in the browser |
| `m` | Mark the notification as read and return to the list |
| `Enter` / `q` / `Esc` | Return to the list |

## Development

```sh
pnpm build   # compile TypeScript to dist/
pnpm start   # run the compiled output
```
