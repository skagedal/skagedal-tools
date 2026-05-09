use simons_misc_helpers::format_dependency_changes::{
    DependencyChanges, write_dependency_changes,
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

Modified:
  ~ clap 3.2.25 -> 4.6.1 (Cargo.lock)
  ~ clap_lex 0.2.4 -> 1.1.0 (Cargo.lock)
  ~ strsim 0.10.0 -> 0.11.1 (Cargo.lock)
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
    assert!(actual.contains("\x1b[1mModified:\x1b[0m"));
    assert!(actual.contains("\x1b[1mRemoved:\x1b[0m"));
    // Green for added (crossterm Stylize::green: \x1b[38;5;10m...\x1b[39m).
    assert!(actual.contains("\x1b[38;5;10m+\x1b[39m"));
    assert!(actual.contains("\x1b[38;5;10manstream\x1b[39m"));
    // Yellow for modified.
    assert!(actual.contains("\x1b[38;5;11m~\x1b[39m"));
    assert!(actual.contains("\x1b[38;5;11mclap\x1b[39m"));
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
