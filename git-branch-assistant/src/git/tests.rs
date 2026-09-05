#![cfg(test)]

use super::parse_branches;
use crate::git::{Branch, GitRepo, PrOptions, Upstream, UpstreamStatus, pr_create_args};
use anyhow::Result;
use std::path::PathBuf;
use std::process::Command;

#[test]
fn branch_needs_action_when_no_upstream() {
    let branch = Branch {
        refname: "feature".to_string(),
        upstream: None,
        worktree_path: None,
    };
    assert!(branch.needs_action());
}

#[test]
fn branch_needs_action_when_upstream_status_not_identical() {
    let branch = Branch {
        refname: "feature".to_string(),
        upstream: Some(Upstream {
            name: "origin/feature".to_string(),
            status: UpstreamStatus::MergeNeeded,
        }),
        worktree_path: None,
    };
    assert!(branch.needs_action());
}

#[test]
fn branch_no_action_when_identical() {
    let branch = Branch {
        refname: "feature".to_string(),
        upstream: Some(Upstream {
            name: "origin/feature".to_string(),
            status: UpstreamStatus::Identical,
        }),
        worktree_path: None,
    };
    assert!(!branch.needs_action());
}

#[test]
fn git_repo_dir_returns_path() {
    let dir = PathBuf::from("/tmp/example");
    let repo = GitRepo::new(dir.clone());
    assert_eq!(repo.dir(), dir.as_path());
}

fn test_repo(repo_name: &str) -> Result<GitRepo> {
    let temp_dir = tempfile::tempdir()?;
    let tarball_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(format!("{}.tar.gz", repo_name));

    // --no-same-owner prevents tar (when run as root) from preserving the
    // fixture's original UID, which would trigger git's safe.directory check.
    let status = Command::new("tar")
        .arg("xzf")
        .arg(&tarball_path)
        .arg("--no-same-owner")
        .current_dir(temp_dir.path())
        .status()?;

    if !status.success() {
        return Err(anyhow::anyhow!("tar extraction failed"));
    }

    let repo_path = temp_dir.path().join(repo_name);
    let _ = temp_dir.keep();
    Ok(GitRepo::new(repo_path))
}

#[test]
fn parse_branches_handles_all_upstream_states() -> Result<()> {
    let output = "\
main|origin/main||/repo
behind|origin/behind|[behind 2]|
ahead|origin/ahead|[ahead 1]|
diverged|origin/diverged|[ahead 1, behind 2]|
gone|origin/gone|[gone]|
local-only||||
";
    let branches = parse_branches(output)?;
    assert_eq!(branches.len(), 6);

    let by_name: std::collections::HashMap<_, _> =
        branches.iter().map(|b| (b.refname.as_str(), b)).collect();

    let main = by_name["main"];
    assert_eq!(
        main.upstream.as_ref().map(|u| u.status),
        Some(UpstreamStatus::Identical)
    );
    assert_eq!(main.worktree_path, Some(PathBuf::from("/repo")));

    assert_eq!(
        by_name["behind"].upstream.as_ref().map(|u| u.status),
        Some(UpstreamStatus::UpstreamIsAheadOfLocal)
    );
    assert_eq!(
        by_name["ahead"].upstream.as_ref().map(|u| u.status),
        Some(UpstreamStatus::LocalIsAheadOfUpstream)
    );
    assert_eq!(
        by_name["diverged"].upstream.as_ref().map(|u| u.status),
        Some(UpstreamStatus::MergeNeeded)
    );
    assert_eq!(
        by_name["gone"].upstream.as_ref().map(|u| u.status),
        Some(UpstreamStatus::UpstreamIsGone)
    );
    assert!(by_name["local-only"].upstream.is_none());
    Ok(())
}

#[test]
fn test_getting_branches() -> Result<()> {
    let repo = test_repo("repo-with-some-branches")?;
    let branches = repo.get_branches()?;
    let mut refnames: Vec<String> = branches.iter().map(|b| b.refname.clone()).collect();
    refnames.sort();

    assert_eq!(refnames, vec!["existing", "master"]);
    Ok(())
}

#[test]
fn pr_create_args_default_omits_optional_flags() {
    let args = pr_create_args("feature", "main", PrOptions::default());
    assert_eq!(
        args,
        vec!["pr", "create", "--head", "feature", "--base", "main"]
    );
}

#[test]
fn pr_create_args_with_draft_appends_draft_and_fill() {
    let args = pr_create_args(
        "feature",
        "main",
        PrOptions {
            draft: true,
            web: false,
        },
    );
    assert_eq!(
        args,
        vec![
            "pr", "create", "--head", "feature", "--base", "main", "--draft", "--fill",
        ]
    );
}

#[test]
fn pr_create_args_with_web_appends_only_web() {
    let args = pr_create_args(
        "feature",
        "main",
        PrOptions {
            draft: false,
            web: true,
        },
    );
    assert_eq!(
        args,
        vec![
            "pr", "create", "--head", "feature", "--base", "main", "--web"
        ]
    );
}

#[test]
fn pr_create_args_prefers_draft_when_both_are_set() {
    let args = pr_create_args(
        "feature",
        "main",
        PrOptions {
            draft: true,
            web: true,
        },
    );
    assert!(args.iter().any(|a| a == "--draft"));
    assert!(args.iter().any(|a| a == "--fill"));
    assert!(!args.iter().any(|a| a == "--web"));
}

