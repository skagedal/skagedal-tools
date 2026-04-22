# cloudwatch-insights

Download logs from [AWS CloudWatch Logs Insights](https://docs.aws.amazon.com/AmazonCloudWatch/latest/logs/AnalyzingLogData.html) from the command line, with a flexible time-range syntax and per-git-repository defaults.

## Requirements

- [Node.js](https://nodejs.org) (v20+, required by [gunshi](https://github.com/kazupon/gunshi))
- AWS credentials available through the standard SDK chain (env vars, `~/.aws/credentials`, SSO, IMDS, …)

## Installation

Install globally using pnpm:

```sh
pnpm link --global
```

This builds the TypeScript source and installs the `cloudwatch-insights` command on your `PATH`. After installation you can run `cloudwatch-insights` from anywhere.

> `pnpm install -g .` is broken in pnpm v10, so use `pnpm link --global`.

## Usage

```sh
cloudwatch-insights -g <log-group> -t <time-range> [-q '<insights query>']
```

### Examples

```sh
# Last 5 hours of a default query (fields @timestamp, @message | sort @timestamp desc)
cloudwatch-insights -g /aws/lambda/my-func -t 5h

# A specific minute today, with a custom query
cloudwatch-insights -g /aws/lambda/my-func -t 13.00-13.01 \
  -q 'fields @timestamp, @message | filter @message like /ERROR/'

# Millisecond granularity
cloudwatch-insights -g /my/group -t 09:15:00.000-09:15:00.500

# A time range on a named day
cloudwatch-insights -g /my/group -t "yesterday 17-18"

# No -g needed when there's a matching config section — just supply --environment
cloudwatch-insights -t 30m -e systest
```

### Time-range syntax

The `-t / --time` flag accepts, in order of precedence:

| Form | Examples | Meaning |
|------|----------|---------|
| Relative duration | `5h`, `30m`, `45s`, `500ms`, `2d`, `1w` | Last N units up to now |
| Day keyword + time range | `yesterday 17-18`, `today 13.00-13.01.30` | Range on a named day |
| Bare day keyword | `today`, `yesterday` | Entire day (local time) |
| Time-only range | `13.00-13.01`, `13:00-13:01`, `09.15.30.000-09.15.30.500` | Range on today (local time). Wraps past midnight. |
| Explicit ISO range | `2026-04-22T13:00:00Z/2026-04-22T14:00:00Z` | Separator `/`, `..`, or ` to ` |
| Natural language | `"last Monday 9am to 5pm"` | Parsed by [chrono-node](https://github.com/wanasit/chrono) as a fallback |

Times use `.` or `:` as the sub-separator. Milliseconds are introduced with a `.` after the seconds (e.g. `09:15:00.500`).

### Per-repository defaults (`settings.toml`)

The tool detects the git repository you're running it from (via `git rev-parse --show-toplevel`) and looks up a section keyed by the repo's directory name in:

```
~/.config/cloudwatch-insights/settings.toml
```

Overridable with `$XDG_CONFIG_HOME` or the explicit `$CLOUDWATCH_INSIGHTS_CONFIG` env var.

```toml
# settings.toml
[my-service]
group = "/{env}/my-team"
app   = "my-service"

[another-repo]
group = "/prod/another"
```

Fields:

- `group` — log group used when `--log-group` is not given. The literal string `{env}` is substituted with the value of `--environment`.
- `app` — if neither `--query` nor `--query-file` is given, the default query becomes:
  ```
  fields @timestamp, @message, app | filter app = "<app>" | sort @timestamp desc
  ```

### `--environment`

```
-e, --environment <env>      systest | uat | prod
```

Substituted into `{env}` in the log group template (either from config or from `--log-group`). Required whenever the template contains `{env}`.

### Output formats

- `jsonl` (default) — one JSON object per row, one row per line.
- `json` — a pretty-printed JSON array.

Progress (query status, row counts, byte totals) is written to stderr so piping stdout is safe. Use `--quiet` to silence it.

### Credentials and region

Standard AWS SDK resolution applies:

- `--region` overrides `AWS_REGION` / `AWS_DEFAULT_REGION`.
- `--profile` sets `AWS_PROFILE` for the process.
- Otherwise the default credentials chain is used.

## Development

```sh
pnpm install
pnpm build                         # compile TypeScript to dist/
pnpm dev -- -g /my/group -t 5h     # run directly from src/ via tsx
pnpm test                          # time-range + config tests
```
