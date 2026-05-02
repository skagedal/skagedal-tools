use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{Context, Result, anyhow};
use serde::Deserialize;

#[derive(Debug, Clone)]
pub struct Branch {
    pub refname: String,
    pub upstream: Option<Upstream>,
    pub worktree_path: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct BranchCommitInfo {
    pub commit_timestamp: i64,
    pub commit_date: String,
    pub committer: String,
}

impl Branch {
    pub fn needs_action(&self) -> bool {
        self.upstream
            .as_ref()
            .map(|upstream| upstream.status != UpstreamStatus::Identical)
            .unwrap_or(true)
    }
}

#[derive(Debug, Clone)]
pub struct Upstream {
    pub name: String,
    pub status: UpstreamStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpstreamStatus {
    Identical,
    UpstreamIsAheadOfLocal,
    LocalIsAheadOfUpstream,
    MergeNeeded,
    UpstreamIsGone,
}

pub struct GitRepo {
    dir: PathBuf,
}

impl GitRepo {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn get_branches(&self) -> Result<Vec<Branch>> {
        let output = self.run_and_capture(
            "git",
            &[
                "for-each-ref",
                "--format=%(refname:short)|%(upstream:short)|%(upstream:track)|%(worktreepath)",
                "refs/heads/",
            ],
        )?;
        parse_branches(&output)
    }

    pub fn branch_commit_infos(
        &self,
    ) -> Result<std::collections::HashMap<String, BranchCommitInfo>> {
        let output = self.run_and_capture(
            "git",
            &[
                "for-each-ref",
                "--format=%(refname:short)|%(committerdate:unix)|%(committerdate:short)|%(committername)",
                "refs/heads/",
            ],
        )?;

        let mut map = std::collections::HashMap::new();
        for line in output.lines().filter(|line| !line.trim().is_empty()) {
            let parts: Vec<&str> = line.splitn(4, '|').collect();
            if parts.len() != 4 {
                return Err(anyhow!("unexpected output from git for-each-ref: {line}"));
            }
            let timestamp: i64 = parts[1]
                .parse()
                .with_context(|| format!("failed to parse committer timestamp: {}", parts[1]))?;
            map.insert(
                parts[0].to_string(),
                BranchCommitInfo {
                    commit_timestamp: timestamp,
                    commit_date: parts[2].to_string(),
                    committer: parts[3].to_string(),
                },
            );
        }
        Ok(map)
    }

    pub fn push(&self, refname: &str) -> Result<()> {
        self.run_interactive_printing("git", &["push", "origin", refname])
    }

    pub fn push_creating_origin(&self, refname: &str) -> Result<()> {
        self.run_interactive_printing("git", &["push", "--set-upstream", "origin", refname])
    }

    pub fn rebase(&self, refname: &str, upstream: &str) -> Result<()> {
        self.run_interactive_printing("git", &["rebase", upstream, refname])
    }

    pub fn delete_branch_forcefully(&self, branch: &str) -> Result<()> {
        self.run_interactive_printing("git", &["branch", "-D", branch])
    }

    pub fn delete_worktree(&self, path: &Path) -> Result<()> {
        let path_string = path.to_string_lossy().to_string();
        let args = ["worktree", "remove", path_string.as_str()];
        self.run_interactive_printing("git", &args)
    }

    pub fn create_pull_request(&self, refname: &str) -> Result<()> {
        let default_branch = self.default_branch()?;
        self.run_interactive_printing(
            "gh",
            &["pr", "create", "--head", refname, "--base", &default_branch],
        )
    }

    pub fn show_log(&self, branch: &str) -> Result<()> {
        self.run_interactive("tig", &[branch])
    }

    pub fn checkout_branch(&self, branch: &str) -> Result<()> {
        self.run_and_capture("git", &["checkout", branch])?;
        Ok(())
    }

    pub fn checkout_default_branch(&self) -> Result<()> {
        let branch = self.init_default_branch()?;
        self.checkout_branch(&branch)
            .with_context(|| format!("failed to checkout default branch '{}'", branch))
    }

    pub fn is_dirty(&self) -> Result<bool> {
        let output = self.run_and_capture("git", &["status", "--porcelain"])?;
        Ok(!output.trim().is_empty())
    }

