//! How much space is used and free on the far side of a pair.
//!
//! Nothing in the rsync protocol reports the size of a remote filesystem, but
//! the Hetzner Storage Box's restricted shell does run `df`, and for a pair
//! without a remote the very same command answers for the local disk. Output is
//! read in POSIX mode (`-Pk`): fixed columns, 1024-byte blocks.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{Context, Result, anyhow, bail};

use crate::config::{Remote, ResolvedPair, expand};
use crate::rsync;

/// What `df -k` counts in.
const BLOCK: u64 = 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Usage {
    pub total: u64,
    pub used: u64,
    pub avail: u64,
}

impl Usage {
    /// The share of the *usable* space that is taken — what `df` calls
    /// Capacity. Filesystems reserve blocks that count as neither used nor
    /// available, so `used / total` would read lower here than in every other
    /// tool on the machine.
    pub fn percent_full(&self) -> u32 {
        let usable = self.used + self.avail;
        if usable == 0 {
            return 0;
        }
        (self.used as f64 * 100.0 / usable as f64).round() as u32
    }

    /// One line, the way `disky` reports it.
    pub fn summary(&self) -> String {
        format!(
            "{} used, {} free of {} ({}% full)",
            format_bytes(self.used),
            format_bytes(self.avail),
            format_bytes(self.total),
            self.percent_full()
        )
    }
}

/// A `Usage` plus, when the numbers had to come from somewhere other than the
/// directory asked about, what that somewhere was.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reading {
    pub usage: Usage,
    pub measured: Option<String>,
}

impl Reading {
    pub fn summary(&self) -> String {
        match &self.measured {
            Some(m) => format!("{} (measured at {m})", self.usage.summary()),
            None => self.usage.summary(),
        }
    }
}

/// Space on the disk holding the far side of `pair`.
pub fn usage(pair: &ResolvedPair) -> Result<Reading> {
    match &pair.remote {
        Some(remote) => remote_usage(pair, remote),
        None => local_usage(&expand(&pair.remote_path)),
    }
}

fn local_usage(path: &Path) -> Result<Reading> {
    // The directory itself need not exist — nothing has to have been offloaded
    // yet — so fall back to the nearest ancestor that does. That is usually the
    // same disk, but not when an external drive simply isn't mounted: then the
    // walk lands on `/Volumes`, which is the internal disk. Reporting the
    // machine's own free space as the backup drive's would be a lie of exactly
    // the kind this tool exists to avoid, so say where the numbers came from.
    let target = nearest_existing(path)
        .ok_or_else(|| anyhow!("nothing exists at or above {}", path.display()))?;
    let out = run(Command::new("df").arg("-Pk").arg(&target))?;
    let usage = parse_df(&out)
        .ok_or_else(|| anyhow!("could not make sense of df output for {}", target.display()))?;
    Ok(Reading {
        usage,
        measured: (target != path).then(|| target.display().to_string()),
    })
}

fn remote_usage(pair: &ResolvedPair, remote: &Remote) -> Result<Reading> {
    let target = match &remote.user {
        Some(u) => format!("{}@{}", u, remote.host),
        None => remote.host.clone(),
    };
    let path = if pair.remote_path.is_empty() {
        "."
    } else {
        &pair.remote_path
    };

    match df_over_ssh(pair, &target, path) {
        Ok(usage) => Ok(Reading {
            usage,
            measured: None,
        }),
        // Before the first offload the remote directory isn't there yet. The
        // home directory it will be created in is, and on a Storage Box that is
        // the same disk — but say so rather than implying we saw the directory.
        Err(e) if path != "." => df_over_ssh(pair, &target, ".")
            .map(|usage| Reading {
                usage,
                measured: Some("the remote home directory".into()),
            })
            .map_err(|_| e),
        Err(e) => Err(e),
    }
}

