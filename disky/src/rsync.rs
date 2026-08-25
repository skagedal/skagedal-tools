//! Building and running rsync commands.
//!
//! There is no Rust crate that speaks rsync's remote protocol — the librsync
//! bindings only do the delta algorithm on local files — so this shells out to
//! the real rsync, which is also what the Hetzner Storage Box is happiest with.

use std::path::Path;
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};

use crate::config::{ResolvedPair, expand};

/// Finder and Spotlight droppings. Excluded both to keep transfers clean and
/// because they otherwise show up as endless phantom differences.
pub const EXCLUDES: &[&str] = &[
    ".DS_Store",
    ".Spotlight-V100",
    ".TemporaryItems",
    ".fseventsd",
    ".DocumentRevisions-V100",
];

/// `-a` minus `-o`/`-g`: we are not root, and the Storage Box maps everything
/// to the one account anyway, so preserving owner/group only adds noise.
/// Extended attributes (`-X`) are deliberately not set — see the README.
const ARCHIVE: &[&str] = &["-rlptD", "--human-readable"];

fn base_args(pair: &ResolvedPair) -> Vec<String> {
    let mut args: Vec<String> = ARCHIVE.iter().map(|s| s.to_string()).collect();
    for ex in EXCLUDES {
        args.push("--exclude".into());
        args.push((*ex).into());
    }
    if let Some(rsh) = ssh_transport(pair) {
        args.push("-e".into());
        args.push(rsh);
    }
    args
}

/// The `-e` argument, when the pair has a remote. Port and key live here rather
/// than in ssh config so the tool is self-contained.
pub fn ssh_transport(pair: &ResolvedPair) -> Option<String> {
    let remote = pair.remote.as_ref()?;
    let mut parts = vec!["ssh".to_string()];
    if let Some(port) = remote.port {
        parts.push("-p".into());
        parts.push(port.to_string());
    }
    if let Some(key) = &remote.identity_file {
        parts.push("-i".into());
        parts.push(expand(key).to_string_lossy().into_owned());
        // An explicit key means exactly that key; don't let the agent offer
        // others and trip the server's auth-attempt limit.
        parts.push("-o".into());
        parts.push("IdentitiesOnly=yes".into());
    }
    Some(parts.join(" "))
}

fn run(args: &[String], quiet: bool) -> Result<()> {
    let mut cmd = Command::new("rsync");
    cmd.args(args);
    if quiet {
        cmd.stdout(Stdio::null());
    }
    let status = cmd
        .status()
        .context("could not run rsync — is it installed?")?;
    if !status.success() {
        bail!("rsync failed with {status}");
    }
    Ok(())
}

struct Output {
    stdout: String,
    stderr: String,
    code: Option<i32>,
    ok: bool,
}

fn capture_raw(args: &[String]) -> Result<Output> {
    let out = Command::new("rsync")
        .args(args)
        .output()
        .context("could not run rsync — is it installed?")?;
    Ok(Output {
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        code: out.status.code(),
        ok: out.status.success(),
    })
}

fn capture(args: &[String]) -> Result<String> {
    let out = capture_raw(args)?;
    if !out.ok {
        bail!(
            "rsync failed (exit {}): {}",
            out.code.unwrap_or(-1),
            out.stderr.trim()
        );
    }
    Ok(out.stdout)
}

/// Did rsync fail merely because the directory isn't there?
///
/// This matters because "nothing has been offloaded yet" and "the remote is
/// unreachable" must not look alike — swallowing the latter would make `list`
/// quietly report every project as local-only. rsync exits 23 for a missing
/// path but 12 for a protocol or authentication failure, and the message
/// distinguishes them further.
fn is_missing_dir(code: Option<i32>, stderr: &str) -> bool {
    code == Some(23) && stderr.contains("No such file or directory")
}

