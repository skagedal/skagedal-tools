use clap::{Args, Parser, Subcommand, ValueEnum};
use regex::Regex;
use serde_json::Value;
use std::io::IsTerminal;
use std::process::{Command, Stdio, exit};

/// Manage GitHub pull requests for the current branch.
#[derive(Parser)]
#[command(name = "gh-pr", version, about, long_about = None)]
struct Cli {
    #[command(flatten)]
    default_args: DefaultArgs,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Args)]
struct DefaultArgs {
    /// Create the PR (as draft) if it doesn't exist
    #[arg(short, long)]
    create: bool,

    /// Open the PR URL in the default browser
    #[arg(short, long)]
    open: bool,

    /// Base branch to target when creating a PR
    #[arg(long, value_name = "BRANCH")]
    toward: Option<String>,
}

#[derive(Subcommand)]
enum Commands {
    /// Print the PR's comments (both conversation and inline review comments).
    Comments(CommentsArgs),
    /// Mark files in the PR as "Viewed" for the current user.
    MarkViewed(MarkViewedArgs),
    /// Mark the PR as ready for review
    MarkReady,
}

#[derive(Args)]
struct MarkViewedArgs {
    /// By default these are exact file paths (as they appear in the diff) to
    /// mark as viewed. With --regex, they are treated as regular expressions
    /// matched against every changed file in the PR.
    #[arg(value_name = "FILE", required = true)]
    files: Vec<String>,

    /// Treat the arguments as regular expressions and mark every changed file
    /// in the PR whose path matches any of them.
    #[arg(short, long)]
    regex: bool,
}

#[derive(Args)]
struct CommentsArgs {
    /// Output format. Text is human/agent-readable; json dumps the raw API
    /// responses in a single container object.
    #[arg(long, value_enum, default_value_t = CommentsFormat::Text)]
    format: CommentsFormat,

    /// Include comments belonging to resolved review threads (hidden by
    /// default in text format). Has no effect on --format json, which always
    /// includes everything.
    #[arg(long)]
    resolved: bool,
}

#[derive(ValueEnum, Clone, Copy)]
enum CommentsFormat {
    Text,
    Json,
}

fn main() {
    let cli = Cli::parse();
    match cli.command {
        None => default_command(cli.default_args),
        Some(Commands::Comments(args)) => comments_command(args),
        Some(Commands::MarkViewed(args)) => mark_viewed_command(args),
        Some(Commands::MarkReady) => mark_ready_command(),
    }
}

fn default_command(args: DefaultArgs) {
    let url = match find_pr_url() {
        Some(url) => url,
        None => {
            if args.create {
                create_pr(args.toward.as_deref())
            } else {
                eprintln!("No pull request found for the current branch");
                exit(1);
            }
        }
    };

    println!("{}", url);

    if args.open && let Err(e) = opener::open_browser(&url) {
        eprintln!("Failed to open browser: {}", e);
        exit(1);
    }
}

fn find_pr_url() -> Option<String> {
    let output = run_gh(&["pr", "view", "--json", "url", "--jq", ".url"], true)?;
    if !output.status.success() {
        return None;
    }
    let url = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if url.is_empty() { None } else { Some(url) }
}

fn create_pr(toward: Option<&str>) -> String {
    let base = match toward {
        Some(b) => b.to_string(),
        None => default_branch().unwrap_or_else(|| "main".to_string()),
    };

    push_current_branch();

    let create_args = ["pr", "create", "--draft", "--fill", "--base", &base];
    echo_gh(&create_args);
    let output = Command::new("gh")
        .args(create_args)
        .output()
        .unwrap_or_else(|e| {
            eprintln!("Failed to run gh: {}", e);
            exit(1);
        });

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        eprint!("{}", stderr);
        if !stderr.ends_with('\n') {
            eprintln!();
        }
        exit(1);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout
        .lines()
        .rev()
        .find(|l| l.starts_with("http"))
        .map(str::to_string)
        .unwrap_or_else(|| {
            eprintln!("Could not determine PR URL from gh output");
            exit(1);
        })
}

fn push_current_branch() {
    let status = Command::new("git")
        .args(["push", "-u", "origin", "HEAD"])
        .status()
        .unwrap_or_else(|e| {
            eprintln!("Failed to run git: {}", e);
            exit(1);
        });
    if !status.success() {
        exit(1);
    }
}

