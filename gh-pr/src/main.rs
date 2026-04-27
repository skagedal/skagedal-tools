use clap::{Args, Parser, Subcommand};
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
    /// Print the raw JSON of the PR's issue comments from the GitHub API
    Comments,
    /// Mark the PR as ready for review
    MarkReady,
}

fn main() {
    let cli = Cli::parse();
    match cli.command {
        None => default_command(cli.default_args),
        Some(Commands::Comments) => comments_command(),
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

fn comments_command() {
    let number = match find_pr_number() {
        Some(n) => n,
        None => {
            eprintln!("No pull request found for the current branch");
            exit(1);
        }
    };

    let path = format!("repos/{{owner}}/{{repo}}/issues/{}/comments", number);
    let api_args = ["api", path.as_str()];
    echo_gh(&api_args);
    let status = Command::new("gh")
        .args(api_args)
        .status()
        .unwrap_or_else(|e| {
            eprintln!("Failed to run gh: {}", e);
            exit(1);
        });
    if !status.success() {
        exit(1);
    }
}

fn find_pr_number() -> Option<u64> {
    let output = run_gh(&["pr", "view", "--json", "number", "--jq", ".number"], true)?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8_lossy(&output.stdout).trim().parse().ok()
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
