//! The API key in the macOS keychain, through `/usr/bin/security`.
//!
//! Shelling out rather than linking a keychain crate: the command is stable,
//! it is the same one you would type, and it keeps the tool building on the
//! Linux runner that checks this workspace. The secret never appears in an
//! argument vector — `security` takes it on stdin, and hands it back on
//! stdout — so it stays out of `ps` output.

use std::io::Write;
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};

/// The keychain service the key is filed under.
pub const DEFAULT_SERVICE: &str = "trafikverket";

/// Overrides the service name. Tests set it so they never see, or touch, a
/// real key.
pub const SERVICE_ENV: &str = "TRAFIKVERKET_KEYCHAIN_SERVICE";

/// The account within the service. One key, one name.
pub const ACCOUNT: &str = "api-key";

/// What the item is called in Keychain Access, which shows the service name
/// only when there is no label.
const LABEL: &str = "trafikverket API key";

/// `security`'s own code for "no such item", as opposed to a keychain that is
/// locked or a user who said no.
const NOT_FOUND: i32 = 44;

const SECURITY: &str = "/usr/bin/security";

pub fn service() -> String {
    std::env::var(SERVICE_ENV)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| DEFAULT_SERVICE.to_string())
}

/// Whether this platform has a keychain to talk to at all.
pub fn is_available() -> bool {
    cfg!(target_os = "macos") && std::path::Path::new(SECURITY).exists()
}

/// The stored key, or `None` when there is none — or nowhere to store one.
/// A keychain that refuses to answer is an error rather than a silent miss:
/// falling back to "no key configured" would be a confusing way to report a
/// locked keychain.
pub fn get() -> Result<Option<String>> {
    get_in(&service())
}

fn get_in(service: &str) -> Result<Option<String>> {
    if !is_available() {
        return Ok(None);
    }
    let output = Command::new(SECURITY)
        .args(["find-generic-password", "-s", service, "-a", ACCOUNT, "-w"])
        .output()
        .with_context(|| format!("could not run {SECURITY}"))?;
    if output.status.code() == Some(NOT_FOUND) {
        return Ok(None);
    }
    if !output.status.success() {
        bail!(
            "could not read the key from the keychain: {}",
            complaint(&output.stderr)
        );
    }
    let key = String::from_utf8(output.stdout)
        .context("the keychain item is not text — was it written by something else?")?
        .trim()
        .to_string();
    Ok((!key.is_empty()).then_some(key))
}

/// Store the key, replacing whatever was filed under the same name.
pub fn set(key: &str) -> Result<()> {
    set_in(&service(), key)
}

fn set_in(service: &str, key: &str) -> Result<()> {
    require_keychain()?;
    let mut command = Command::new(SECURITY);
    command
        .args([
            "add-generic-password",
            "-s",
            service,
            "-a",
            ACCOUNT,
            "-l",
            LABEL,
            // Replace an existing item rather than failing on it.
            "-U",
            // No value after -w: the secret comes on stdin instead, twice,
            // since security asks for it and then for a confirmation.
            "-w",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    detach_from_terminal(&mut command);
    let mut child = command
        .spawn()
        .with_context(|| format!("could not run {SECURITY}"))?;
    {
        let stdin = child
            .stdin
            .as_mut()
            .expect("stdin was piped when the child was spawned");
        write!(stdin, "{key}\n{key}\n").context("could not hand the key to the keychain")?;
    }
    let output = child
        .wait_with_output()
        .context("could not wait for the keychain")?;
    if !output.status.success() {
        bail!(
            "could not store the key in the keychain: {}",
            complaint(&output.stderr)
        );
    }
    Ok(())
}

/// Remove the key. Returns false when there was none to remove.
pub fn delete() -> Result<bool> {
    delete_in(&service())
}

fn delete_in(service: &str) -> Result<bool> {
    require_keychain()?;
    let output = Command::new(SECURITY)
        .args(["delete-generic-password", "-s", service, "-a", ACCOUNT])
        .output()
        .with_context(|| format!("could not run {SECURITY}"))?;
    if output.status.code() == Some(NOT_FOUND) {
        return Ok(false);
    }
    if !output.status.success() {
        bail!(
            "could not remove the key from the keychain: {}",
            complaint(&output.stderr)
        );
    }
    Ok(true)
}

/// `security` asks for the passphrase with `readpassphrase`, which takes it
/// from `/dev/tty` whenever there is one and never looks at the stdin we piped
/// it — so run from a terminal it would block on a prompt forever. Its own
/// session leaves it no terminal to ask on, and it falls back to stdin.
fn detach_from_terminal(command: &mut Command) {
    unsafe {
        command.pre_exec(|| {
            // Fails only for a session leader, which a fresh child is not.
            libc::setsid();
            Ok(())
        });
    }
}

fn require_keychain() -> Result<()> {
    if !is_available() {
        bail!(
            "there is no keychain here — that is a macOS thing. Set ${} instead",
            crate::config::API_KEY_ENV
        );
    }
    Ok(())
}

/// What `security` had to say. It prefixes its own errors with its name, and
/// writes its prompts without a newline, so the message is what follows the
/// last such prefix rather than the last line.
fn complaint(stderr: &[u8]) -> String {
    let text = String::from_utf8_lossy(stderr);
    let message = match text.rfind(PREFIX) {
        Some(index) => &text[index + PREFIX.len()..],
        None => text.as_ref(),
    };
    let message = message.lines().next().unwrap_or("").trim();
    if message.is_empty() {
        "the keychain said nothing about why".to_string()
    } else {
        message.to_string()
    }
}

const PREFIX: &str = "security:";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_complaint_is_what_security_said_after_its_own_prompts() {
        // The prompts come without a newline, so this really is one line.
        let stderr = b"password data for new item: retype password for new item: \
                       security: SecKeychainAddGenericPassword: bad news\n";
        assert_eq!(complaint(stderr), "SecKeychainAddGenericPassword: bad news");
        assert_eq!(
            complaint(b"security: SecKeychainSearchCopyNext: not found\n"),
            "SecKeychainSearchCopyNext: not found"
        );
        assert!(complaint(b"").contains("nothing about why"));
    }

    /// The point of the module is the three `security` invocations, so the
    /// one test worth having runs them. It files its item under a service
    /// name nothing else uses, so the real key is never in reach — but it is
    /// still the login keychain it writes to, which is no business of an
    /// ordinary `cargo test`. Hence ignored by default. Run it deliberately:
    ///
    /// ```text
    /// cargo test -p trafikverket keychain -- --ignored
    /// ```
    #[test]
    #[cfg(target_os = "macos")]
    #[ignore = "writes to the real login keychain; run with --ignored"]
    fn a_key_survives_a_round_trip_through_the_keychain() {
        let service = format!("trafikverket-test-{}", std::process::id());
        // Whatever happened last time, start from nothing.
        delete_in(&service).unwrap();
        assert_eq!(get_in(&service).unwrap(), None);

        set_in(&service, "abc123").unwrap();
        assert_eq!(get_in(&service).unwrap().as_deref(), Some("abc123"));

        // -U replaces rather than adding a second item.
        set_in(&service, "def456").unwrap();
        assert_eq!(get_in(&service).unwrap().as_deref(), Some("def456"));

        assert!(delete_in(&service).unwrap());
        assert!(!delete_in(&service).unwrap());
        assert_eq!(get_in(&service).unwrap(), None);
    }
}