/// What to run to ask a remote about its disk. Everything after the destination
/// is handed to a shell on the far side, so the path travels quoted.
fn df_argv(pair: &ResolvedPair, target: &str, path: &str) -> Option<Vec<String>> {
    let mut argv = rsync::ssh_command(pair)?;
    argv.push(target.to_string());
    argv.push(format!("df -Pk {}", shell_quote(path)));
    Some(argv)
}

fn df_over_ssh(pair: &ResolvedPair, target: &str, path: &str) -> Result<Usage> {
    let argv = df_argv(pair, target, path).ok_or_else(|| anyhow!("pair has no remote"))?;
    let (program, args) = argv.split_first().expect("ssh_command names a program");

    let out = run(Command::new(program).args(args))?;
    parse_df(&out).ok_or_else(|| anyhow!("unexpected df output from {target}: {}", out.trim()))
}

fn run(cmd: &mut Command) -> Result<String> {
    let program = cmd.get_program().to_string_lossy().into_owned();
    let out = cmd
        .stdin(Stdio::null())
        .output()
        .with_context(|| format!("could not run {program}"))?;
    if !out.status.success() {
        bail!(
            "{program} failed (exit {}): {}",
            out.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// The nearest ancestor of `path` — possibly `path` itself — that exists.
fn nearest_existing(path: &Path) -> Option<PathBuf> {
    let mut p = path;
    loop {
        if p.exists() {
            return Some(p.to_path_buf());
        }
        p = p.parent()?;
    }
}

/// A path as one word for the remote shell.
fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

/// Pull total/used/available out of `df -Pk` output.
///
/// Reading fixed columns is not safe: filesystem names can contain spaces
/// (`map auto_home` on macOS), so can mount points, and a long device name
/// makes some `df`s wrap the row onto two lines. So the header goes, and the
/// first three consecutive integers in what remains are taken — those are
/// always blocks, used and available, in that order.
pub fn parse_df(output: &str) -> Option<Usage> {
    let mut run: Vec<u64> = Vec::new();
    for line in output.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("Filesystem") {
            continue;
        }
        for token in line.split_whitespace() {
            match token.parse::<u64>() {
                Ok(n) => run.push(n),
                Err(_) => run.clear(),
            }
            if run.len() == 3 {
                return Some(Usage {
                    total: run[0].saturating_mul(BLOCK),
                    used: run[1].saturating_mul(BLOCK),
                    avail: run[2].saturating_mul(BLOCK),
                });
            }
        }
    }
    None
}

pub fn format_bytes(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "K", "M", "G", "T"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{}B", bytes)
    } else {
        format!("{:.1}{}", value, UNITS[unit])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MACOS: &str = "\
Filesystem  1024-blocks       Used Available Capacity  Mounted on
/dev/disk3s5 1942700360 1211230744 706429616    64%    /System/Volumes/Data
";

    #[test]
    fn formats_sizes_like_du() {
        assert_eq!(format_bytes(0), "0B");
        assert_eq!(format_bytes(512), "512B");
        assert_eq!(format_bytes(1024), "1.0K");
        assert_eq!(format_bytes(1536), "1.5K");
        assert_eq!(format_bytes(3_650_722_201), "3.4G");
    }

    #[test]
    fn reads_blocks_used_and_available() {
        let u = parse_df(MACOS).unwrap();
        assert_eq!(u.total, 1_942_700_360 * 1024);
        assert_eq!(u.used, 1_211_230_744 * 1024);
        assert_eq!(u.avail, 706_429_616 * 1024);
        assert_eq!(u.summary(), "1.1T used, 673.7G free of 1.8T (63% full)");
    }

    #[test]
    fn reads_a_row_wrapped_onto_two_lines() {
        // Some df builds break the line after a long device name.
        let out = "\
Filesystem           1K-blocks      Used Available Use% Mounted on
/dev/mapper/storagebox--vg-data
                       41152736   6975320  32064840  18% /home
";
        let u = parse_df(out).unwrap();
        assert_eq!(u.used, 6_975_320 * 1024);
        assert_eq!(u.avail, 32_064_840 * 1024);
    }

    #[test]
    fn spaces_in_filesystem_and_mount_names_do_not_shift_the_columns() {
        let out = "\
Filesystem  1024-blocks Used Available Capacity  Mounted on
map auto_home         0    0         0   100%    /Volumes/My Disk 2
";
        let u = parse_df(out).unwrap();
        assert_eq!(
            u,
            Usage {
                total: 0,
                used: 0,
                avail: 0
            }
        );
        // Nothing usable: report 0 rather than dividing by zero.
        assert_eq!(u.percent_full(), 0);
    }

    #[test]
    fn output_without_numbers_is_not_a_reading() {
        assert!(parse_df("").is_none());
        assert!(parse_df("df: /nope: No such file or directory\n").is_none());
        assert!(parse_df("Filesystem 1024-blocks Used Available Capacity Mounted on\n").is_none());
    }

    #[test]
    fn a_reading_says_so_when_it_is_not_the_directory_asked_about() {
        let usage = Usage {
            total: 1024,
            used: 512,
            avail: 512,
        };
        let exact = Reading {
            usage,
            measured: None,
        };
        assert_eq!(exact.summary(), "512B used, 512B free of 1.0K (50% full)");

        // An unmounted backup drive must not pass the internal disk's numbers
        // off as its own.
        let approximate = Reading {
            usage,
            measured: Some("/Volumes".into()),
        };
        assert_eq!(
            approximate.summary(),
            "512B used, 512B free of 1.0K (50% full) (measured at /Volumes)"
        );
    }

    #[test]
    fn local_usage_notes_the_ancestor_it_fell_back_to() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("backup");
        std::fs::create_dir_all(&root).unwrap();

        assert_eq!(local_usage(&root).unwrap().measured, None);
        assert_eq!(
            local_usage(&root.join("studio")).unwrap().measured,
            Some(root.display().to_string())
        );
    }

    #[test]
    fn capacity_ignores_reserved_blocks_the_way_df_does() {
        // 5% of this disk is reserved: used + avail is less than total.
        let u = Usage {
            total: 100,
            used: 45,
            avail: 45,
        };
        assert_eq!(u.percent_full(), 50);
    }

    #[test]
    fn remote_paths_survive_quoting() {
        assert_eq!(shell_quote("studio"), "'studio'");
        assert_eq!(shell_quote("my studio"), "'my studio'");
        assert_eq!(shell_quote("simon's"), r"'simon'\''s'");
    }

    #[test]
    fn asks_the_remote_over_the_pair_s_own_ssh_settings() {
        let pair = crate::config::parse(
            r#"
[remotes.hetzner]
host = "u656759.your-storagebox.de"
user = "u656759"
port = 23
identity_file = "/keys/hetzner-key"

[pairs.studio]
remote = "hetzner"
local = "/local/studio"
path = "my studio"
"#,
        )
        .unwrap()
        .resolve("studio")
        .unwrap();

        assert_eq!(
            df_argv(&pair, "u656759@u656759.your-storagebox.de", "my studio").unwrap(),
            vec![
                "ssh",
                "-p",
                "23",
                "-i",
                "/keys/hetzner-key",
                "-o",
                "IdentitiesOnly=yes",
                "u656759@u656759.your-storagebox.de",
                "df -Pk 'my studio'",
            ]
        );
    }

    #[test]
    fn walks_up_to_a_disk_that_is_actually_there() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("backup");
        std::fs::create_dir_all(&root).unwrap();

        assert_eq!(nearest_existing(&root).unwrap(), root);
        assert_eq!(nearest_existing(&root.join("studio/song")).unwrap(), root);
        assert_eq!(nearest_existing(Path::new("relative-nonsense")), None);
    }
}
