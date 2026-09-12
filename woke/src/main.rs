//! Keep this Mac awake with the lid closed.
//!
//! `caffeinate` cannot do this. All it does is take out power assertions, and
//! those only suppress sleep triggered by the idle timer. Closing the lid is a
//! separate, forced path in the kernel that ignores userspace assertions -- the
//! power log calls it "Clamshell Sleep" -- which is why a service stops
//! answering a few minutes after you close the machine. Apple documents the
//! limitation outright in IOKit's public IOPMLib.h, where
//! kIOPMAssertPreventUserIdleSystemSleep is described as leaving the system
//! free to "sleep for lid close, Apple menu, low battery, or other sleep
//! reasons".
//!
//! The one knob that stops it is `pmset disablesleep`, an undocumented setting
//! backed by SleepDisabled in IOPMrootDomain. Underneath, pmset is calling
//! IOPMSetSystemPowerSetting(CFSTR("SleepDisabled"), ...), which is exported
//! from IOKit but declared only in IOPMLibPrivate.h -- and calling it directly
//! buys nothing, since it returns kIOReturnNotPrivileged without root just as
//! pmset does. So this shells out to pmset under sudo.
//!
//! The setting is global and it survives reboots, which is the awkward part:
//! set it and forget it, and the laptop will happily cook itself in a bag some
//! week later. So it is held only while this process runs and put back on the
//! way out, whether that is Ctrl-C, a TERM, or the wrapped command finishing.

use anyhow::{Context, Result, bail};
use clap::Parser;
use std::io::Write;
use std::path::Path;
use std::process::{Command, ExitCode, ExitStatus, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::Duration;
use tempfile::NamedTempFile;

const SUDOERS_FILE: &str = "/etc/sudoers.d/woke";
const PMSET: &str = "/usr/bin/pmset";

/// Keep this Mac awake with the lid closed.
///
/// Disables system sleep for as long as this runs, then puts the setting back.
/// The display is untouched and still sleeps normally. Needs sudo for pmset;
/// run `woke --install-sudoers` once so it stops asking for a password.
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Print what is currently set and exit.
    #[arg(long, conflicts_with_all = ["install_sudoers", "uninstall_sudoers"])]
    status: bool,

    /// Permit this tool's pmset calls without a password, once and for all.
    #[arg(long, conflicts_with = "uninstall_sudoers")]
    install_sudoers: bool,

    /// Undo --install-sudoers.
    #[arg(long)]
    uninstall_sudoers: bool,

    /// Hold sleep off only while this command runs, instead of until Ctrl-C.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    command: Vec<String>,
}

