# skagedal-tools

A collection of small tools.

## Tools

- [appicon-generator](appicon-generator/) — Generates placeholder app icons from an emoji, for Xcode and Flutter projects
- [appletv-vlc](appletv-vlc/) — Plays a local movie file on an Apple TV in VLC, serving it over HTTP and opening it on the TV
- [assistant](assistant/) — Drives a daily routine by running configured tasks when they're due
- [chrome-page-notes](chrome-page-notes/) — Chrome extension, plus the CLI that registers and serves it, for attaching your own notes to a page
- [cloudwatch-insights](cloudwatch-insights/) — Download logs from AWS CloudWatch Logs Insights with a flexible time-range syntax
- [disky](disky/) — Offload and onload big directories between this machine and a remote, deleting locally only after checksum verification
- [gh-pr](gh-pr/) — Manage GitHub pull requests for the current branch via the `gh` CLI
- [git-branch-assistant](git-branch-assistant/) — Interactively syncs local git branches with their upstreams across one or many repos, and lists repos by latest commit
- [git-dirty-checker](git-dirty-checker/) — Checks git repositories for uncommitted changes
- [imessage-log](imessage-log/) — Print Messages history for a span of days, optionally only the conversations with one contact
- [intellij-patch](intellij-patch/) — Apply XML patches to IntelliJ project files from a TOML config
- [kontoutdrag](kontoutdrag/) — Identify merchants and categorise spending in a bank statement export, using YAML merchant tables
- [linear-notifications](linear-notifications/) — Interactive CLI for viewing and opening unread Linear notifications
- [log-jsonify](log-jsonify/) — Processes JSONL streams, wrapping non-JSON lines in JSON envelopes
- [log-viewer](log-viewer/) — View JSONL logs in a TUI or in a webview-embedded React app, with vi-like navigation and JSON drill-down
- [package-json-merge](package-json-merge/) — Git merge driver for `package.json` that picks the higher semver range when both branches bumped the same dependency
- [protobuf-text-to-json](protobuf-text-to-json/) — Converts protobuf text format to JSON
- [simons-misc-helpers](simons-misc-helpers/) — Miscellaneous helpers; currently formats the JSON output of `git pkgs diff --format=json` with colors
- [sync-brewfile](sync-brewfile/) — Reconcile locally installed Homebrew packages against a Brewfile, prompting to add or uninstall each extra
- [tracker](tracker/) — Tracks weekly work hours in a simple per-week text file with start/stop/report commands
- [trafikverket](trafikverket/) — The next trains between two stations, with live delays, filtered to the ones your ticket covers
- [wifi-info](wifi-info/) — Prints the current Wi-Fi network name and BSSID, which macOS otherwise redacts for command-line tools
- [woke](woke/) — Keeps this Mac awake with the lid closed, restoring the setting when it exits
- [x-java-home](x-java-home/) — A drop-in replacement for macOS `java_home` with JSON output support

## Libraries

Shared code used by the tools above.

- [companion-link](companion-link/) — IO-free client for the parts of Apple's Companion protocol needed to pair with an Apple TV and launch apps
- [skagedal-dirs](skagedal-dirs/) — The XDG config/data/cache directories every tool stores its files in
- [webview-shell](webview-shell/) — Builds, embeds and serves a React app, and opens it in a webview window

## Writing

- [codestyle](codestyle/) — Conventions that apply across repos, not just this one
- [comparison-aws-emulation](comparison-aws-emulation/) — Local emulators for S3, DynamoDB, SSM and Logs Insights: LocalStack and its 2026 successors, Moto, and single-service S3 and DynamoDB servers
- [comparison-typescript-cli-arguments](comparison-typescript-cli-arguments/) — Side-by-side comparison of CLI argument parsing libraries for Node.js/TypeScript
- [comparison-typescript-codemods](comparison-typescript-codemods/) — Overview of codemod tooling for TypeScript
- [specs](specs/) — Design specs for changes that reach past a single tool
