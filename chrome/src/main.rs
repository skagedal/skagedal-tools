use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::{Command, ExitCode};

use anyhow::{Context, Result, bail};
use clap::error::ErrorKind;
use clap::{CommandFactory, Parser};
use serde::Deserialize;

/// Exit status when no profile is signed in as the account, which setup
/// scripts check for.
const NOT_SIGNED_IN: u8 = 3;

/// Opens URLs in Chrome, optionally in the profile signed in as an account.
///
/// Chrome names profile directories ("Default", "Profile 1", ...) in creation
/// order, so they differ between machines. This looks the directory up in
/// Chrome's Local State by the address signed into it instead.
#[derive(Parser)]
#[command(version)]
struct Args {
    /// Open in the profile signed in as this address; without it, Chrome picks.
    #[arg(long, short)]
    account: Option<String>,

    /// URLs to open; with none, just a window.
    urls: Vec<String>,

    /// Open the URLs in a new window rather than as tabs in the last one.
    #[arg(long)]
    new_window: bool,

    /// Open an incognito window.
    #[arg(long)]
    incognito: bool,

    /// Open the URL in its own window, without tabs or address bar.
    #[arg(long, requires = "urls")]
    app: bool,

    /// Print the account's profile directory instead of opening Chrome.
    #[arg(
        long,
        requires = "account",
        conflicts_with_all = ["urls", "new_window", "incognito", "app"]
    )]
    profile_dir: bool,

    /// List every profile directory and the account signed into it.
    #[arg(
        long,
        conflicts_with_all = ["account", "urls", "new_window", "incognito", "app", "profile_dir"]
    )]
    list: bool,
}

#[derive(Deserialize, Default)]
struct LocalState {
    #[serde(default)]
    profile: Profiles,
}

#[derive(Deserialize, Default)]
struct Profiles {
    #[serde(default)]
    info_cache: BTreeMap<String, ProfileInfo>,
}

#[derive(Deserialize)]
struct ProfileInfo {
    #[serde(default)]
    user_name: String,
    #[serde(default)]
    name: String,
}

fn local_state_path() -> Result<PathBuf> {
    let home = std::env::var_os("HOME").context("HOME is not set")?;
    Ok(PathBuf::from(home).join("Library/Application Support/Google/Chrome/Local State"))
}

/// The profiles, keyed by directory; `None` if Chrome has never run.
fn read_profiles() -> Result<Option<BTreeMap<String, ProfileInfo>>> {
    let path = local_state_path()?;
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e).with_context(|| format!("reading {}", path.display())),
    };
    let state: LocalState =
        serde_json::from_str(&text).with_context(|| format!("parsing {}", path.display()))?;
    Ok(Some(state.profile.info_cache))
}

fn find_profile_dir(profiles: &BTreeMap<String, ProfileInfo>, account: &str) -> Option<String> {
    profiles
        .iter()
        .find(|(_, info)| info.user_name.eq_ignore_ascii_case(account))
        .map(|(dir, _)| dir.clone())
}

fn list() -> Result<ExitCode> {
    let Some(profiles) = read_profiles()? else {
        bail!("No Chrome profiles yet: {}", local_state_path()?.display());
    };
    for (dir, info) in &profiles {
        let who = if info.user_name.is_empty() {
            format!("{} (not signed in)", info.name)
        } else {
            info.user_name.clone()
        };
        println!("{dir:<12} {who}");
    }
    Ok(ExitCode::SUCCESS)
}

fn run(args: Args) -> Result<ExitCode> {
    if args.list {
        return list();
    }

    let profile_dir = match &args.account {
        None => None,
        Some(account) => {
            let dir = read_profiles()?.and_then(|profiles| find_profile_dir(&profiles, account));
            let Some(dir) = dir else {
                eprintln!("No Chrome profile signed in as {account}.");
                eprintln!("Add it in Chrome (profile menu → Add), then re-run.");
                return Ok(ExitCode::from(NOT_SIGNED_IN));
            };
            Some(dir)
        }
    };

    if args.profile_dir {
        println!(
            "{}",
            profile_dir.expect("clap requires --account with --profile-dir")
        );
        return Ok(ExitCode::SUCCESS);
    }

    let mut command = Command::new("open");
    command.args(["-na", "Google Chrome", "--args"]);
    if let Some(dir) = profile_dir {
        command.arg(format!("--profile-directory={dir}"));
    }
    if args.new_window {
        command.arg("--new-window");
    }
    if args.incognito {
        command.arg("--incognito");
    }
    if args.app {
        // Chrome takes the app's URL as the switch's value, not positionally.
        command.arg(format!("--app={}", args.urls[0]));
    } else {
        command.args(&args.urls);
    }

    let status = command.status().context("running open")?;
    if !status.success() {
        bail!("open exited with {status}");
    }
    Ok(ExitCode::SUCCESS)
}

fn main() -> ExitCode {
    let args = Args::parse();
    if args.app && args.urls.len() > 1 {
        Args::command()
            .error(ErrorKind::TooManyValues, "--app opens exactly one URL")
            .exit();
    }
    match run(args) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("chrome: {e:#}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profiles(json: &str) -> BTreeMap<String, ProfileInfo> {
        serde_json::from_str::<LocalState>(json)
            .unwrap()
            .profile
            .info_cache
    }

    const STATE: &str = r#"{
        "profile": {
            "info_cache": {
                "Default": { "user_name": "me@example.com", "name": "Personal" },
                "Profile 1": { "user_name": "Me@Work.example", "name": "Work" },
                "Profile 2": { "name": "Guest-ish" }
            }
        }
    }"#;

    #[test]
    fn finds_profile_by_account_ignoring_case() {
        let p = profiles(STATE);
        assert_eq!(
            find_profile_dir(&p, "me@example.com").as_deref(),
            Some("Default")
        );
        assert_eq!(
            find_profile_dir(&p, "me@work.example").as_deref(),
            Some("Profile 1")
        );
    }

    #[test]
    fn unknown_account_has_no_profile() {
        assert_eq!(
            find_profile_dir(&profiles(STATE), "nobody@example.com"),
            None
        );
    }

    #[test]
    fn arguments_are_consistent() {
        Args::command().debug_assert();
    }

    #[test]
    fn tolerates_missing_profile_section() {
        assert!(profiles("{}").is_empty());
    }
}
