//! OSC 8 hyperlink and per-platform browser-launch helpers.

const ESC: &str = "\x1b";

/// Wrap `text` in an OSC 8 hyperlink escape sequence pointing at `url`.
/// Terminals that understand OSC 8 render it as a clickable link; others
/// display the text plainly.
pub fn hyperlink(url: &str, text: &str) -> String {
    let st = format!("{ESC}\\");
    format!("{ESC}]8;;{url}{st}{text}{ESC}]8;;{st}")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenUrlCommand {
    pub cmd: String,
    pub args: Vec<String>,
}

/// Per-platform command for opening a URL in the user's default browser.
pub fn open_url_command(url: &str, platform: &str) -> OpenUrlCommand {
    match platform {
        "macos" => OpenUrlCommand {
            cmd: "open".into(),
            args: vec![url.into()],
        },
        "windows" => OpenUrlCommand {
            // `start` is a cmd.exe builtin; the empty "" is the window title.
            cmd: "cmd".into(),
            args: vec!["/c".into(), "start".into(), "".into(), url.into()],
        },
        _ => OpenUrlCommand {
            cmd: "xdg-open".into(),
            args: vec![url.into()],
        },
    }
}

/// Resolve the current platform string used by [`open_url_command`].
pub fn current_platform() -> &'static str {
    if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else {
        "linux"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hyperlink_basic() {
        let result = hyperlink("https://example.com/foo", "click me");
        assert_eq!(
            result,
            "\x1b]8;;https://example.com/foo\x1b\\click me\x1b]8;;\x1b\\"
        );
    }

    #[test]
    fn hyperlink_empty_text() {
        assert_eq!(
            hyperlink("https://x", ""),
            "\x1b]8;;https://x\x1b\\\x1b]8;;\x1b\\"
        );
    }

    #[test]
    fn open_url_macos() {
        assert_eq!(
            open_url_command("https://x", "macos"),
            OpenUrlCommand {
                cmd: "open".into(),
                args: vec!["https://x".into()],
            }
        );
    }

    #[test]
    fn open_url_windows() {
        assert_eq!(
            open_url_command("https://x", "windows"),
            OpenUrlCommand {
                cmd: "cmd".into(),
                args: vec!["/c".into(), "start".into(), "".into(), "https://x".into()],
            }
        );
    }

    #[test]
    fn open_url_linux() {
        assert_eq!(
            open_url_command("https://x", "linux"),
            OpenUrlCommand {
                cmd: "xdg-open".into(),
                args: vec!["https://x".into()],
            }
        );
        assert_eq!(
            open_url_command("https://x", "freebsd"),
            OpenUrlCommand {
                cmd: "xdg-open".into(),
                args: vec!["https://x".into()],
            }
        );
    }
}
