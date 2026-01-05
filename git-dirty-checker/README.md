# git-dirty-checker

A tool to find git repositories with uncommitted changes (dirty working trees) in subdirectories of specified paths.

## Purpose

This tool helps you quickly identify which repositories in a collection of git repositories have uncommitted changes. This is useful when you maintain multiple repositories and want to ensure you haven't forgotten to commit or push changes.

## Implementations

This repository contains multiple implementations of the same functionality:

- **[java/](java/)** - A Java implementation that can be run as a standalone JAR
- **[rust/](rust/)** - A Rust implementation with parallel processing for high performance
- **[shell/](shell/)** - Shell script implementations (both sequential and parallel versions)

Each implementation provides the same core functionality: given one or more directory paths, it will search for git repositories in subdirectories and report which ones have uncommitted changes.

## Usage

See the README in each subdirectory for implementation-specific usage instructions.
