# Instructions for AI Agents

When working on this repository, please follow these guidelines:

## Tools Table Maintenance

The README.md file contains a table listing all tools in the repository. **This table must be kept up-to-date** whenever:

- A new tool is added to the repository
- A tool is removed from the repository
- A tool's description changes significantly

The table format is:

```markdown
| Tool | Description |
|------|-------------|
| [tool-name](tool-name/) | Brief description of the tool |
```

Each tool name should be a link to its directory, and the description should be concise (one line).

## Java Code Formatting

All Java code in this repository must be formatted with **4-space indentation**. This applies to:

- Class and method bodies
- Control structures (if, for, while, etc.)
- Method chaining and fluent APIs
- All other indented code blocks

Do not use tabs for indentation in Java files.

## Shell Scripts

All shell scripts in this repository must follow these guidelines:

- Use `#!/usr/bin/env bash` as the shebang line instead of `#!/bin/bash`
  - This provides better portability across different systems where bash may be installed in different locations

## Node.js Programs

All Node.js/TypeScript projects in this repository must follow these guidelines:

- Use **pnpm** as the package manager
- Set `minimumReleaseAge: 4320` in `pnpm-workspace.yaml` (4320 minutes = 3 days):
  ```yaml
  minimumReleaseAge: 4320
  ```
  This is a pnpm supply-chain security feature that prevents installing package versions
  published fewer than 3 days ago, giving the community time to detect and pull compromised releases.

## Rust Projects

All Rust projects in this repository must follow these guidelines:

- Use `edition = "2024"` in Cargo.toml
  - This ensures projects use the latest Rust edition with modern language features and best practices
