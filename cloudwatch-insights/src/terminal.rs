//! OSC 8 hyperlink and per-platform browser-launch helpers.

const ESC: &str = "\x1b";
const RESET: &str = "\x1b[0m";

/// Wrap `text` in an OSC 8 hyperlink escape sequence pointing at `url`.
/// Terminals that understand OSC 8 render it as a clickable link; others
/// display the text plainly.
pub fn hyperlink(url: &str, text: &str) -> String {
    let st = format!("{ESC}\\");
    format!("{ESC}]8;;{url}{st}{text}{ESC}]8;;{st}")
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Color {
    Blue,
    Cyan,
    Green,
    Yellow,
    Red,
    Magenta,
    Gray,
}

impl Color {
    fn code(self) -> &'static str {
        match self {
            Color::Blue => "34",
            Color::Cyan => "36",
            Color::Green => "32",
            Color::Yellow => "33",
            Color::Red => "31",
            Color::Magenta => "35",
            Color::Gray => "90",
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ColorOptions {
    pub color: Option<Color>,
    pub bold: bool,
    pub underline: bool,
}

pub fn colorize(text: &str, opts: ColorOptions) -> String {
    let mut codes: Vec<&str> = Vec::new();
    if opts.bold {
        codes.push("1");
    }
    if opts.underline {
        codes.push("4");
    }
    if let Some(c) = opts.color {
        codes.push(c.code());
    }
    if codes.is_empty() {
        return text.to_string();
    }
    format!("{ESC}[{}m{text}{RESET}", codes.join(";"))
}

/// OSC 8 hyperlink whose visible text is wrapped in "link-like" SGR styling
/// (cyan + underline by default). Terminals without OSC 8 still see the
/// styled text.
pub fn styled_link(url: &str, text: &str, opts: ColorOptions) -> String {
    hyperlink(url, &colorize(text, opts))
}

pub fn default_link_style() -> ColorOptions {
    ColorOptions {
        color: Some(Color::Cyan),
        underline: true,
        bold: false,
    }
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
    fn colorize_with_styling() {
        assert_eq!(
            colorize(
                "hi",
                ColorOptions {
                    color: Some(Color::Cyan),
                    underline: true,
                    bold: false
                }
            ),
            "\x1b[4;36mhi\x1b[0m"
        );
    }

    #[test]
    fn colorize_without_styling_returns_text_unchanged() {
        assert_eq!(colorize("hi", ColorOptions::default()), "hi");
    }

    #[test]
    fn styled_link_combines_osc8_and_sgr() {
        let url = "https://example.com/q";
        let result = styled_link(
            url,
            "Open",
            ColorOptions {
                color: Some(Color::Cyan),
                underline: true,
                bold: false,
            },
        );
        assert_eq!(
            result,
            format!("\x1b]8;;{url}\x1b\\\x1b[4;36mOpen\x1b[0m\x1b]8;;\x1b\\")
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
