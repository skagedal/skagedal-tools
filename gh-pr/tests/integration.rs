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

fn mock_git_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_mock-git"))
}

struct Harness {
    tempdir: TempDir,
    path_value: String,
    log_path: PathBuf,
    git_log_path: PathBuf,
}

impl Harness {
    fn new() -> Self {
        let tempdir = TempDir::new().expect("create tempdir");
        let gh_path = tempdir.path().join("gh");
        fs::copy(mock_gh_bin(), &gh_path).expect("copy mock-gh as gh");
        let git_path = tempdir.path().join("git");
        fs::copy(mock_git_bin(), &git_path).expect("copy mock-git as git");

        let parent_path = std::env::var("PATH").unwrap_or_default();
        let path_value = format!("{}:{}", tempdir.path().display(), parent_path);
        let log_path = tempdir.path().join("invocations.log");
        let git_log_path = tempdir.path().join("git-invocations.log");

        Self {
            tempdir,
            path_value,
            log_path,
            git_log_path,
        }
    }

    fn run(&self, args: &[&str], extra_env: &[(&str, &str)]) -> Output {
        let mut cmd = Command::new(pr_bin());
        cmd.env("PATH", &self.path_value)
            .env("MOCK_GH_LOG", &self.log_path)
            .env("MOCK_GIT_LOG", &self.git_log_path)
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

    fn git_log(&self) -> String {
        fs::read_to_string(&self.git_log_path).unwrap_or_default()
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
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("gh pr view --json url --jq .url"),
        "stderr: {stderr}"
    );
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
    let git_log = h.git_log();
    assert!(
        git_log.contains("push -u origin HEAD"),
        "git log: {git_log}"
    );
}

#[test]
fn create_aborts_when_push_fails() {
    let h = Harness::new();
    let out = h.run(
        &["--create", "--toward", "main"],
        &[
            ("MOCK_GIT_FAIL", "1"),
            (
                "MOCK_GH_CREATE_URL",
                "https://example.invalid/should-not-be-used",
            ),
        ],
    );
    assert_eq!(out.status.code(), Some(1));
    let log = h.log();
    assert!(!log.contains("pr create"), "log: {log}");
}

#[test]
fn short_flags_work() {
    let h = Harness::new();
    let out = h.run(
        &["-c", "--toward", "main"],
        &[("MOCK_GH_CREATE_URL", "https://github.com/me/repo/pull/55")],
    );
    assert_success(&out);
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).trim(),
        "https://github.com/me/repo/pull/55"
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
            (
                "MOCK_GH_CREATE_URL",
                "https://example.invalid/should-not-be-used",
            ),
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

const SAMPLE_PR_COMMENTS_JSON: &str = r#"{
  "data": {
    "repository": {
      "pullRequest": {
        "comments": {
          "nodes": [
            {"databaseId": 1, "author": {"login": "alice"}, "createdAt": "2026-05-01T14:23:00Z", "body": "LGTM"}
          ]
        },
        "reviews": {
          "nodes": [
            {"databaseId": 500, "author": {"login": "dave"}, "submittedAt": "2026-05-02T09:30:00Z", "state": "CHANGES_REQUESTED", "body": "please fix the proto annotation"},
            {"databaseId": 501, "author": {"login": "bob"}, "submittedAt": "2026-05-02T10:00:00Z", "state": "COMMENTED", "body": ""}
          ]
        },
        "reviewThreads": {
          "nodes": [
            {
              "isResolved": false,
              "isOutdated": false,
              "path": "src/foo.rs",
              "line": 42,
              "originalLine": 42,
              "comments": {"nodes": [
                {"databaseId": 100, "author": {"login": "bob"}, "createdAt": "2026-05-02T10:00:00Z", "body": "nit: rename this"}
              ]}
            },
            {
              "isResolved": true,
              "isOutdated": false,
              "path": "src/bar.rs",
              "line": 7,
              "originalLine": 7,
              "comments": {"nodes": [
                {"databaseId": 200, "author": {"login": "carol"}, "createdAt": "2026-05-02T11:00:00Z", "body": "old concern"}
              ]}
            }
          ]
        }
      }
    }
  }
}"#;

#[test]
fn comments_text_format_calls_graphql_and_hides_resolved() {
    let h = Harness::new();
    let out = h.run(
        &["comments"],
        &[
            ("MOCK_GH_PR_NUMBER", "42"),
            ("MOCK_GH_NAME_WITH_OWNER", "me/repo"),
            ("MOCK_GH_PR_COMMENTS_JSON", SAMPLE_PR_COMMENTS_JSON),
        ],
    );
    assert_success(&out);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("Conversation comments (1)"),
        "stdout: {stdout}"
    );
    assert!(stdout.contains("@alice"), "stdout: {stdout}");
    assert!(stdout.contains("LGTM"), "stdout: {stdout}");
    assert!(stdout.contains("Review summaries (1)"), "stdout: {stdout}");
    assert!(
        stdout.contains("@dave (CHANGES_REQUESTED)"),
        "stdout: {stdout}"
    );
    assert!(
        stdout.contains("please fix the proto annotation"),
        "stdout: {stdout}"
    );
    assert!(
        stdout.contains("Review threads (1, 1 resolved hidden)"),
        "stdout: {stdout}"
    );
    assert!(stdout.contains("src/foo.rs:42"), "stdout: {stdout}");
    assert!(stdout.contains("@bob"), "stdout: {stdout}");
    assert!(!stdout.contains("src/bar.rs"), "stdout: {stdout}");
    assert!(!stdout.contains("@carol"), "stdout: {stdout}");

    let log = h.log();
    assert!(log.contains("api graphql"), "log: {log}");
    assert!(log.contains("-F number=42"), "log: {log}");
    assert!(log.contains("-f owner=me"), "log: {log}");
    assert!(log.contains("-f repo=repo"), "log: {log}");
}

