//! Mock implementation of the `gh` CLI, supporting just enough surface area
//! for the `pr` tool's integration tests.
//!
//! Behavior is driven by environment variables:
//!
//! - `MOCK_GH_LOG`            — file to which each invocation appends its
//!   joined argv (without `gh`); enables tests to assert on the calls made.
//! - `MOCK_GH_PR_URL`         — `gh pr view --json url --jq .url` prints
//!   this. Unset/empty means "no PR for this branch" (exit 1).
//! - `MOCK_GH_PR_NUMBER`      — same, for `--json number --jq .number`.
//! - `MOCK_GH_DEFAULT_BRANCH` — `gh repo view --json defaultBranchRef --jq
//!   .defaultBranchRef.name` prints this. Unset means failure.
//! - `MOCK_GH_CREATE_URL`     — URL printed by `gh pr create`. Defaults to a
//!   placeholder if unset.
//! - `MOCK_GH_CREATE_FAIL`    — if set, `gh pr create` exits non-zero.
//! - `MOCK_GH_NAME_WITH_OWNER` — `gh repo view --json nameWithOwner --jq
//!   .nameWithOwner` prints this. Defaults to `me/repo`.
//! - `MOCK_GH_PR_COMMENTS_JSON` — body returned by `gh api graphql` for the
//!   PR-comments query. Defaults to an empty result envelope.
//! - `MOCK_GH_PR_NODE_ID`       — `id` returned by `gh pr view --json
//!   id,number`. Defaults to `PR_node`.
//! - `MOCK_GH_PR_FILES_JSON`    — body returned by `gh api .../pulls/N/files`.
//!   Defaults to an empty array.

use std::env;
use std::fs::OpenOptions;
use std::io::Write;
use std::process::exit;

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    log_invocation(&args);

    match args.first().map(String::as_str) {
        Some("pr") => match args.get(1).map(String::as_str) {
            Some("view") => pr_view(&args),
            Some("create") => pr_create(&args),
            Some("ready") => {} // success, no output
            other => unsupported("pr", other, &args),
        },
        Some("repo") => match args.get(1).map(String::as_str) {
            Some("view") => repo_view(&args),
            other => unsupported("repo", other, &args),
        },
        Some("api") => api(&args),
        other => unsupported("(top-level)", other, &args),
    }
}

fn log_invocation(args: &[String]) {
    let Ok(path) = env::var("MOCK_GH_LOG") else {
        return;
    };
    let Ok(mut f) = OpenOptions::new().create(true).append(true).open(&path) else {
        return;
    };
    let _ = writeln!(f, "{}", args.join(" "));
}

fn pr_view(args: &[String]) {
    // `mark-viewed` fetches id + number together without a --jq filter.
    if arg_value(args, "--jq").is_none()
        && arg_value(args, "--json").as_deref() == Some("id,number")
    {
        match env_nonempty("MOCK_GH_PR_NUMBER") {
            Some(number) => {
                let id = env::var("MOCK_GH_PR_NODE_ID").unwrap_or_else(|_| "PR_node".to_string());
                println!(r#"{{"id":"{}","number":{}}}"#, id, number);
            }
            None => no_pr(),
        }
        return;
    }
    match arg_value(args, "--jq").as_deref() {
        Some(".url") => match env_nonempty("MOCK_GH_PR_URL") {
            Some(url) => println!("{}", url),
            None => no_pr(),
        },
        Some(".number") => match env_nonempty("MOCK_GH_PR_NUMBER") {
            Some(n) => println!("{}", n),
            None => no_pr(),
        },
        other => {
            eprintln!("mock-gh: unsupported --jq for `pr view`: {:?}", other);
            exit(2);
        }
    }
}

fn pr_create(args: &[String]) {
    if env::var("MOCK_GH_CREATE_FAIL").is_ok() {
        eprintln!("mock-gh: `pr create` configured to fail");
        exit(1);
    }
    // Validate that a base was passed; the real `gh pr create` doesn't
    // require it, but the `pr` tool always passes one and we want tests to
    // notice if that ever changes.
    if arg_value(args, "--base").is_none() {
        eprintln!("mock-gh: `pr create` invoked without --base");
        exit(2);
    }
    let url = env::var("MOCK_GH_CREATE_URL")
        .unwrap_or_else(|_| "https://github.com/example/repo/pull/1".to_string());
    println!("{}", url);
}

fn repo_view(args: &[String]) {
    match arg_value(args, "--jq").as_deref() {
        Some(".defaultBranchRef.name") => match env_nonempty("MOCK_GH_DEFAULT_BRANCH") {
            Some(branch) => println!("{}", branch),
            None => {
                eprintln!("mock-gh: no default branch configured");
                exit(1);
            }
        },
        Some(".nameWithOwner") => {
            let name =
                env::var("MOCK_GH_NAME_WITH_OWNER").unwrap_or_else(|_| "me/repo".to_string());
            println!("{}", name);
        }
        other => {
            eprintln!("mock-gh: unsupported --jq for `repo view`: {:?}", other);
            exit(2);
        }
    }
}

fn api(args: &[String]) {
    let path = args
        .iter()
        .skip(1)
        .find(|a| !a.starts_with('-'))
        .map(String::as_str)
        .unwrap_or("");
    if path.is_empty() {
        eprintln!("mock-gh: `api` requires a path argument");
        exit(2);
    }
    let body = if path == "graphql" {
        if args.iter().any(|a| a.contains("markFileAsViewed")) {
            r#"{"data":{"markFileAsViewed":{"pullRequest":{"id":"PR_node"}}}}"#.to_string()
        } else {
            env::var("MOCK_GH_PR_COMMENTS_JSON").unwrap_or_else(|_| {
                r#"{"data":{"repository":{"pullRequest":{"comments":{"nodes":[]},"reviewThreads":{"nodes":[]}}}}}"#.to_string()
            })
        }
    } else if path.starts_with("repos/") && path.ends_with("/files") {
        env::var("MOCK_GH_PR_FILES_JSON").unwrap_or_else(|_| "[]".to_string())
    } else {
        eprintln!("mock-gh: unsupported api path: {}", path);
        exit(2);
    };
    println!("{}", body);
}

fn no_pr() {
    eprintln!("no pull requests found");
    exit(1);
}

fn unsupported(group: &str, sub: Option<&str>, args: &[String]) {
    eprintln!(
        "mock-gh: unsupported `{} {}` invocation: {}",
        group,
        sub.unwrap_or(""),
        args.join(" ")
    );
    exit(2);
}

fn arg_value(args: &[String], flag: &str) -> Option<String> {
    let mut iter = args.iter();
    while let Some(a) = iter.next() {
        if a == flag {
            return iter.next().cloned();
        }
    }
    None
}

fn env_nonempty(name: &str) -> Option<String> {
    env::var(name).ok().filter(|s| !s.is_empty())
}