fn main() -> ExitCode {
    // Rust ignores SIGPIPE at startup, which turns `woke --status | head` into a
    // panic on the broken pipe rather than the quiet exit every other command
    // in a pipeline manages. Put the default disposition back.
    unsafe { libc::signal(libc::SIGPIPE, libc::SIG_DFL) };

    match run() {
        Ok(code) => code,
        Err(err) => {
            eprintln!("woke: {err:#}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<ExitCode> {
    let args = Args::parse();

    if args.status {
        return print_status().map(|()| ExitCode::SUCCESS);
    }
    if args.install_sudoers {
        return install_sudoers().map(|()| ExitCode::SUCCESS);
    }
    if args.uninstall_sudoers {
        return uninstall_sudoers().map(|()| ExitCode::SUCCESS);
    }

    hold(&args.command)
}

fn hold(command: &[String]) -> Result<ExitCode> {
    if sleep_disabled()? {
        println!("==> sleep is already disabled by something else; leaving that setting alone");
        return wait_for_release(command);
    }

    if !sudo_is_passwordless() {
        eprintln!("woke: sudo will ask for your password now, and again on the way out.");
        eprintln!("      Run `woke --install-sudoers` once to stop that.");
    }

    let _guard = SleepGuard::acquire()?;
    wait_for_release(command)
}

/// Owns the sleep setting for as long as it is alive.
struct SleepGuard;

impl SleepGuard {
    fn acquire() -> Result<Self> {
        // Claim ownership before touching anything, not after. If the call
        // half-works, the restore in Drop still has to run: the one
        // unrecoverable outcome is exiting with sleep disabled and nobody
        // knowing it.
        let guard = SleepGuard;
        set_disablesleep(true)?;
        Ok(guard)
    }
}

impl Drop for SleepGuard {
    fn drop(&mut self) {
        match set_disablesleep(false) {
            Ok(()) => println!("==> sleep re-enabled"),
            Err(err) => {
                eprintln!();
                eprintln!("woke: FAILED to re-enable sleep: {err:#}");
                eprintln!("This Mac will not sleep, on any power source, until you run:");
                eprintln!();
                eprintln!("    sudo {PMSET} -a disablesleep 0");
                eprintln!();
            }
        }
    }
}

fn set_disablesleep(disabled: bool) -> Result<()> {
    let value = if disabled { "1" } else { "0" };
    let status = Command::new("sudo")
        .args(["pmset", "-a", "disablesleep", value])
        .status()
        .context("could not run sudo pmset")?;
    if !status.success() {
        bail!("sudo pmset -a disablesleep {value} failed");
    }

    // pmset exits 0 even when it refuses the command outright -- run it as a
    // normal user and it prints "'pmset' must be run as root..." and still
    // returns success -- so its exit status proves nothing. Ask the kernel.
    if sleep_disabled()? != disabled {
        bail!("pmset reported success but the setting did not change");
    }
    Ok(())
}

/// The kernel's own view of the setting, which is more trustworthy than the
/// stored preference: `pmset -g custom` does not even list disablesleep until
/// something has set it once.
fn sleep_disabled() -> Result<bool> {
    let output = Command::new("/usr/sbin/ioreg")
        .args(["-n", "IOPMrootDomain", "-r", "-d", "1"])
        .output()
        .context("could not run ioreg")?;
    parse_sleep_disabled(&String::from_utf8_lossy(&output.stdout))
        .context("ioreg did not report SleepDisabled")
}

fn parse_sleep_disabled(ioreg_output: &str) -> Option<bool> {
    ioreg_output
        .lines()
        .find_map(|line| line.split_once("\"SleepDisabled\" = "))
        .map(|(_, value)| value.trim() == "Yes")
}

/// Whether the exact pmset call is already allowed without a password. `sudo -l`
/// answers that without running anything and, with -n, without prompting.
fn sudo_is_passwordless() -> bool {
    Command::new("sudo")
        .args(["-n", "-l", PMSET, "-a", "disablesleep", "1"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

fn wait_for_release(command: &[String]) -> Result<ExitCode> {
    // Records which signal arrived, not merely that one did, so the exit code
    // can be the conventional 128 + signo rather than a single guess.
    let signalled = Arc::new(AtomicUsize::new(0));
    for signal in [
        signal_hook::consts::SIGINT,
        signal_hook::consts::SIGTERM,
        signal_hook::consts::SIGHUP,
    ] {
        signal_hook::flag::register_usize(
            signal,
            Arc::clone(&signalled),
            usize::try_from(signal).expect("signal numbers are small and positive"),
        )
        .context("could not install signal handlers")?;
    }
    let signal_exit_code = |signalled: &AtomicUsize| {
        let signo = u8::try_from(signalled.load(Ordering::Relaxed)).unwrap_or(0);
        ExitCode::from(128u8.saturating_add(signo))
    };

    let Some((program, rest)) = command.split_first() else {
        println!("==> lid sleep off -- close the lid; Ctrl-C here to release");
        while signalled.load(Ordering::Relaxed) == 0 {
            thread::sleep(Duration::from_millis(100));
        }
        return Ok(signal_exit_code(&signalled));
    };

    println!("==> lid sleep off while: {}", command.join(" "));
    let mut child = Command::new(program)
        .args(rest)
        .spawn()
        .with_context(|| format!("could not run {program}"))?;

    loop {
        if let Some(status) = child.try_wait().context("waiting for the command")? {
            return Ok(exit_code(status));
        }
        if signalled.load(Ordering::Relaxed) != 0 {
            // A Ctrl-C reaches the child too, through the process group, so it
            // is normally already gone by now and the try_wait above caught it.
            // Getting here means we were signalled on our own, in which case
            // stop holding the setting and leave the command to its own life.
            println!("==> released; {program} is still running");
            return Ok(signal_exit_code(&signalled));
        }
        thread::sleep(Duration::from_millis(100));
    }
}

fn exit_code(status: ExitStatus) -> ExitCode {
    // A command killed by a signal has no exit code of its own; 1 is as good a
    // summary as any.
    match status.code() {
        Some(code) => ExitCode::from(u8::try_from(code).unwrap_or(1)),
        None => ExitCode::from(1),
    }
}

fn print_status() -> Result<()> {
    println!(
        "SleepDisabled (kernel):  {}",
        if sleep_disabled()? { "Yes" } else { "No" }
    );
    // Deliberately two separate lines. Some other file in sudoers.d may well be
    // what is letting the pmset call through, and reporting that as though this
    // tool had installed it would be a lie.
    let passwordless = sudo_is_passwordless();
    println!(
        "Passwordless pmset:      {}",
        if passwordless {
            "yes"
        } else {
            "no (see --install-sudoers)"
        }
    );
    println!(
        "woke sudoers file:       {}",
        match (Path::new(SUDOERS_FILE).exists(), passwordless) {
            (true, _) => "installed",
            (false, true) => "not installed -- something else is granting it",
            (false, false) => "not installed",
        }
    );
    Ok(())
}

/// Only ever the exact argument vectors this tool uses. sudo matches specified
/// arguments literally, so this grants the sleep setting and nothing else: no
/// wildcards, no other pmset subcommand, no general root.
fn sudoers_content(user: &str) -> String {
    format!(
        "# Installed by woke. Lets {user} flip the system sleep setting without a\n\
         # password. Exact command matches only; remove with `woke --uninstall-sudoers`.\n\
         {user} ALL=(root) NOPASSWD: {PMSET} -a disablesleep 1, {PMSET} -a disablesleep 0\n"
    )
}

fn current_username() -> Result<String> {
    let output = Command::new("/usr/bin/id")
        .arg("-un")
        .output()
        .context("could not run id -un")?;
    if !output.status.success() {
        bail!("id -un failed");
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn install_sudoers() -> Result<()> {
    let content = sudoers_content(&current_username()?);

    let mut tmp = NamedTempFile::new().context("could not create a temp file")?;
    tmp.write_all(content.as_bytes())
        .and_then(|()| tmp.flush())
        .context("could not write the temp file")?;

    // Validate before going anywhere near /etc. A malformed file in sudoers.d
    // breaks sudo for everything, which is a memorable way to lock yourself out.
    let valid = Command::new("visudo")
        .args(["-c", "-f"])
        .arg(tmp.path())
        .stdout(Stdio::null())
        .status()
        .context("could not run visudo")?;
    if !valid.success() {
        bail!("the generated sudoers file did not validate");
    }

    println!("==> installing {SUDOERS_FILE} (asks for your password this one time)");
    let installed = Command::new("sudo")
        .args(["install", "-m", "0440", "-o", "root", "-g", "wheel"])
        .arg(tmp.path())
        .arg(SUDOERS_FILE)
        .status()
        .context("could not run sudo install")?;
    if !installed.success() {
        bail!("could not install {SUDOERS_FILE}");
    }

    // Re-check the whole ruleset, not just our fragment: the check above parsed
    // the file in isolation and cannot see what it does in context. Back the
    // change out at the first sign of trouble, while the sudo timestamp from the
    // install above is still warm enough to do it without another prompt.
    let whole_ruleset = Command::new("sudo")
        .args(["visudo", "-c"])
        .stdout(Stdio::null())
        .status()
        .context("could not re-check sudoers")?;
    if !whole_ruleset.success() {
        let _ = Command::new("sudo")
            .args(["rm", "-f", SUDOERS_FILE])
            .status();
        bail!("that file broke sudoers, so I removed it again; sudo is unharmed");
    }

    // A perfectly valid file in sudoers.d still does nothing at all if
    // /etc/sudoers has no @includedir line for that directory, so confirm the
    // rule actually bites rather than assuming it did.
    if sudo_is_passwordless() {
        println!("==> done -- woke will not ask for a password again");
    } else {
        println!("==> installed, but it is not taking effect: sudo still wants a password.");
        println!("    Check that /etc/sudoers has an @includedir for /etc/sudoers.d");
    }
    Ok(())
}

fn uninstall_sudoers() -> Result<()> {
    if !Path::new(SUDOERS_FILE).exists() {
        println!("==> {SUDOERS_FILE} is not there, nothing to do");
        return Ok(());
    }
    let status = Command::new("sudo")
        .args(["rm", "-f", SUDOERS_FILE])
        .status()
        .context("could not run sudo rm")?;
    if !status.success() {
        bail!("could not remove {SUDOERS_FILE}");
    }
    println!("==> removed {SUDOERS_FILE}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_sleep_disabled_from_ioreg_output() {
        let output = "\
+-o IOPMrootDomain  <class IOPMrootDomain>
    {
      \"SleepDisabled\" = No
      \"IOPMSystemCapabilities\" = 15
    }
";
        assert_eq!(parse_sleep_disabled(output), Some(false));
        assert_eq!(
            parse_sleep_disabled(&output.replace("= No", "= Yes")),
            Some(true)
        );
    }

    #[test]
    fn reports_missing_sleep_disabled_rather_than_guessing() {
        assert_eq!(parse_sleep_disabled("nothing to see here"), None);
    }

    #[test]
    fn sudoers_entry_pins_both_exact_commands() {
        let content = sudoers_content("simon");
        assert!(content.contains("simon ALL=(root) NOPASSWD:"));
        assert!(content.contains("/usr/bin/pmset -a disablesleep 1"));
        assert!(content.contains("/usr/bin/pmset -a disablesleep 0"));
        // A wildcard here would let any pmset invocation through.
        assert!(!content.contains('*'));
    }
}
