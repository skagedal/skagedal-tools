use anyhow::{Context, Result};
use node_semver::Range;
use serde::Serialize;
use serde_json::{Map, Value};
use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::process::Command;

const DEP_KEYS: &[&str] = &[
    "dependencies",
    "devDependencies",
    "peerDependencies",
    "optionalDependencies",
];

#[derive(Debug, Clone)]
pub struct Resolution {
    pub section: String,
    pub name: String,
    pub ours: String,
    pub theirs: String,
    pub picked: String,
}

#[derive(Debug, Default)]
pub struct MergeOutcome {
    pub conflicted: bool,
    pub reason: Option<String>,
    pub resolutions: Vec<Resolution>,
}

pub fn run_merge(base_path: &Path, ours_path: &Path, theirs_path: &Path) -> Result<MergeOutcome> {
    let ours_raw = fs::read_to_string(ours_path)
        .with_context(|| format!("reading {}", ours_path.display()))?;
    let base_raw = fs::read_to_string(base_path).ok();
    let theirs_raw = fs::read_to_string(theirs_path).ok();

    let (Some(base_raw), Some(theirs_raw)) = (base_raw, theirs_raw) else {
        return Ok(fallback(
            base_path,
            ours_path,
            theirs_path,
            "missing base or theirs file",
        ));
    };

    let base = match serde_json::from_str::<Value>(&base_raw) {
        Ok(v) => v,
        Err(e) => {
            return Ok(fallback(
                base_path,
                ours_path,
                theirs_path,
                &format!("parse base: {e}"),
            ));
        }
    };
    let ours = match serde_json::from_str::<Value>(&ours_raw) {
        Ok(v) => v,
        Err(e) => {
            return Ok(fallback(
                base_path,
                ours_path,
                theirs_path,
                &format!("parse ours: {e}"),
            ));
        }
    };
    let theirs = match serde_json::from_str::<Value>(&theirs_raw) {
        Ok(v) => v,
        Err(e) => {
            return Ok(fallback(
                base_path,
                ours_path,
                theirs_path,
                &format!("parse theirs: {e}"),
            ));
        }
    };

    let (Value::Object(base), Value::Object(ours_obj), Value::Object(theirs_obj)) =
        (base, ours, theirs)
    else {
        return Ok(fallback(
            base_path,
            ours_path,
            theirs_path,
            "top-level value is not a JSON object",
        ));
    };

    let mut resolutions = Vec::new();
    let result = merge_object(&base, &ours_obj, &theirs_obj, &mut resolutions);

    if result.conflicted {
        return Ok(fallback(
            base_path,
            ours_path,
            theirs_path,
            "structural conflict outside of dependencies",
        ));
    }

    let indent = detect_indent(&ours_raw);
    let trailing = if ours_raw.ends_with('\n') { "\n" } else { "" };
    let serialized = serialize_pretty(&Value::Object(result.value), &indent);
    fs::write(ours_path, format!("{serialized}{trailing}"))?;

    Ok(MergeOutcome {
        conflicted: false,
        reason: None,
        resolutions,
    })
}

struct ValueResult {
    value: Value,
    conflicted: bool,
}

struct ObjectResult {
    value: Map<String, Value>,
    conflicted: bool,
}

fn merge_value(
    base: Option<&Value>,
    ours: &Value,
    theirs: &Value,
    parent_key: Option<&str>,
    resolutions: &mut Vec<Resolution>,
) -> ValueResult {
    if ours == theirs {
        return ValueResult {
            value: ours.clone(),
            conflicted: false,
        };
    }
    if base == Some(ours) {
        return ValueResult {
            value: theirs.clone(),
            conflicted: false,
        };
    }
    if base == Some(theirs) {
        return ValueResult {
            value: ours.clone(),
            conflicted: false,
        };
    }

    if let (Value::Object(o), Value::Object(t)) = (ours, theirs) {
        let empty = Map::new();
        let b = match base {
            Some(Value::Object(m)) => m,
            _ => &empty,
        };
        if parent_key.map(|k| DEP_KEYS.contains(&k)).unwrap_or(false) {
            let r = merge_deps(b, o, t, parent_key.unwrap(), resolutions);
            return ValueResult {
                value: Value::Object(r.value),
                conflicted: r.conflicted,
            };
        }
        let r = merge_object(b, o, t, resolutions);
        return ValueResult {
            value: Value::Object(r.value),
            conflicted: r.conflicted,
        };
    }

    ValueResult {
        value: ours.clone(),
        conflicted: true,
    }
}

fn merge_object(
    base: &Map<String, Value>,
    ours: &Map<String, Value>,
    theirs: &Map<String, Value>,
    resolutions: &mut Vec<Resolution>,
) -> ObjectResult {
    let mut merged = Map::new();
    let mut conflicted = false;

    for k in union_keys(ours, theirs) {
        let in_ours = ours.contains_key(&k);
        let in_theirs = theirs.contains_key(&k);
        let in_base = base.contains_key(&k);

        if in_ours && in_theirs {
            let r = merge_value(
                base.get(&k),
                ours.get(&k).unwrap(),
                theirs.get(&k).unwrap(),
                Some(&k),
                resolutions,
            );
            if r.conflicted {
                conflicted = true;
            }
            merged.insert(k, r.value);
        } else if in_ours {
            let ov = ours.get(&k).unwrap();
            if in_base && base.get(&k) != Some(ov) {
                conflicted = true;
                merged.insert(k, ov.clone());
            } else if !in_base {
                merged.insert(k, ov.clone());
            }
        } else {
            let tv = theirs.get(&k).unwrap();
            if in_base && base.get(&k) != Some(tv) {
                conflicted = true;
                merged.insert(k, tv.clone());
            } else if !in_base {
                merged.insert(k, tv.clone());
            }
        }
    }

    ObjectResult {
        value: merged,
        conflicted,
    }
}

