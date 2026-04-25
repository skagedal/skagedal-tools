//! Mock implementation of `git` for the `gh-pr` integration tests. Logs each
//! invocation's argv to `MOCK_GIT_LOG` and exits non-zero if `MOCK_GIT_FAIL`
//! is set; otherwise exits 0 with no output.

use std::env;
use std::fs::OpenOptions;
use std::io::Write;
use std::process::exit;

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    if let Ok(path) = env::var("MOCK_GIT_LOG")
        && let Ok(mut f) = OpenOptions::new().create(true).append(true).open(&path)
    {
        let _ = writeln!(f, "{}", args.join(" "));
    }
    if env::var("MOCK_GIT_FAIL").is_ok() {
        eprintln!("mock-git: configured to fail");
        exit(1);
    }
}
