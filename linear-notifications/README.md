# linear-notifications

Interactive CLI for viewing and opening unread [Linear](https://linear.app) notifications in the terminal.

## Requirements

- [Node.js](https://nodejs.org) (v18+)
- The [`linear`](https://github.com/linear/linear) CLI, authenticated and available on your `PATH`

## Installation

```sh
npm install -g .
```

This builds the TypeScript source and installs the `linear-notifications` command globally.

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
↑↓/jk: navigate  Enter: open  r: reload  q/Esc: quit
```

### Key bindings

| Key | Action |
|-----|--------|
| `↑` / `k` | Move selection up |
| `↓` / `j` | Move selection down |
| `Enter` | Open selected notification URL in the browser |
| `r` | Reload the notification list |
| `q` / `Esc` | Quit |

## Development

```sh
npm run build   # compile TypeScript to dist/
npm start       # run the compiled output
```
