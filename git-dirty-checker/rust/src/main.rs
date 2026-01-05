use rayon::prelude::*;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();

    if args.is_empty() {
        eprintln!("Usage: git-dirty-checker <directory> [<directory> ...]");
        eprintln!("\nFinds git repositories with uncommitted changes in subdirectories.");
        std::process::exit(1);
    }

    let repositories: Vec<PathBuf> = args
        .par_iter()
        .flat_map(|dir| find_subdirectories(dir))
        .collect();

    let dirty_repos: Vec<PathBuf> = repositories
        .par_iter()
        .filter(|repo| is_dirty_repository(repo))
        .cloned()
        .collect();

    for repo in dirty_repos {
        if let Ok(canonical) = fs::canonicalize(&repo) {
            println!("{}", canonical.display());
        }
    }
}

fn find_subdirectories(path: &str) -> Vec<PathBuf> {
    let dir_path = Path::new(path);

    if !dir_path.is_dir() {
        return Vec::new();
    }

    match fs::read_dir(dir_path) {
        Ok(entries) => entries
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.path().is_dir())
            .map(|entry| entry.path())
            .collect(),
        Err(_) => Vec::new(),
    }
}

fn is_dirty_repository(path: &Path) -> bool {
    let output = Command::new("git")
        .arg("-C")
        .arg(path)
        .arg("status")
        .arg("--porcelain")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output();

    match output {
        Ok(output) => !output.stdout.is_empty(),
        Err(_) => false,
    }
}
