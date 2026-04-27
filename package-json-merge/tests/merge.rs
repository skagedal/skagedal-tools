use package_json_merge::merge::{MergeOutcome, run_merge};
use serde_json::{Value, json};
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

struct MergeCase {
    _dir: TempDir,
    base: PathBuf,
    ours: PathBuf,
    theirs: PathBuf,
}

impl MergeCase {
    fn new(base: Value, ours: Value, theirs: Value) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let base_path = dir.path().join("base.json");
        let ours_path = dir.path().join("ours.json");
        let theirs_path = dir.path().join("theirs.json");
        fs::write(
            &base_path,
            serde_json::to_string_pretty(&base).unwrap() + "\n",
        )
        .unwrap();
        fs::write(
            &ours_path,
            serde_json::to_string_pretty(&ours).unwrap() + "\n",
        )
        .unwrap();
        fs::write(
            &theirs_path,
            serde_json::to_string_pretty(&theirs).unwrap() + "\n",
        )
        .unwrap();
        Self {
            _dir: dir,
            base: base_path,
            ours: ours_path,
            theirs: theirs_path,
        }
    }

    fn run(&self) -> MergeOutcome {
        run_merge(&self.base, &self.ours, &self.theirs).unwrap()
    }

    fn merged(&self) -> Value {
        serde_json::from_str(&fs::read_to_string(&self.ours).unwrap()).unwrap()
    }
}

#[test]
fn both_bumped_same_dep_higher_wins() {
    let case = MergeCase::new(
        json!({"dependencies": {"foo": "^1.0.0"}}),
        json!({"dependencies": {"foo": "^1.5.0"}}),
        json!({"dependencies": {"foo": "^1.2.0"}}),
    );
    let outcome = case.run();
    assert!(!outcome.conflicted);
    assert_eq!(case.merged(), json!({"dependencies": {"foo": "^1.5.0"}}));
    assert_eq!(outcome.resolutions.len(), 1);
    let r = &outcome.resolutions[0];
    assert_eq!(r.section, "dependencies");
    assert_eq!(r.name, "foo");
    assert_eq!(r.picked, "^1.5.0");
}

#[test]
fn clean_merge_has_no_resolutions() {
    let case = MergeCase::new(
        json!({"dependencies": {"foo": "^1.0.0"}}),
        json!({"dependencies": {"foo": "^2.0.0"}}),
        json!({"dependencies": {"foo": "^1.0.0"}}),
    );
    let outcome = case.run();
    assert!(!outcome.conflicted);
    assert!(outcome.resolutions.is_empty());
}

#[test]
fn only_ours_bumped() {
    let case = MergeCase::new(
        json!({"dependencies": {"foo": "^1.0.0"}}),
        json!({"dependencies": {"foo": "^2.0.0"}}),
        json!({"dependencies": {"foo": "^1.0.0"}}),
    );
    let outcome = case.run();
    assert!(!outcome.conflicted);
    assert_eq!(case.merged(), json!({"dependencies": {"foo": "^2.0.0"}}));
}

#[test]
fn unioned_disjoint_deps() {
    let case = MergeCase::new(
        json!({"dependencies": {"foo": "^1.0.0"}}),
        json!({"dependencies": {"foo": "^1.0.0", "bar": "^1.0.0"}}),
        json!({"dependencies": {"foo": "^1.0.0", "baz": "^2.0.0"}}),
    );
    let outcome = case.run();
    assert!(!outcome.conflicted);
    assert_eq!(
        case.merged(),
        json!({"dependencies": {"foo": "^1.0.0", "bar": "^1.0.0", "baz": "^2.0.0"}})
    );
}

#[test]
fn dep_added_on_both_sides_higher_wins() {
    let case = MergeCase::new(
        json!({}),
        json!({"dependencies": {"foo": "^1.2.0"}}),
        json!({"dependencies": {"foo": "^1.5.0"}}),
    );
    let outcome = case.run();
    assert!(!outcome.conflicted);
    assert_eq!(case.merged(), json!({"dependencies": {"foo": "^1.5.0"}}));
}

#[test]
fn dep_removed_one_side_modified_other_conflicts() {
    let case = MergeCase::new(
        json!({"dependencies": {"foo": "^1.0.0"}}),
        json!({"dependencies": {"foo": "^2.0.0"}}),
        json!({"dependencies": {}}),
    );
    let outcome = case.run();
    assert!(outcome.conflicted);
}