    fn default_branch(&self) -> Result<String> {
        let output = self.run_and_capture("gh", &["repo", "view", "--json", "defaultBranchRef"])?;
        let response: DefaultBranchResponse =
            serde_json::from_str(&output).context("failed to parse gh repo view output")?;
        Ok(response.default_branch_ref.name.trim().to_string())
    }

    fn init_default_branch(&self) -> Result<String> {
        match self.run_and_capture("git", &["config", "--get", "init.defaultBranch"]) {
            Ok(branch) => {
                let branch = branch.trim();
                if !branch.is_empty() {
                    return Ok(branch.to_string());
                }
                Ok("main".to_string())
            }
            Err(_) => Ok("main".to_string()),
        }
    }

    fn run_and_capture(&self, program: &str, args: &[&str]) -> Result<String> {
        let output = self
            .command(program)
            .args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .with_context(|| format!("failed to run {}", format_command(program, args)))?;

        if output.status.success() {
            Ok(String::from_utf8(output.stdout).context("command output was not valid UTF-8")?)
        } else {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(anyhow!(
                "{} failed (stdout: {}, stderr: {})",
                format_command(program, args),
                stdout.trim(),
                stderr.trim()
            ))
        }
    }

    fn run_interactive(&self, program: &str, args: &[&str]) -> Result<()> {
        let status = self
            .command(program)
            .args(args)
            .status()
            .with_context(|| format!("failed to run {}", format_command(program, args)))?;

        if status.success() {
            Ok(())
        } else {
            Err(anyhow!(
                "{} exited with status {}",
                format_command(program, args),
                status
            ))
        }
    }

    fn run_interactive_printing(&self, program: &str, args: &[&str]) -> Result<()> {
        println!("{}", format_command(program, args));
        self.run_interactive(program, args)
    }

    fn command(&self, program: &str) -> Command {
        let mut command = Command::new(program);
        command.current_dir(&self.dir);
        command
    }
}

#[derive(Deserialize)]
struct DefaultBranchResponse {
    #[serde(rename = "defaultBranchRef")]
    default_branch_ref: DefaultBranchRef,
}

#[derive(Deserialize)]
struct DefaultBranchRef {
    name: String,
}

fn format_command(program: &str, args: &[&str]) -> String {
    let parts: Vec<&str> = std::iter::once(program)
        .chain(args.iter().copied())
        .collect();
    parts.join(" ")
}

fn parse_branches(output: &str) -> Result<Vec<Branch>> {
    let mut branches = Vec::new();
    for line in output.lines().filter(|line| !line.trim().is_empty()) {
        let parts: Vec<&str> = line.splitn(4, '|').collect();
        if parts.len() != 4 {
            return Err(anyhow!("unexpected output from git for-each-ref: {line}"));
        }
        let refname = parts[0].trim().to_string();
        let upstream_name = parts[1].trim();
        let track = parts[2].trim();
        let worktree_path = match parts[3].trim() {
            "" => None,
            path => Some(PathBuf::from(path)),
        };
        let upstream = if upstream_name.is_empty() {
            None
        } else {
            Some(Upstream {
                name: upstream_name.to_string(),
                status: parse_upstream_track(track),
            })
        };
        branches.push(Branch {
            refname,
            upstream,
            worktree_path,
        });
    }
    Ok(branches)
}

fn parse_upstream_track(track: &str) -> UpstreamStatus {
    if track.is_empty() {
        return UpstreamStatus::Identical;
    }
    let inner = track
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
        .unwrap_or(track);
    if inner == "gone" {
        return UpstreamStatus::UpstreamIsGone;
    }
    let has_ahead = inner.split(',').any(|part| part.trim().starts_with("ahead"));
    let has_behind = inner.split(',').any(|part| part.trim().starts_with("behind"));
    match (has_ahead, has_behind) {
        (true, true) => UpstreamStatus::MergeNeeded,
        (true, false) => UpstreamStatus::LocalIsAheadOfUpstream,
        (false, true) => UpstreamStatus::UpstreamIsAheadOfLocal,
        (false, false) => UpstreamStatus::Identical,
    }
}

#[cfg(test)]
mod tests;
