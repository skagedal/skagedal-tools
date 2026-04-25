use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};

use tempfile::TempDir;

fn pr_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_gh-pr"))
}

fn mock_gh_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_mock-gh"))
}

struct Harness {
    tempdir: TempDir,
    path_value: String,
    log_path: PathBuf,
}

impl Harness {
    fn new() -> Self {
        let tempdir = TempDir::new().expect("create tempdir");
        let gh_path = tempdir.path().join("gh");
        fs::copy(mock_gh_bin(), &gh_path).expect("copy mock-gh as gh");

        let parent_path = std::env::var("PATH").unwrap_or_default();
        let path_value = format!("{}:{}", tempdir.path().display(), parent_path);
        let log_path = tempdir.path().join("invocations.log");

        Self { tempdir, path_value, log_path }
    }

    fn run(&self, args: &[&str], extra_env: &[(&str, &str)]) -> Output {
        let mut cmd = Command::new(pr_bin());
        cmd.env("PATH", &self.path_value)
            .env("MOCK_GH_LOG", &self.log_path)
            .current_dir(self.tempdir.path())
            .args(args);
        for (k, v) in extra_env {
            cmd.env(k, v);
        }
        cmd.output().expect("spawn pr")
    }

    fn log(&self) -> String {
        fs::read_to_string(&self.log_path).unwrap_or_default()
    }
}

fn assert_success(out: &Output) {
    assert!(
        out.status.success(),
        "expected success, got {:?}\nstdout: {}\nstderr: {}",
        out.status.code(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
}

#[test]
fn default_prints_pr_url_when_pr_exists() {
    let h = Harness::new();
    let out = h.run(
        &[],
        &[("MOCK_GH_PR_URL", "https://github.com/me/repo/pull/42")],
    );
    assert_success(&out);
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).trim(),
        "https://github.com/me/repo/pull/42"
    );
    assert!(out.stderr.is_empty(), "stderr: {:?}", out.stderr);
}

#[test]
fn default_exits_1_with_stderr_when_no_pr() {
    let h = Harness::new();
    let out = h.run(&[], &[]);
    assert_eq!(out.status.code(), Some(1));
    assert!(out.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("No pull request"),
        "unexpected stderr: {stderr}"
    );
}

#[test]
fn create_creates_pr_using_default_branch() {
    let h = Harness::new();
    let out = h.run(
        &["--create"],
        &[
            ("MOCK_GH_DEFAULT_BRANCH", "trunk"),
            ("MOCK_GH_CREATE_URL", "https://github.com/me/repo/pull/100"),
        ],
    );
    assert_success(&out);
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).trim(),
        "https://github.com/me/repo/pull/100"
    );
    let log = h.log();
    assert!(
        log.contains("pr create --draft --fill --base trunk"),
        "log: {log}"
    );
}

#[test]
fn create_with_toward_uses_specified_base_and_skips_default_lookup() {
    let h = Harness::new();
    let out = h.run(
        &["--create", "--toward", "develop"],
        &[("MOCK_GH_CREATE_URL", "https://github.com/me/repo/pull/7")],
    );
    assert_success(&out);
    let log = h.log();
    assert!(log.contains("--base develop"), "log: {log}");
    assert!(!log.contains("repo view"), "log: {log}");
}

#[test]
fn create_falls_back_to_main_when_no_default_branch() {
    let h = Harness::new();
    let out = h.run(
        &["--create"],
        &[("MOCK_GH_CREATE_URL", "https://github.com/me/repo/pull/3")],
    );
    assert_success(&out);
    let log = h.log();
    assert!(log.contains("--base main"), "log: {log}");
    assert!(log.contains("repo view"), "log: {log}");
}

#[test]
fn create_skipped_when_pr_already_exists() {
    let h = Harness::new();
    let out = h.run(
        &["--create"],
        &[
            ("MOCK_GH_PR_URL", "https://github.com/me/repo/pull/9"),
            ("MOCK_GH_CREATE_URL", "https://example.invalid/should-not-be-used"),
        ],
    );
    assert_success(&out);
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).trim(),
        "https://github.com/me/repo/pull/9"
    );
    let log = h.log();
    assert!(!log.contains("pr create"), "log: {log}");
}

#[test]
fn comments_prints_api_response_for_correct_path() {
    let h = Harness::new();
    let out = h.run(
        &["comments"],
        &[
            ("MOCK_GH_PR_NUMBER", "42"),
            ("MOCK_GH_COMMENTS_JSON", r#"[{"id":1,"body":"hi"}]"#),
        ],
    );
    assert_success(&out);
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).trim(),
        r#"[{"id":1,"body":"hi"}]"#
    );
    let log = h.log();
    assert!(
        log.contains("api repos/{owner}/{repo}/issues/42/comments"),
        "log: {log}"
    );
}

#[test]
fn comments_exits_1_when_no_pr() {
    let h = Harness::new();
    let out = h.run(&["comments"], &[]);
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("No pull request"),
        "unexpected stderr: {stderr}"
    );
}

#[test]
fn mark_ready_invokes_gh_pr_ready() {
    let h = Harness::new();
    let out = h.run(&["mark-ready"], &[]);
    assert_success(&out);
    let log = h.log();
    assert_eq!(log.trim(), "pr ready");
}