fn default_branch() -> Option<String> {
    let output = run_gh(
        &[
            "repo",
            "view",
            "--json",
            "defaultBranchRef",
            "--jq",
            ".defaultBranchRef.name",
        ],
        true,
    )?;
    if !output.status.success() {
        return None;
    }
    let name = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if name.is_empty() { None } else { Some(name) }
}

fn comments_command(args: CommentsArgs) {
    let number = match find_pr_number() {
        Some(n) => n,
        None => {
            eprintln!("No pull request found for the current branch");
            exit(1);
        }
    };

    // One GraphQL call returns both conversation comments and review
    // threads (the latter pre-grouped, with their `isResolved` state). The
    // REST endpoints `/issues/N/comments` and `/pulls/N/comments` would
    // each return half of this, with no resolved state — so GraphQL is
    // both simpler and more complete.
    let response = fetch_pr_comments(number);

    match args.format {
        CommentsFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&response).unwrap_or_else(|e| {
                    eprintln!("Failed to serialize JSON: {}", e);
                    exit(1);
                })
            );
        }
        CommentsFormat::Text => {
            print_text(&response, args.resolved);
        }
    }
}

fn fetch_pr_comments(number: u64) -> Value {
    // first: 100 is GitHub's GraphQL max page size. We don't paginate — PRs
    // with more comments are vanishingly rare; bump if that changes.
    let query = "query($owner: String!, $repo: String!, $number: Int!) { \
        repository(owner: $owner, name: $repo) { \
            pullRequest(number: $number) { \
                comments(first: 100) { \
                    nodes { \
                        databaseId \
                        author { login } \
                        createdAt \
                        body \
                    } \
                } \
                reviews(first: 100) { \
                    nodes { \
                        databaseId \
                        author { login } \
                        submittedAt \
                        state \
                        body \
                    } \
                } \
                reviewThreads(first: 100) { \
                    nodes { \
                        isResolved \
                        isOutdated \
                        path \
                        line \
                        originalLine \
                        comments(first: 100) { \
                            nodes { \
                                databaseId \
                                author { login } \
                                createdAt \
                                body \
                            } \
                        } \
                    } \
                } \
            } \
        } \
    }";

    let name_with_owner = match name_with_owner() {
        Some(s) => s,
        None => {
            eprintln!("Failed to determine repository owner/name");
            exit(1);
        }
    };
    let (owner, repo) = match name_with_owner.split_once('/') {
        Some((o, r)) => (o.to_string(), r.to_string()),
        None => {
            eprintln!("Unexpected nameWithOwner format: {}", name_with_owner);
            exit(1);
        }
    };

    let owner_arg = format!("owner={}", owner);
    let repo_arg = format!("repo={}", repo);
    let number_arg = format!("number={}", number);
    let query_arg = format!("query={}", query);
    fetch_json(&[
        "api",
        "graphql",
        "-f",
        &owner_arg,
        "-f",
        &repo_arg,
        "-F",
        &number_arg,
        "-f",
        &query_arg,
    ])
}

fn name_with_owner() -> Option<String> {
    let output = run_gh(
        &[
            "repo",
            "view",
            "--json",
            "nameWithOwner",
            "--jq",
            ".nameWithOwner",
        ],
        true,
    )?;
    if !output.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if s.is_empty() { None } else { Some(s) }
}

fn fetch_json(args: &[&str]) -> Value {
    echo_gh(args);
    let output = Command::new("gh")
        .args(args)
        .stderr(Stdio::inherit())
        .output()
        .unwrap_or_else(|e| {
            eprintln!("Failed to run gh: {}", e);
            exit(1);
        });
    if !output.status.success() {
        exit(output.status.code().unwrap_or(1));
    }
    serde_json::from_slice(&output.stdout).unwrap_or_else(|e| {
        eprintln!("Failed to parse JSON from gh: {}", e);
        exit(1);
    })
}