fn merge_deps(
    base: &Map<String, Value>,
    ours: &Map<String, Value>,
    theirs: &Map<String, Value>,
    section: &str,
    resolutions: &mut Vec<Resolution>,
) -> ObjectResult {
    let mut merged = Map::new();
    let mut conflicted = false;

    for name in union_keys(ours, theirs) {
        let in_ours = ours.contains_key(&name);
        let in_theirs = theirs.contains_key(&name);
        let in_base = base.contains_key(&name);
        let base_v = base.get(&name);

        if in_ours && in_theirs {
            let ov = ours.get(&name).unwrap();
            let tv = theirs.get(&name).unwrap();
            if ov == tv {
                merged.insert(name, ov.clone());
            } else if Some(ov) == base_v {
                merged.insert(name, tv.clone());
            } else if Some(tv) == base_v {
                merged.insert(name, ov.clone());
            } else if let (Some(os), Some(ts)) = (ov.as_str(), tv.as_str())
                && let Some(higher) = pick_higher_range(os, ts)
            {
                resolutions.push(Resolution {
                    section: section.to_string(),
                    name: name.clone(),
                    ours: os.to_string(),
                    theirs: ts.to_string(),
                    picked: higher.clone(),
                });
                merged.insert(name, Value::String(higher));
            } else {
                conflicted = true;
                merged.insert(name, ov.clone());
            }
        } else if in_ours {
            let ov = ours.get(&name).unwrap();
            if in_base && base_v != Some(ov) {
                conflicted = true;
                merged.insert(name, ov.clone());
            } else if !in_base {
                merged.insert(name, ov.clone());
            }
        } else {
            let tv = theirs.get(&name).unwrap();
            if in_base && base_v != Some(tv) {
                conflicted = true;
                merged.insert(name, tv.clone());
            } else if !in_base {
                merged.insert(name, tv.clone());
            }
        }
    }

    ObjectResult {
        value: merged,
        conflicted,
    }
}

pub fn pick_higher_range(a: &str, b: &str) -> Option<String> {
    let ra: Range = a.parse().ok()?;
    let rb: Range = b.parse().ok()?;
    let am = ra.min_version()?;
    let bm = rb.min_version()?;
    if am >= bm {
        Some(a.to_string())
    } else {
        Some(b.to_string())
    }
}

fn union_keys(a: &Map<String, Value>, b: &Map<String, Value>) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for k in a.keys() {
        if seen.insert(k.clone()) {
            out.push(k.clone());
        }
    }
    for k in b.keys() {
        if seen.insert(k.clone()) {
            out.push(k.clone());
        }
    }
    out
}

fn detect_indent(content: &str) -> String {
    for line in content.lines().skip(1) {
        let ws: String = line
            .chars()
            .take_while(|c| *c == ' ' || *c == '\t')
            .collect();
        if !ws.is_empty() && line.trim_start().starts_with('"') {
            if ws.contains('\t') {
                return "  ".to_string();
            }
            return ws;
        }
    }
    "  ".to_string()
}

fn serialize_pretty(v: &Value, indent: &str) -> String {
    let mut buf = Vec::new();
    let formatter = serde_json::ser::PrettyFormatter::with_indent(indent.as_bytes());
    let mut ser = serde_json::Serializer::with_formatter(&mut buf, formatter);
    v.serialize(&mut ser).expect("serialization");
    String::from_utf8(buf).expect("utf-8 output")
}

fn fallback(base: &Path, ours: &Path, theirs: &Path, reason: &str) -> MergeOutcome {
    let _ = Command::new("git")
        .args(["merge-file", "-L", "ours", "-L", "base", "-L", "theirs"])
        .arg(ours)
        .arg(base)
        .arg(theirs)
        .status();
    MergeOutcome {
        conflicted: true,
        reason: Some(reason.to_string()),
        resolutions: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn higher_caret_wins() {
        assert_eq!(
            pick_higher_range("^1.2.3", "^1.5.0").as_deref(),
            Some("^1.5.0")
        );
        assert_eq!(
            pick_higher_range("^2.0.0", "^1.9.9").as_deref(),
            Some("^2.0.0")
        );
    }

    #[test]
    fn tilde_and_exact() {
        assert_eq!(
            pick_higher_range("~1.2.3", "1.2.4").as_deref(),
            Some("1.2.4")
        );
        assert_eq!(
            pick_higher_range("1.0.0", "1.0.0").as_deref(),
            Some("1.0.0")
        );
    }

    #[test]
    fn invalid_input_returns_none() {
        assert!(pick_higher_range("workspace:*", "^1.0.0").is_none());
        assert!(pick_higher_range("^1.0.0", "github:foo/bar").is_none());
    }
}
