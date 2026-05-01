# skagedal-tools

A collection of small tools.

## Tools

| Tool | Description |
|------|-------------|
| [git-dirty-checker](git-dirty-checker/) | Checks git repositories for uncommitted changes |
| [git-repos-latest-activity](git-repos-latest-activity/) | Lists git repositories sorted by date of latest commit |
| [log-jsonify](log-jsonify/) | Processes JSONL streams, wrapping non-JSON lines in JSON envelopes |
| [log-viewer](log-viewer/) | View JSONL logs in a TUI or browser with vi-like navigation and JSON drill-down |
| [rust-log-viewer](rust-log-viewer/) | Rust port of log-viewer (ratatui TUI plus an optional wry webview that embeds the same React app log-viewer's `--browser` mode uses) |
| [comparison-typescript-cli-arguments](comparison-typescript-cli-arguments/) | Side-by-side comparison of CLI argument parsing libraries for Node.js/TypeScript |
| [comparison-aws-emulation](comparison-aws-emulation/) | Comparison of tools for emulating AWS services locally (LocalStack, Moto, DynamoDB Local, MinIO, Adobe S3Mock) |
| [protobuf-text-to-json](protobuf-text-to-json/) | Converts protobuf text format to JSON |
| [x-java-home](x-java-home/) | A drop-in replacement for macOS java_home with JSON output support |
| [linear-notifications](linear-notifications/) | Interactive CLI for viewing and opening unread Linear notifications |
| [cloudwatch-insights](cloudwatch-insights/) | Download logs from AWS CloudWatch Logs Insights with a flexible time-range syntax |
| [gh-pr](gh-pr/) | Manage GitHub pull requests for the current branch via the `gh` CLI |
| [intellij-patch](intellij-patch/) | Apply XML patches to IntelliJ project files from a TOML config |
| [sync-brewfile](sync-brewfile/) | Reconcile locally installed Homebrew packages against a Brewfile, prompting to add or uninstall each extra |
| [package-json-merge](package-json-merge/) | Git merge driver for `package.json` that picks the higher semver range when both branches bumped the same dependency |