fn print_text(response: &Value, show_resolved: bool) {
    let empty: Vec<Value> = Vec::new();
    let convo = response
        .pointer("/data/repository/pullRequest/comments/nodes")
        .and_then(Value::as_array)
        .unwrap_or(&empty);
    let reviews = response
        .pointer("/data/repository/pullRequest/reviews/nodes")
        .and_then(Value::as_array)
        .unwrap_or(&empty);
    let threads = response
        .pointer("/data/repository/pullRequest/reviewThreads/nodes")
        .and_then(Value::as_array)
        .unwrap_or(&empty);

    println!("=== Conversation comments ({}) ===", convo.len());
    if convo.is_empty() {
        println!();
        println!("(none)");
    }
    for c in convo {
        println!();
        print_comment_header(c, "", false);
        print_body(c, "");
    }

    println!();

    // Review summaries: a review's `body` is the top-level message the reviewer
    // typed when submitting (e.g. "Looks good but please address X"). Inline
    // comments belonging to the same review live under `reviewThreads` and are
    // rendered there. Reviews with an empty body are skipped — they contributed
    // only inline comments, already shown below.
    let review_summaries: Vec<&Value> = reviews
        .iter()
        .filter(|r| {
            let body = r.get("body").and_then(Value::as_str).unwrap_or("");
            let state = r.get("state").and_then(Value::as_str).unwrap_or("");
            !body.is_empty() && state != "PENDING"
        })
        .collect();

    println!("=== Review summaries ({}) ===", review_summaries.len());
    if review_summaries.is_empty() {
        println!();
        println!("(none)");
    }
    for r in &review_summaries {
        println!();
        print_review_summary_header(r);
        print_body(r, "");
    }

    println!();

    let is_resolved = |t: &Value| t.get("isResolved").and_then(Value::as_bool).unwrap_or(false);
    let visible: Vec<&Value> = threads
        .iter()
        .filter(|t| show_resolved || !is_resolved(t))
        .collect();
    let hidden = threads.len() - visible.len();

    let suffix = if hidden > 0 && !show_resolved {
        format!(", {} resolved hidden", hidden)
    } else {
        String::new()
    };
    println!("=== Review threads ({}{}) ===", visible.len(), suffix);

    if visible.is_empty() {
        println!();
        println!("(none)");
    }

    for thread in &visible {
        let path = thread.get("path").and_then(Value::as_str).unwrap_or("?");
        let line = thread
            .get("line")
            .and_then(Value::as_i64)
            .or_else(|| thread.get("originalLine").and_then(Value::as_i64));
        let location = match line {
            Some(l) => format!("{}:{}", path, l),
            None => path.to_string(),
        };
        let res_tag = if is_resolved(thread) { " [RESOLVED]" } else { "" };

        println!();
        println!("— {}{}", location, res_tag);

        let comments = thread
            .pointer("/comments/nodes")
            .and_then(Value::as_array)
            .unwrap_or(&empty);
        for (i, c) in comments.iter().enumerate() {
            if i > 0 {
                println!();
            }
            let indent = if i == 0 { "" } else { "  " };
            print_comment_header(c, indent, i > 0);
            print_body(c, indent);
        }
    }
}

fn print_review_summary_header(r: &Value) {
    let author = r
        .pointer("/author/login")
        .and_then(Value::as_str)
        .unwrap_or("ghost");
    let submitted = r
        .get("submittedAt")
        .and_then(Value::as_str)
        .unwrap_or("?");
    let state = r.get("state").and_then(Value::as_str).unwrap_or("?");
    println!("[{}] @{} ({}):", submitted, author, state);
}

fn print_comment_header(c: &Value, indent: &str, is_reply: bool) {
    let author = c
        .pointer("/author/login")
        .and_then(Value::as_str)
        .unwrap_or("ghost");
    let created = c.get("createdAt").and_then(Value::as_str).unwrap_or("?");
    let suffix = if is_reply { " (reply)" } else { "" };
    println!("{}[{}] @{}{}:", indent, created, author, suffix);
}

fn print_body(c: &Value, indent: &str) {
    let body = c.get("body").and_then(Value::as_str).unwrap_or("");
    for line in body.lines() {
        println!("{}{}", indent, line);
    }
}

fn find_pr_number() -> Option<u64> {
    let output = run_gh(&["pr", "view", "--json", "number", "--jq", ".number"], true)?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8_lossy(&output.stdout).trim().parse().ok()
}

