use simons_misc_helpers::format_dependency_changes::{
    DependencyChanges, is_major_bump, write_dependency_changes,
};

const FIXTURE: &str = include_str!("fixtures/git-pkgs-diff.json");

const EXPECTED_PLAIN: &str = "\
Added:
  + anstream 1.0.0 (Cargo.lock)
  + anstyle 1.0.14 (Cargo.lock)
  + anstyle-parse 1.0.0 (Cargo.lock)
  + anstyle-query 1.1.5 (Cargo.lock)
  + anstyle-wincon 3.0.11 (Cargo.lock)
  + clap_builder 4.6.0 (Cargo.lock)
  + colorchoice 1.0.5 (Cargo.lock)
  + is_terminal_polyfill 1.70.2 (Cargo.lock)
  + once_cell_polyfill 1.70.2 (Cargo.lock)
  + utf8parse 0.2.2 (Cargo.lock)

Modified (major version updates):
  ~ clap 3.2.25 -> 4.6.1 (Cargo.lock)
  ~ clap_lex 0.2.4 -> 1.1.0 (Cargo.lock)
  ~ strsim 0.10.0 -> 0.11.1 (Cargo.lock)

Modified (minor version updates):
  ~ clap 3 -> * (Cargo.toml)

Removed:
  - atty 0.2.14 (Cargo.lock)
  - autocfg 1.5.0 (Cargo.lock)
  - bitflags 1.3.2 (Cargo.lock)
  - hashbrown 0.12.3 (Cargo.lock)
  - hermit-abi 0.1.19 (Cargo.lock)
  - indexmap 1.9.3 (Cargo.lock)
  - libc 0.2.186 (Cargo.lock)
  - os_str_bytes 6.6.1 (Cargo.lock)
  - termcolor 1.4.1 (Cargo.lock)
  - textwrap 0.16.2 (Cargo.lock)
  - winapi 0.3.9 (Cargo.lock)
  - winapi-i686-pc-windows-gnu 0.4.0 (Cargo.lock)
  - winapi-util 0.1.11 (Cargo.lock)
  - winapi-x86_64-pc-windows-gnu 0.4.0 (Cargo.lock)
";

#[test]
fn formats_fixture_without_color() {
    let changes: DependencyChanges = serde_json::from_str(FIXTURE).unwrap();
    let mut out = Vec::new();
    write_dependency_changes(&mut out, &changes, false).unwrap();
    let actual = String::from_utf8(out).unwrap();
    assert_eq!(actual, EXPECTED_PLAIN);
}

#[test]
fn formats_fixture_with_color_emits_ansi_codes() {
    let changes: DependencyChanges = serde_json::from_str(FIXTURE).unwrap();
    let mut out = Vec::new();
    write_dependency_changes(&mut out, &changes, true).unwrap();
    let actual = String::from_utf8(out).unwrap();

    // Bold section headers (crossterm: \x1b[1m...\x1b[0m).
    assert!(actual.contains("\x1b[1mAdded:\x1b[0m"));
    assert!(actual.contains("\x1b[1mModified (major version updates):\x1b[0m"));
    assert!(actual.contains("\x1b[1mModified (minor version updates):\x1b[0m"));
    assert!(actual.contains("\x1b[1mRemoved:\x1b[0m"));
    // Green for added (crossterm Stylize::green: \x1b[38;5;10m...\x1b[39m).
    assert!(actual.contains("\x1b[38;5;10m+\x1b[39m"));
    assert!(actual.contains("\x1b[38;5;10manstream\x1b[39m"));
    // Yellow tilde + name for modified.
    assert!(actual.contains("\x1b[38;5;11m~\x1b[39m"));
    assert!(actual.contains("\x1b[38;5;11mclap\x1b[39m"));
    // The new version of a major bump renders in red.
    assert!(actual.contains("\x1b[38;5;9m4.6.1\x1b[39m"));
    assert!(actual.contains("\x1b[38;5;9m1.1.0\x1b[39m"));
    // Red for removed.
    assert!(actual.contains("\x1b[38;5;9m-\x1b[39m"));
    assert!(actual.contains("\x1b[38;5;9matty\x1b[39m"));
    // Dim for manifest paths and the from-version of modified entries.
    assert!(actual.contains("\x1b[2m(Cargo.lock)\x1b[0m"));
    assert!(actual.contains("\x1b[2m3.2.25\x1b[0m"));
}

