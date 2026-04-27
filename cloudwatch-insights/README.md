# cloudwatch-insights

Download logs from [AWS CloudWatch Logs Insights](https://docs.aws.amazon.com/AmazonCloudWatch/latest/logs/AnalyzingLogData.html) from the command line, with a flexible time-range syntax, per-git-repository defaults, and persistent editable query files.

## Requirements

- [Node.js](https://nodejs.org) (v20+, required by [gunshi](https://github.com/kazupon/gunshi))
- AWS credentials available through the standard SDK chain (env vars, `~/.aws/credentials`, SSO, IMDS, …)

## Installation

Install globally using pnpm:

```sh
pnpm link --global
```

This builds the TypeScript source and installs the `cloudwatch-insights` command on your `PATH`. The `install` script at the repo root does the same for every tool in the repo.

## Subcommands

```
cloudwatch-insights query [options]       run a query
cloudwatch-insights show                  stream the latest run to stdout
cloudwatch-insights edit-config           edit the per-repo config
```

### `query`

By default, `query` opens your `$EDITOR` (or `$VISUAL`) on a persistent `current.insights` file so you can tweak the query and its parameters. Save and exit, and the query runs. Pass `--query` or `--query-file` to skip the editor.

```sh
# First run: seeds current.insights from your config, opens it in $EDITOR
cloudwatch-insights query

# Subsequent runs: reuse the same file (edit, save, exit, execute)
cloudwatch-insights query

# Skip the editor with an inline query
cloudwatch-insights query -g /aws/lambda/my-func -t 5h \
  -q 'fields @timestamp, @message | filter @message like /ERROR/'

# Override the time range at invocation time
cloudwatch-insights query -t "yesterday 17-18" -e systest
```

Results are written as JSONL to:

```
~/.skagedal-tools/cloudwatch-insights/queries/<repo>/results/run-<timestamp>.jsonl
```

The path of the run file is the only thing printed to stdout, making it easy to capture:

```sh
last=$(cloudwatch-insights query)
jq . < "$last"
```

After every successful run the symlink `~/.skagedal-tools/cloudwatch-insights/latest-run.jsonl` is updated to point at the new file.

### `show`

```sh
cloudwatch-insights show
cloudwatch-insights show | jq .
cloudwatch-insights show --path        # print the path to latest-run.jsonl instead
```

Streams `~/.skagedal-tools/cloudwatch-insights/latest-run.jsonl` to stdout. Handy for re-inspecting or re-formatting the most recent run without re-querying. Pass `--path` to print just the file path (useful for piping into other tools).

### `edit-config`

```sh
cloudwatch-insights edit-config
```

Opens `~/.skagedal-tools/cloudwatch-insights/settings.toml` in `$EDITOR`, creating the file (and a commented placeholder section for the current git repository) on first use.

## The `.insights` file format

`current.insights` has an optional TOML **front-matter** block followed by a `---` separator and then the query body:

```
time = "5h"
env = "systest"
log-group = "/my/group"
---
fields @timestamp, @message
| sort @timestamp desc
| filter app = '{{ app }}'
| filter level in ['WARN', 'ERROR']
| limit 200
```

Front-matter fields:

| Field         | Purpose                                                             |
|---------------|---------------------------------------------------------------------|
| `time`        | time range (same syntax as `--time`)                                |
| `env`         | substituted for `{{ env }}` in the query and log group templates    |
| `app`         | substituted for `{{ app }}` in the query and log group templates (overrides the configured `app`) |
| `log-group`   | a log group, or an array of log groups                              |

CLI flags always win over front-matter, and front-matter wins over the repo defaults in `settings.toml`. The result `limit` belongs in the query body itself (`| limit 200`), not the front-matter.

The query body and log group templates may contain Jinja-style placeholders `{{ env }}` and `{{ app }}` (whitespace inside the braces is allowed). They're expanded at query-execution time — the file you edit always shows the unexpanded source. `{{ env }}` resolves from `--environment` or the front-matter `env`; `{{ app }}` resolves from the front-matter `app` or the configured `app` for the repo. Referencing a placeholder with no corresponding value is an error.

When seeding a fresh `current.insights` (first run, or with `--new`), the front-matter is **pre-filled** with the values that would currently be used — `--time` (or `1h`), `--environment`, the configured `app`, and the CLI `--log-group` or the configured `group` template — so you can see and tweak exactly what will run. Existing files are never rewritten.

Pass `--new` to `query` to overwrite the current file with a fresh default template before opening the editor.

## Time-range syntax

The `-t / --time` flag (and the front-matter `time:` key) accepts, in order of precedence:

| Form | Examples | Meaning |
|------|----------|---------|
| Relative duration | `5h`, `30m`, `45s`, `500ms`, `2d`, `1w` | Last N units up to now |
| Day keyword + time range | `yesterday 17-18`, `today 13.00-13.01.30` | Range on a named day |
| Bare day keyword | `today`, `yesterday` | Entire day (local time) |
| Time-only range | `13.00-13.01`, `13:00-13:01`, `09.15.30.000-09.15.30.500` | Range on today (local time). Wraps past midnight. |
| Explicit ISO range | `2026-04-22T13:00:00Z/2026-04-22T14:00:00Z` | Separator `/`, `..`, or ` to ` |
| Natural language | `"last Monday 9am to 5pm"` | Parsed by [chrono-node](https://github.com/wanasit/chrono) as a fallback |

Times use `.` or `:` as the sub-separator. Milliseconds are introduced with a `.` after the seconds (e.g. `09:15:00.500`).

Default: **1h** (last hour) if neither CLI nor front-matter specifies one.

## Per-repository defaults (`settings.toml`)

The tool detects the git repository you're running it from (via `git rev-parse --show-toplevel`) and looks up a section keyed by the repo's directory name in:

```
~/.skagedal-tools/cloudwatch-insights/settings.toml
```

Per the skagedal-tools convention, per-tool state lives under `~/.skagedal-tools/<tool-name>/`. Override the whole base directory with `$SKAGEDAL_TOOLS_HOME`, or point at an arbitrary config file with `$CLOUDWATCH_INSIGHTS_CONFIG`.

The file has a top-level `[defaults]` block that applies to every repo, plus per-repo `[repo.<name>]` blocks. Per-repo values fully replace the corresponding `[defaults]` value (arrays are not merged).

```toml
# settings.toml
[defaults]
flatten-fields = ["@message"]

[env.prod]
aws-profile = "company-prod"
region      = "us-east-1"

[env.systest]
aws-profile = "company-systest"
region      = "eu-west-1"

[repo.my-service]
group = "/{{ env }}/logs"
app   = "my-service"

[repo.another-repo]
group = "/prod/another"
flatten-fields = ["@message", "context"]   # replaces the defaults entry entirely

# Per-repo override for one env: only keys you mention are overridden.
# Region for special-service+prod still comes from [env.prod].
[repo.special-service.env.prod]
aws-profile = "special-prod"
```

Fields legal in both `[defaults]` and `[repo.<name>]`:

- `group` — log group used when no `--log-group` / front-matter `log-group` is given. `{{ env }}` and `{{ app }}` are expanded at query time.
- `app` — used only when seeding a fresh `current.insights`: the seeded query body will filter on `app = "<app>"`.
- `flatten-fields` — array of field names whose value is a JSON-encoded object. After each query, those fields are JSON-decoded and their keys merged into the row (the original field is removed). If a value isn't valid JSON or isn't a plain object, the field is left untouched. Useful for log-line fields like `@message` that wrap structured payloads.

`[env.<name>]` declares an environment (any lower kebab-case name — e.g. `prod`, `systest`, `us-east-1`). Supported keys:

- `aws-profile` — exported as `AWS_PROFILE` for the run unless `--profile` overrides it.
- `region` — passed to the AWS SDK unless `--region` / `AWS_REGION` overrides it.

A repo can override individual keys via `[repo.<repo>.env.<env>]`. Inside an env block the merge is **per-key**: only the fields you mention are overridden; the rest fall through to the top-level entry. Resolution order at query time: `--profile` / `--region` > `[repo.X.env.Y]` > `[env.Y]` > `AWS_PROFILE` / `AWS_REGION` env vars > SDK default chain.

Using `--environment <name>` for an env name that has no `[env.<name>]` section is fine — `{{ env }}` substitution still works; `aws-profile` and `region` simply aren't auto-set.

## `--environment`

```
-e, --environment <name>     any lower kebab-case env name (e.g. prod, systest, us-east-1)
        --env <name>         alias for --environment
```

Substituted into `{{ env }}` in the query and log group template (from CLI, front-matter, or config) and used to look up `[env.<name>]` for `aws-profile` / `region`. Required whenever the template references `{{ env }}`. Unknown env names are accepted — they simply don't auto-set a profile or region.

## Directory layout

```
~/.skagedal-tools/cloudwatch-insights/
├── settings.toml                       # per-repo defaults
├── latest-run.jsonl                    # symlink to the most recent run
└── queries/
    ├── <repo-name>/
    │   ├── current.insights            # editor file for `query`
    │   └── results/
    │       └── run-<timestamp>.jsonl
    └── _default/                       # used when run outside any git repo
        ├── current.insights
        └── results/
```

## Credentials and region

Standard AWS SDK resolution applies:

- `--region` overrides `AWS_REGION` / `AWS_DEFAULT_REGION`.
- `--profile` sets `AWS_PROFILE` for the process.
- Otherwise the default credentials chain is used.

## Development

```sh
pnpm install
pnpm build                                   # compile TypeScript to dist/
pnpm dev -- query -g /my/group -t 5h         # run directly from src/ via tsx
pnpm test                                    # time-range, config, query-file tests
```