/// Copy `src` to `dst`, mirroring deletions.
pub fn transfer(
    pair: &ResolvedPair,
    src: &str,
    dst: &str,
    dry_run: bool,
    delete: bool,
) -> Result<()> {
    let mut args = base_args(pair);
    args.extend(pair.rsync_extra.iter().cloned());
    if delete {
        args.push("--delete".into());
    }
    args.push("--mkpath".into());
    args.push("--partial".into());
    if std::io::IsTerminal::is_terminal(&std::io::stdout()) {
        args.push("--info=progress2".into());
    } else {
        args.push("--quiet".into());
    }
    if dry_run {
        args.push("--dry-run".into());
    }
    args.push(with_trailing_slash(src));
    args.push(with_trailing_slash(dst));
    run(&args, false)
}

/// Re-compare both sides by content hash and return every real difference.
///
/// This is what makes `offload --delete` safe: the ordinary transfer only
/// checks size and mtime, so a flipped bit on the remote that preserved both
/// would go unnoticed. `--checksum` re-reads and hashes both trees.
pub fn diff(pair: &ResolvedPair, src: &str, dst: &str) -> Result<Vec<String>> {
    let mut args = base_args(pair);
    args.extend(pair.rsync_extra.iter().cloned());
    args.push("--dry-run".into());
    args.push("--itemize-changes".into());
    args.push("--checksum".into());
    args.push("--delete".into());
    args.push(with_trailing_slash(src));
    args.push(with_trailing_slash(dst));
    Ok(significant_changes(&capture(&args)?))
}

/// Filter an `--itemize-changes` listing down to differences that matter.
///
/// A leading `.d` means an existing directory whose own metadata differs but
/// whose contents are itemized separately — in practice always an mtime that
/// moved because a file inside changed, and the Storage Box does not reliably
/// round-trip directory mtimes anyway. Everything else is a real difference,
/// including `*deleting` lines (files present remotely that are gone locally).
pub fn significant_changes(output: &str) -> Vec<String> {
    output
        .lines()
        .map(str::trim_end)
        .filter(|l| !l.is_empty())
        .filter(|l| !l.starts_with(".d"))
        .map(|l| l.to_string())
        .collect()
}

/// Top-level directory names on the far side of the pair.
pub fn list_projects(pair: &ResolvedPair) -> Result<Vec<String>> {
    let mut args: Vec<String> = vec!["--list-only".into()];
    for ex in EXCLUDES {
        args.push("--exclude".into());
        args.push((*ex).into());
    }
    if let Some(rsh) = ssh_transport(pair) {
        args.push("-e".into());
        args.push(rsh);
    }
    args.push(pair.remote_spec(""));

    // A missing remote root is not an error — it just means nothing has been
    // offloaded yet. Anything else (unreachable host, refused key) must
    // surface, or an empty listing would be indistinguishable from a failure.
    let out = capture_raw(&args)?;
    if !out.ok {
        if is_missing_dir(out.code, &out.stderr) {
            return Ok(Vec::new());
        }
        bail!(
            "could not list {} (rsync exit {}): {}",
            pair.remote_label(),
            out.code.unwrap_or(-1),
            out.stderr.trim()
        );
    }
    Ok(parse_listing(&out.stdout))
}

/// Pull directory names out of an `rsync --list-only` listing.
///
/// Lines look like `drwxr-xr-x  4.10K 2026/08/25 07:45:00 about a girl`. The
/// name starts one character after the timestamp, which is the only reliable
/// anchor: sizes vary in width and names contain spaces.
pub fn parse_listing(output: &str) -> Vec<String> {
    let re = regex::Regex::new(r"\d{2}:\d{2}:\d{2} ").expect("static regex");
    let mut names = Vec::new();
    for line in output.lines() {
        if !line.starts_with('d') {
            continue;
        }
        let Some(m) = re.find(line) else { continue };
        let name = &line[m.end()..];
        if name.is_empty() || name == "." {
            continue;
        }
        names.push(name.to_string());
    }
    names.sort();
    names
}