fn mark_viewed_command(args: MarkViewedArgs) {
    let (pr_id, number) = match find_pr_id_and_number() {
        Some(pair) => pair,
        None => {
            eprintln!("No pull request found for the current branch");
            exit(1);
        }
    };

    let paths: Vec<String> = if args.regex {
        let regexes: Vec<Regex> = args
            .files
            .iter()
            .map(|p| {
                Regex::new(p).unwrap_or_else(|e| {
                    eprintln!("Invalid regex {:?}: {}", p, e);
                    exit(2);
                })
            })
            .collect();

        let matched: Vec<String> = fetch_pr_files(number)
            .into_iter()
            .filter(|f| regexes.iter().any(|re| re.is_match(f)))
            .collect();

        if matched.is_empty() {
            eprintln!("No changed files in the PR matched the given pattern(s)");
            exit(1);
        }
        matched
    } else {
        args.files
    };

    let mut failures = 0;
    for path in &paths {
        if mark_file_viewed(&pr_id, path) {
            println!("Viewed: {}", path);
        } else {
            failures += 1;
        }
    }
    if failures > 0 {
        exit(1);
    }
}

/// Fetch the PR's node ID (needed for the GraphQL mutation) and number in a
/// single `gh` call. Returns None when there's no PR for the current branch.
fn find_pr_id_and_number() -> Option<(String, u64)> {
    let output = run_gh(&["pr", "view", "--json", "id,number"], true)?;
    if !output.status.success() {
        return None;
    }
    let value: Value = serde_json::from_slice(&output.stdout).ok()?;
    let id = value.get("id")?.as_str()?.to_string();
    let number = value.get("number")?.as_u64()?;
    Some((id, number))
}

/// All changed file paths in the PR, across every page. Uses the REST
/// `pulls/{n}/files` endpoint with `--paginate` (which merges the array pages
/// into one), so it isn't capped at GitHub's 100-per-page limit — generated
/// directories can easily exceed that.
fn fetch_pr_files(number: u64) -> Vec<String> {
    let name_with_owner = match name_with_owner() {
        Some(s) => s,
        None => {
            eprintln!("Failed to determine repository owner/name");
            exit(1);
        }
    };
    let path = format!("repos/{}/pulls/{}/files", name_with_owner, number);
    let response = fetch_json(&["api", "--paginate", &path]);
    response
        .as_array()
        .map(|files| {
            files
                .iter()
                .filter_map(|f| f.get("filename").and_then(Value::as_str).map(String::from))
                .collect()
        })
        .unwrap_or_default()
}

/// Mark a single file as viewed via the `markFileAsViewed` GraphQL mutation.
/// Returns false (and lets gh print its error) rather than exiting, so a bad
/// path among several doesn't abort the rest.
fn mark_file_viewed(pr_id: &str, path: &str) -> bool {
    let query = "mutation($pullRequestId: ID!, $path: String!) { \
        markFileAsViewed(input: { pullRequestId: $pullRequestId, path: $path }) { \
            pullRequest { id } \
        } \
    }";
    let pr_arg = format!("pullRequestId={}", pr_id);
    let path_arg = format!("path={}", path);
    let query_arg = format!("query={}", query);
    let args = [
        "api", "graphql", "-f", &pr_arg, "-f", &path_arg, "-f", &query_arg,
    ];
    echo_gh(&args);
    let output = Command::new("gh")
        .args(args)
        .stderr(Stdio::inherit())
        .output()
        .unwrap_or_else(|e| {
            eprintln!("Failed to run gh: {}", e);
            exit(1);
        });
    output.status.success()
}

fn mark_ready_command() {
    let ready_args = ["pr", "ready"];
    echo_gh(&ready_args);
    let status = Command::new("gh")
        .args(ready_args)
        .status()
        .unwrap_or_else(|e| {
            eprintln!("Failed to run gh: {}", e);
            exit(1);
        });
    if !status.success() {
        exit(1);
    }
}

fn run_gh(args: &[&str], silence_stderr: bool) -> Option<std::process::Output> {
    echo_gh(args);
    let mut cmd = Command::new("gh");
    cmd.args(args);
    if silence_stderr {
        cmd.stderr(Stdio::null());
    }
    match cmd.output() {
        Ok(o) => Some(o),
        Err(e) => {
            eprintln!("Failed to run gh: {}", e);
            exit(1);
        }
    }
}

fn echo_gh(args: &[&str]) {
    let line = format!("gh {}", args.join(" "));
    if std::io::stderr().is_terminal() {
        eprintln!("\x1b[32m{}\x1b[0m", line);
    } else {
        eprintln!("{}", line);
    }
}