#[test]
fn dep_removed_one_side_untouched_other_applies() {
    let case = MergeCase::new(
        json!({"dependencies": {"foo": "^1.0.0", "bar": "^1.0.0"}}),
        json!({"dependencies": {"foo": "^1.0.0"}}),
        json!({"dependencies": {"foo": "^1.0.0", "bar": "^1.0.0"}}),
    );
    let outcome = case.run();
    assert!(!outcome.conflicted);
    assert_eq!(case.merged(), json!({"dependencies": {"foo": "^1.0.0"}}));
}

#[test]
fn non_version_conflict_falls_back() {
    let case = MergeCase::new(
        json!({"dependencies": {"foo": "^1.0.0"}}),
        json!({"dependencies": {"foo": "workspace:*"}}),
        json!({"dependencies": {"foo": "github:other/repo"}}),
    );
    let outcome = case.run();
    assert!(outcome.conflicted);
}

#[test]
fn conflicting_top_level_keys_fall_back() {
    let case = MergeCase::new(
        json!({"name": "pkg", "version": "1.0.0"}),
        json!({"name": "pkg", "version": "1.1.0"}),
        json!({"name": "pkg", "version": "1.2.0"}),
    );
    let outcome = case.run();
    assert!(outcome.conflicted);
}

#[test]
fn non_conflicting_changes_across_top_level_keys_merge() {
    let case = MergeCase::new(
        json!({"name": "pkg", "version": "1.0.0", "dependencies": {"foo": "^1.0.0"}}),
        json!({"name": "pkg", "version": "1.1.0", "dependencies": {"foo": "^1.0.0"}}),
        json!({"name": "pkg", "version": "1.0.0", "dependencies": {"foo": "^1.5.0"}}),
    );
    let outcome = case.run();
    assert!(!outcome.conflicted);
    assert_eq!(
        case.merged(),
        json!({"name": "pkg", "version": "1.1.0", "dependencies": {"foo": "^1.5.0"}})
    );
}

#[test]
fn dev_and_peer_dependencies_use_semver_max() {
    let case = MergeCase::new(
        json!({
            "devDependencies": {"tsx": "^4.0.0"},
            "peerDependencies": {"react": "^18.0.0"}
        }),
        json!({
            "devDependencies": {"tsx": "^4.5.0"},
            "peerDependencies": {"react": "^18.2.0"}
        }),
        json!({
            "devDependencies": {"tsx": "^4.3.0"},
            "peerDependencies": {"react": "^19.0.0"}
        }),
    );
    let outcome = case.run();
    assert!(!outcome.conflicted);
    assert_eq!(
        case.merged(),
        json!({
            "devDependencies": {"tsx": "^4.5.0"},
            "peerDependencies": {"react": "^19.0.0"}
        })
    );
}

#[test]
fn indentation_of_ours_is_preserved() {
    use serde::Serialize;

    let dir = tempfile::tempdir().unwrap();
    let base = dir.path().join("base.json");
    let ours = dir.path().join("ours.json");
    let theirs = dir.path().join("theirs.json");
    fs::write(
        &base,
        serde_json::to_string_pretty(&json!({"dependencies": {"foo": "^1.0.0"}})).unwrap() + "\n",
    )
    .unwrap();

    let mut buf = Vec::new();
    let formatter = serde_json::ser::PrettyFormatter::with_indent(b"    ");
    let mut ser = serde_json::Serializer::with_formatter(&mut buf, formatter);
    json!({"dependencies": {"foo": "^1.5.0"}}).serialize(&mut ser).unwrap();
    fs::write(&ours, format!("{}\n", String::from_utf8(buf).unwrap())).unwrap();

    fs::write(
        &theirs,
        serde_json::to_string_pretty(&json!({"dependencies": {"foo": "^1.2.0"}})).unwrap() + "\n",
    )
    .unwrap();

    let outcome = run_merge(&base, &ours, &theirs).unwrap();
    assert!(!outcome.conflicted);
    let merged = fs::read_to_string(&ours).unwrap();
    assert!(
        merged.contains("\n    \"dependencies\""),
        "expected 4-space indent, got: {merged:?}"
    );
}