#[test]
fn comments_resolved_flag_shows_resolved_threads() {
    let h = Harness::new();
    let out = h.run(
        &["comments", "--resolved"],
        &[
            ("MOCK_GH_PR_NUMBER", "42"),
            ("MOCK_GH_NAME_WITH_OWNER", "me/repo"),
            ("MOCK_GH_PR_COMMENTS_JSON", SAMPLE_PR_COMMENTS_JSON),
        ],
    );
    assert_success(&out);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("Review threads (2)"), "stdout: {stdout}");
    assert!(
        stdout.contains("src/bar.rs:7 [RESOLVED]"),
        "stdout: {stdout}"
    );
    assert!(stdout.contains("@carol"), "stdout: {stdout}");
}

#[test]
fn comments_json_format_emits_full_raw_response() {
    let h = Harness::new();
    let out = h.run(
        &["comments", "--format", "json"],
        &[
            ("MOCK_GH_PR_NUMBER", "42"),
            ("MOCK_GH_NAME_WITH_OWNER", "me/repo"),
            ("MOCK_GH_PR_COMMENTS_JSON", SAMPLE_PR_COMMENTS_JSON),
        ],
    );
    assert_success(&out);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let parsed: serde_json::Value =
        serde_json::from_str(&stdout).unwrap_or_else(|e| panic!("invalid JSON: {e}\n{stdout}"));
    // Both the resolved and unresolved threads must be present — JSON is
    // unfiltered regardless of --resolved.
    let threads = parsed
        .pointer("/data/repository/pullRequest/reviewThreads/nodes")
        .and_then(serde_json::Value::as_array)
        .expect("review threads array");
    assert_eq!(threads.len(), 2);
    let convo = parsed
        .pointer("/data/repository/pullRequest/comments/nodes")
        .and_then(serde_json::Value::as_array)
        .expect("conversation comments array");
    assert_eq!(convo.len(), 1);
    let reviews = parsed
        .pointer("/data/repository/pullRequest/reviews/nodes")
        .and_then(serde_json::Value::as_array)
        .expect("reviews array");
    assert_eq!(reviews.len(), 2);
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
fn mark_viewed_with_literal_filenames_marks_each() {
    let h = Harness::new();
    let out = h.run(
        &["mark-viewed", "src/foo.rs", "db/generated/schema.sql"],
        &[
            ("MOCK_GH_PR_NUMBER", "42"),
            ("MOCK_GH_PR_NODE_ID", "PR_abc"),
        ],
    );
    assert_success(&out);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("Viewed: src/foo.rs"), "stdout: {stdout}");
    assert!(
        stdout.contains("Viewed: db/generated/schema.sql"),
        "stdout: {stdout}"
    );

    let log = h.log();
    // One mutation per file, each carrying the node id and the path. The
    // file list is NOT fetched in literal mode.
    assert_eq!(log.matches("markFileAsViewed").count(), 2, "log: {log}");
    assert!(log.contains("-f pullRequestId=PR_abc"), "log: {log}");
    assert!(log.contains("-f path=src/foo.rs"), "log: {log}");
    assert!(
        log.contains("-f path=db/generated/schema.sql"),
        "log: {log}"
    );
    assert!(!log.contains("/files"), "should not list files: {log}");
}

const SAMPLE_PR_FILES_JSON: &str = r#"[
  {"filename": "src/foo.rs"},
  {"filename": "db/generated/schema.sql"},
  {"filename": "db/generated/seed.sql"},
  {"filename": "README.md"}
]"#;

#[test]
fn mark_viewed_regex_marks_only_matching_files() {
    let h = Harness::new();
    let out = h.run(
        &["mark-viewed", "--regex", "db/generated/"],
        &[
            ("MOCK_GH_PR_NUMBER", "42"),
            ("MOCK_GH_PR_NODE_ID", "PR_abc"),
            ("MOCK_GH_NAME_WITH_OWNER", "me/repo"),
            ("MOCK_GH_PR_FILES_JSON", SAMPLE_PR_FILES_JSON),
        ],
    );
    assert_success(&out);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("Viewed: db/generated/schema.sql"),
        "stdout: {stdout}"
    );
    assert!(
        stdout.contains("Viewed: db/generated/seed.sql"),
        "stdout: {stdout}"
    );
    assert!(!stdout.contains("src/foo.rs"), "stdout: {stdout}");
    assert!(!stdout.contains("README.md"), "stdout: {stdout}");

    let log = h.log();
    assert!(
        log.contains("api --paginate repos/me/repo/pulls/42/files"),
        "log: {log}"
    );
    assert!(
        log.contains("-f path=db/generated/schema.sql"),
        "log: {log}"
    );
    assert!(!log.contains("-f path=src/foo.rs"), "log: {log}");
}

#[test]
fn mark_viewed_regex_exits_1_when_nothing_matches() {
    let h = Harness::new();
    let out = h.run(
        &["mark-viewed", "--regex", "no/such/path"],
        &[
            ("MOCK_GH_PR_NUMBER", "42"),
            ("MOCK_GH_PR_NODE_ID", "PR_abc"),
            ("MOCK_GH_NAME_WITH_OWNER", "me/repo"),
            ("MOCK_GH_PR_FILES_JSON", SAMPLE_PR_FILES_JSON),
        ],
    );
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("matched"), "stderr: {stderr}");
}

#[test]
fn mark_viewed_exits_1_when_no_pr() {
    let h = Harness::new();
    let out = h.run(&["mark-viewed", "src/foo.rs"], &[]);
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