#[test]
fn empty_sections_are_omitted() {
    let changes = DependencyChanges {
        added: Vec::new(),
        modified: Vec::new(),
        removed: Vec::new(),
    };
    let mut out = Vec::new();
    write_dependency_changes(&mut out, &changes, false).unwrap();
    assert_eq!(out, b"");
}

#[test]
fn only_added_section_no_trailing_blank_line() {
    let json = r#"{
        "added": [
            {
                "name": "foo",
                "ecosystem": "cargo",
                "manifest_path": "Cargo.lock",
                "dependency_type": "runtime",
                "to_requirement": "1.0.0"
            }
        ],
        "modified": [],
        "removed": []
    }"#;
    let changes: DependencyChanges = serde_json::from_str(json).unwrap();
    let mut out = Vec::new();
    write_dependency_changes(&mut out, &changes, false).unwrap();
    let actual = String::from_utf8(out).unwrap();
    assert_eq!(actual, "Added:\n  + foo 1.0.0 (Cargo.lock)\n");
}

#[test]
fn only_minor_section_appears_when_no_major_bumps() {
    let json = r#"{
        "added": [],
        "modified": [
            {
                "name": "foo",
                "ecosystem": "cargo",
                "manifest_path": "Cargo.lock",
                "dependency_type": "runtime",
                "from_requirement": "1.2.3",
                "to_requirement": "1.5.0"
            }
        ],
        "removed": []
    }"#;
    let changes: DependencyChanges = serde_json::from_str(json).unwrap();
    let mut out = Vec::new();
    write_dependency_changes(&mut out, &changes, false).unwrap();
    let actual = String::from_utf8(out).unwrap();
    assert_eq!(
        actual,
        "Modified (minor version updates):\n  ~ foo 1.2.3 -> 1.5.0 (Cargo.lock)\n",
    );
}

#[test]
fn unparseable_versions_classified_as_minor() {
    let json = r#"{
        "added": [],
        "modified": [
            {
                "name": "foo",
                "ecosystem": "cargo",
                "manifest_path": "Cargo.toml",
                "dependency_type": "runtime",
                "from_requirement": "3",
                "to_requirement": "*"
            }
        ],
        "removed": []
    }"#;
    let changes: DependencyChanges = serde_json::from_str(json).unwrap();
    let mut out = Vec::new();
    write_dependency_changes(&mut out, &changes, false).unwrap();
    let actual = String::from_utf8(out).unwrap();
    assert!(actual.contains("Modified (minor version updates):"));
    assert!(!actual.contains("Modified (major version updates):"));
}

#[test]
fn is_major_bump_classification() {
    // Standard major: leading non-zero component changes.
    assert_eq!(is_major_bump("1.2.3", "2.0.0"), Some(true));
    assert_eq!(is_major_bump("3.2.25", "4.6.1"), Some(true));
    // Minor / patch within same major.
    assert_eq!(is_major_bump("1.2.3", "1.5.0"), Some(false));
    assert_eq!(is_major_bump("1.2.3", "1.2.4"), Some(false));
    // 0.x: minor bump is treated as major (cargo caret rules).
    assert_eq!(is_major_bump("0.10.0", "0.11.1"), Some(true));
    assert_eq!(is_major_bump("0.2.4", "1.1.0"), Some(true));
    assert_eq!(is_major_bump("0.1.0", "0.1.5"), Some(false));
    // 0.0.x: every patch is major.
    assert_eq!(is_major_bump("0.0.1", "0.0.2"), Some(true));
    // Partial versions (e.g. "3" parses as 3.0.0).
    assert_eq!(is_major_bump("3", "3.0.1"), Some(false));
    assert_eq!(is_major_bump("3", "4"), Some(true));
    // Unparseable.
    assert_eq!(is_major_bump("3", "*"), None);
    assert_eq!(is_major_bump("*", "1.0.0"), None);
}