/// Local top-level directory names in the pair's root.
pub fn list_local(root: &Path) -> Result<Vec<String>> {
    if !root.exists() {
        return Ok(Vec::new());
    }
    let mut names = Vec::new();
    for entry in
        std::fs::read_dir(root).with_context(|| format!("could not read {}", root.display()))?
    {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            names.push(entry.file_name().to_string_lossy().into_owned());
        }
    }
    names.sort();
    Ok(names)
}

fn with_trailing_slash(s: &str) -> String {
    if s.ends_with('/') {
        s.to_string()
    } else {
        format!("{s}/")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::parse;

    fn studio() -> ResolvedPair {
        parse(
            r#"
[remotes.hetzner]
host = "u656759.your-storagebox.de"
user = "u656759"
port = 23
identity_file = "/keys/hetzner-key"

[pairs.studio]
remote = "hetzner"
local = "/local/studio"
path = "studio"
"#,
        )
        .unwrap()
        .resolve("studio")
        .unwrap()
    }

    #[test]
    fn ssh_transport_carries_port_and_key() {
        let t = ssh_transport(&studio()).unwrap();
        assert_eq!(t, "ssh -p 23 -i /keys/hetzner-key -o IdentitiesOnly=yes");
    }

    #[test]
    fn local_pair_has_no_ssh_transport() {
        let p = parse("[pairs.p]\nlocal = \"/a\"\npath = \"/b\"\n")
            .unwrap()
            .resolve("p")
            .unwrap();
        assert!(ssh_transport(&p).is_none());
    }

    #[test]
    fn base_args_exclude_junk_and_avoid_owner_group() {
        let args = base_args(&studio());
        assert!(args.contains(&"-rlptD".to_string()));
        assert!(!args.iter().any(|a| a == "-a"));
        assert!(args.contains(&".DS_Store".to_string()));
    }

    #[test]
    fn parses_a_listing_keeping_spaces_in_names() {
        let out = "\
drwxr-xr-x            160 2026/08/25 07:45:01 .
drwxr-xr-x            128 2026/08/25 07:45:01 about a girl
drwxr-xr-x            128 2026/08/24 09:12:33 next stop jörn
-rw-r--r--          4.10K 2026/08/25 07:45:01 loose-file.txt
";
        assert_eq!(
            parse_listing(out),
            vec!["about a girl".to_string(), "next stop jörn".to_string()]
        );
    }

    #[test]
    fn listing_ignores_files_and_the_root_entry() {
        let out = "drwxr-xr-x  160 2026/08/25 07:45:01 .\n";
        assert!(parse_listing(out).is_empty());
    }

    #[test]
    fn directory_mtime_noise_is_not_a_difference() {
        let out = "\
.d..t...... ./
.d..t...... Audio Files/
";
        assert!(significant_changes(out).is_empty());
    }

    #[test]
    fn content_differences_and_deletions_are_significant() {
        let out = "\
.d..t...... ./
>fc........ project.logicx
*deleting   stale.wav
cd+++++++++ new-dir/
";
        assert_eq!(
            significant_changes(out),
            vec![
                ">fc........ project.logicx".to_string(),
                "*deleting   stale.wav".to_string(),
                "cd+++++++++ new-dir/".to_string(),
            ]
        );
    }

    #[test]
    fn missing_directory_is_told_apart_from_a_broken_connection() {
        // rsync exits 23 for a path that isn't there...
        assert!(is_missing_dir(
            Some(23),
            "rsync: [sender] change_dir \"/studio\" failed: No such file or directory (2)"
        ));
        // ...but 12 when ssh auth fails, which must not be mistaken for empty.
        assert!(!is_missing_dir(
            Some(12),
            "Permission denied, please try again."
        ));
        assert!(!is_missing_dir(Some(255), "connection unexpectedly closed"));
        // A 23 for some other reason is not a missing directory either.
        assert!(!is_missing_dir(
            Some(23),
            "some files could not be transferred"
        ));
    }

    #[test]
    fn trailing_slash_is_added_once() {
        assert_eq!(with_trailing_slash("/a/b"), "/a/b/");
        assert_eq!(with_trailing_slash("/a/b/"), "/a/b/");
    }
}