/// Builds a repository with one commit on `initial_branch`, no remote and no
/// `origin/HEAD`, so each test can add exactly the state it is about to assert
/// on. Repository-local config shadows the developer's global config, which
/// keeps the `init.defaultBranch` tests hermetic.
fn repo_with_initial_branch(initial_branch: &str) -> Result<(tempfile::TempDir, GitRepo)> {
    let temp_dir = tempfile::tempdir()?;
    let path = temp_dir.path().to_path_buf();
    git(&path, &["init", "-q", "-b", initial_branch])?;
    git(&path, &["config", "commit.gpgsign", "false"])?;
    git(&path, &["config", "user.name", "Test"])?;
    git(&path, &["config", "user.email", "test@example.com"])?;
    std::fs::write(path.join("file.txt"), "hello")?;
    git(&path, &["add", "file.txt"])?;
    git(&path, &["commit", "-q", "-m", "Initial commit"])?;
    Ok((temp_dir, GitRepo::new(path)))
}

fn git(dir: &PathBuf, args: &[&str]) -> Result<()> {
    let status = Command::new("git").args(args).current_dir(dir).status()?;
    if !status.success() {
        return Err(anyhow::anyhow!("git {:?} failed", args));
    }
    Ok(())
}

#[test]
fn default_branch_for_checkout_prefers_origin_head() -> Result<()> {
    let (_temp, repo) = repo_with_initial_branch("main")?;
    let dir = repo.dir.clone();
    git(&dir, &["branch", "trunk"])?;
    // What `init.defaultBranch` says must lose against what this repository's
    // remote actually calls its default branch.
    git(&dir, &["config", "init.defaultBranch", "main"])?;
    git(
        &dir,
        &[
            "symbolic-ref",
            "refs/remotes/origin/HEAD",
            "refs/remotes/origin/trunk",
        ],
    )?;

    assert_eq!(repo.default_branch_for_checkout()?, "trunk");
    Ok(())
}

#[test]
fn default_branch_for_checkout_handles_slashes_in_origin_head() -> Result<()> {
    let (_temp, repo) = repo_with_initial_branch("main")?;
    let dir = repo.dir.clone();
    git(&dir, &["branch", "release/stable"])?;
    git(
        &dir,
        &[
            "symbolic-ref",
            "refs/remotes/origin/HEAD",
            "refs/remotes/origin/release/stable",
        ],
    )?;

    assert_eq!(repo.default_branch_for_checkout()?, "release/stable");
    Ok(())
}

#[test]
fn default_branch_for_checkout_falls_back_to_init_default_branch() -> Result<()> {
    let (_temp, repo) = repo_with_initial_branch("master")?;
    let dir = repo.dir.clone();
    git(&dir, &["branch", "custom-default"])?;
    git(&dir, &["config", "init.defaultBranch", "custom-default"])?;

    // No origin/HEAD: a repository made with `git init` never gets one.
    assert_eq!(repo.default_branch_for_checkout()?, "custom-default");
    Ok(())
}

#[test]
fn default_branch_for_checkout_skips_candidates_missing_locally() -> Result<()> {
    let (_temp, repo) = repo_with_initial_branch("fallback-branch")?;
    let dir = repo.dir.clone();
    // origin/HEAD can name a branch that was never fetched into this clone.
    git(
        &dir,
        &[
            "symbolic-ref",
            "refs/remotes/origin/HEAD",
            "refs/remotes/origin/never-fetched",
        ],
    )?;
    git(&dir, &["config", "init.defaultBranch", "fallback-branch"])?;

    assert_eq!(repo.default_branch_for_checkout()?, "fallback-branch");
    Ok(())
}

#[test]
fn default_branch_for_checkout_falls_back_to_main() -> Result<()> {
    let (_temp, repo) = repo_with_initial_branch("main")?;
    let dir = repo.dir.clone();
    git(&dir, &["config", "init.defaultBranch", "does-not-exist"])?;

    assert_eq!(repo.default_branch_for_checkout()?, "main");
    Ok(())
}

#[test]
fn default_branch_for_checkout_errors_when_no_candidate_exists() -> Result<()> {
    let (_temp, repo) = repo_with_initial_branch("only-branch")?;
    let dir = repo.dir.clone();
    git(&dir, &["config", "init.defaultBranch", "does-not-exist"])?;

    let error = repo
        .default_branch_for_checkout()
        .expect_err("no candidate branch exists locally");
    let message = error.to_string();
    assert!(message.contains("does-not-exist"), "{message}");
    assert!(message.contains("main"), "{message}");
    Ok(())
}

#[test]
fn checkout_default_branch_uses_the_branch_the_remote_defaults_to() -> Result<()> {
    // A real clone of a remote whose default branch is `trunk`, which is
    // exactly the case `init.defaultBranch` alone gets wrong.
    let (_upstream_temp, upstream) = repo_with_initial_branch("trunk")?;
    git(&upstream.dir, &["checkout", "-q", "--detach"])?;

    let clone_temp = tempfile::tempdir()?;
    let clone_path = clone_temp.path().join("clone");
    git(
        &upstream.dir,
        &[
            "clone",
            "-q",
            upstream.dir.to_str().unwrap(),
            clone_path.to_str().unwrap(),
        ],
    )?;
    let repo = GitRepo::new(clone_path.clone());
    git(&clone_path, &["config", "init.defaultBranch", "main"])?;
    git(&clone_path, &["checkout", "-q", "-b", "feature"])?;

    repo.checkout_default_branch()?;

    let head = Command::new("git")
        .args(["branch", "--show-current"])
        .current_dir(&clone_path)
        .output()?;
    assert_eq!(String::from_utf8(head.stdout)?.trim(), "trunk");
    Ok(())
}
