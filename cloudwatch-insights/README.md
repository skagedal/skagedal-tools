# cloudwatch-insights

Download logs from [AWS CloudWatch Logs Insights](https://docs.aws.amazon.com/AmazonCloudWatch/latest/logs/AnalyzingLogData.html) from the command line, with a flexible time-range syntax.

## Requirements

- [Node.js](https://nodejs.org) (v18+)
- AWS credentials available through the standard SDK chain (env vars, `~/.aws/credentials`, SSO, IMDS, …)

## Installation

```sh
pnpm link --global
```

This builds the TypeScript source and installs the `cloudwatch-insights` command globally.

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

# A natural-language time range on a named day
cloudwatch-insights -g /my/group -t "yesterday 17-18"

# Multiple log groups, JSON output, read query from a file
cloudwatch-insights -g /api/prod -g /api/staging -t 30m \
  -f query.txt -o json
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

### Output formats

- `ndjson` (default) — one JSON object per row, one row per line.
- `json` — a pretty-printed JSON array.
- `tsv` — tab-separated values with a header row (`@ptr` is omitted).

Progress (query status, row counts, byte totals) is written to stderr so piping the data side is safe. Use `--quiet` to silence it.

### Credentials and region

Standard AWS SDK resolution applies:

- `--region` overrides `AWS_REGION` / `AWS_DEFAULT_REGION`.
- `--profile` sets `AWS_PROFILE` for the process.
- Otherwise the default credentials chain is used.

## Development

```sh
pnpm install
pnpm build   # compile TypeScript to dist/
pnpm dev -- -g /my/group -t 5h   # run directly from src/ via tsx
pnpm test    # run the time-range parser tests
```
