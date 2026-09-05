use anyhow::{Context, Result, anyhow, bail};
use clap::Parser;
use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};
use xmltree::{Element, EmitterConfig, XMLNode};

#[derive(Parser, Debug)]
#[command(version, about = "Apply XML patches to IntelliJ project files", long_about = None)]
struct Args {
    /// Path to the IntelliJ project to patch. Defaults to the current directory.
    project: Option<PathBuf>,

    /// Path to the config file. Defaults to
    /// ~/.config/skagedal-tools/intellij-patch/config.toml.
    #[arg(long)]
    config: Option<PathBuf>,

    /// Show what would change without writing files.
    #[arg(long)]
    dry_run: bool,
}

#[derive(Deserialize, Debug)]
struct Config {
    #[serde(default)]
    patch: Vec<Patch>,
}

#[derive(Deserialize, Debug)]
struct Patch {
    name: String,
    file: String,
    #[serde(default, rename = "applies-when-under")]
    applies_when_under: Vec<String>,
    #[serde(default, rename = "create-if-missing")]
    create_if_missing: Option<String>,
    #[serde(default, rename = "ensure-children")]
    ensure_children: Vec<EnsureChildren>,
}

#[derive(Deserialize, Debug)]
struct EnsureChildren {
    parent: String,
    #[serde(rename = "match-by")]
    match_by: String,
    fragments: Vec<String>,
}

fn main() -> Result<()> {
    let args = Args::parse();

    let project = args
        .project
        .map(|p| p.canonicalize().unwrap_or(p))
        .unwrap_or_else(|| std::env::current_dir().expect("cwd"));

    let config_path = match args.config {
        Some(p) => p,
        None => default_config_path(),
    };

    let config_text = fs::read_to_string(&config_path)
        .with_context(|| format!("reading config {}", config_path.display()))?;
    let config: Config = toml::from_str(&config_text)
        .with_context(|| format!("parsing config {}", config_path.display()))?;

    let mut applied = 0;
    let mut skipped = 0;
    for patch in &config.patch {
        match apply_patch(&project, patch, args.dry_run)? {
            PatchResult::Applied => applied += 1,
            PatchResult::AlreadyClean => applied += 1,
            PatchResult::Skipped(reason) => {
                println!("- {}: skipped ({})", patch.name, reason);
                skipped += 1;
            }
        }
    }

    println!(
        "==> {} patch(es) applied, {} skipped{}",
        applied,
        skipped,
        if args.dry_run { " (dry run)" } else { "" }
    );
    Ok(())
}

enum PatchResult {
    Applied,
    AlreadyClean,
    Skipped(String),
}

fn apply_patch(project: &Path, patch: &Patch, dry_run: bool) -> Result<PatchResult> {
    if !patch.applies_when_under.is_empty() {
        let prefixes: Vec<PathBuf> = patch
            .applies_when_under
            .iter()
            .map(|p| {
                let expanded = shellexpand::tilde(p);
                PathBuf::from(expanded.as_ref())
                    .canonicalize()
                    .unwrap_or_else(|_| PathBuf::from(expanded.as_ref()))
            })
            .collect();
        if !prefixes.iter().any(|p| project.starts_with(p)) {
            let display = prefixes
                .iter()
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>()
                .join(", ");
            return Ok(PatchResult::Skipped(format!(
                "project is not under any of: {display}"
            )));
        }
    }

    let target = project.join(&patch.file);
    let original_text = match fs::read_to_string(&target) {
        Ok(text) => Some(text),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => {
            return Err(anyhow!(e)).with_context(|| format!("reading {}", target.display()));
        }
    };

    let starting_text = match (&original_text, &patch.create_if_missing) {
        (Some(t), _) => t.clone(),
        (None, Some(template)) => template.clone(),
        (None, None) => {
            return Ok(PatchResult::Skipped(format!(
                "{} does not exist and no create-if-missing template was provided",
                target.display()
            )));
        }
    };

    let mut root = Element::parse(starting_text.as_bytes())
        .with_context(|| format!("parsing XML in {}", target.display()))?;

    for ensure in &patch.ensure_children {
        apply_ensure_children(&mut root, ensure)
            .with_context(|| format!("applying ensure-children for patch '{}'", patch.name))?;
    }

    let mut buf: Vec<u8> = Vec::new();
    let cfg = EmitterConfig::new()
        .perform_indent(true)
        .indent_string("  ")
        .pad_self_closing(true)
        .write_document_declaration(true);
    root.write_with_config(&mut buf, cfg)?;
    let new_text = String::from_utf8(buf).context("emitted XML was not valid UTF-8")?;

    let unchanged = original_text.as_deref() == Some(new_text.as_str());
    if unchanged {
        println!("- {}: ok", patch.name);
        return Ok(PatchResult::AlreadyClean);
    }

    if dry_run {
        println!("- {}: would update {}", patch.name, target.display());
    } else {
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
        }
        fs::write(&target, &new_text).with_context(|| format!("writing {}", target.display()))?;
        let verb = if original_text.is_some() {
            "updated"
        } else {
            "created"
        };
        println!("- {}: {} {}", patch.name, verb, target.display());
    }
    Ok(PatchResult::Applied)
}

fn apply_ensure_children(root: &mut Element, ensure: &EnsureChildren) -> Result<()> {
    let path = parse_path(&ensure.parent)?;
    let parent = walk_or_create(root, &path)?;

    let key_path = parse_match_by(&ensure.match_by)?;
    let existing_keys: Vec<Option<String>> = parent
        .children
        .iter()
        .filter_map(|n| match n {
            XMLNode::Element(e) => Some(eval_match(e, &key_path)),
            _ => None,
        })
        .collect();

    for fragment in &ensure.fragments {
        let elem = Element::parse(fragment.as_bytes())
            .with_context(|| format!("parsing fragment: {fragment}"))?;
        let key = eval_match(&elem, &key_path);
        let already_present = key.is_some() && existing_keys.iter().any(|k| k == &key);
        if !already_present {
            parent.children.push(XMLNode::Element(elem));
        }
    }

    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PathStep {
    tag: String,
    attrs: Vec<(String, String)>,
}

fn parse_path(input: &str) -> Result<Vec<PathStep>> {
    let trimmed = input.trim().trim_start_matches('/');
    if trimmed.is_empty() {
        bail!("path is empty");
    }
    trimmed.split('/').map(parse_step).collect()
}

fn parse_step(input: &str) -> Result<PathStep> {
    let s = input.trim();
    let bracket = s.find('[');
    let (tag, rest) = match bracket {
        Some(i) => (&s[..i], &s[i..]),
        None => (s, ""),
    };
    if tag.is_empty() {
        bail!("path step is missing tag name: {input:?}");
    }
    let attrs = parse_attr_predicates(rest)
        .with_context(|| format!("parsing predicates in step {input:?}"))?;
    Ok(PathStep {
        tag: tag.to_string(),
        attrs,
    })
}

fn parse_attr_predicates(mut s: &str) -> Result<Vec<(String, String)>> {
    let mut out = Vec::new();
    while !s.is_empty() {
        if !s.starts_with('[') {
            bail!("expected '[' but found {s:?}");
        }
        let end = s
            .find(']')
            .ok_or_else(|| anyhow!("missing closing ']' in predicate: {s:?}"))?;
        let inner = s[1..end].trim();
        let inner = inner.strip_prefix('@').ok_or_else(|| {
            anyhow!("only attribute predicates ([@name='val']) are supported: {inner:?}")
        })?;
        let eq = inner
            .find('=')
            .ok_or_else(|| anyhow!("predicate is missing '=': {inner:?}"))?;
        let name = inner[..eq].trim().to_string();
        let value_raw = inner[eq + 1..].trim();
        let value = strip_quotes(value_raw)
            .ok_or_else(|| anyhow!("predicate value must be quoted: {value_raw:?}"))?;
        out.push((name, value.to_string()));
        s = &s[end + 1..];
    }
    Ok(out)
}

fn strip_quotes(s: &str) -> Option<&str> {
    if (s.starts_with('\'') && s.ends_with('\'') && s.len() >= 2)
        || (s.starts_with('"') && s.ends_with('"') && s.len() >= 2)
    {
        Some(&s[1..s.len() - 1])
    } else {
        None
    }
}

fn walk_or_create<'a>(root: &'a mut Element, path: &[PathStep]) -> Result<&'a mut Element> {
    let first = path.first().ok_or_else(|| anyhow!("empty path"))?;
    if root.name != first.tag {
        bail!(
            "path root <{}> does not match document root <{}>",
            first.tag,
            root.name
        );
    }
    if !step_matches(root, first) {
        bail!(
            "document root <{}> does not satisfy the predicates for the first path step",
            root.name
        );
    }
    let mut cur = root;
    for step in &path[1..] {
        cur = ensure_child(cur, step);
    }
    Ok(cur)
}

fn step_matches(elem: &Element, step: &PathStep) -> bool {
    if elem.name != step.tag {
        return false;
    }
    step.attrs
        .iter()
        .all(|(k, v)| elem.attributes.get(k.as_str()) == Some(v))
}

fn ensure_child<'a>(parent: &'a mut Element, step: &PathStep) -> &'a mut Element {
    let mut found = None;
    for (i, node) in parent.children.iter().enumerate() {
        if let XMLNode::Element(e) = node
            && step_matches(e, step)
        {
            found = Some(i);
            break;
        }
    }
    let idx = match found {
        Some(i) => i,
        None => {
            let mut new_elem = Element::new(&step.tag);
            for (k, v) in &step.attrs {
                new_elem.attributes.insert(k.clone(), v.clone());
            }
            parent.children.push(XMLNode::Element(new_elem));
            parent.children.len() - 1
        }
    };
    match &mut parent.children[idx] {
        XMLNode::Element(e) => e,
        _ => unreachable!(),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct MatchPath {
    steps: Vec<PathStep>,
    attribute: String,
}

fn parse_match_by(input: &str) -> Result<MatchPath> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        bail!("match-by is empty");
    }
    let parts: Vec<&str> = trimmed.split('/').collect();
    let last = parts
        .last()
        .ok_or_else(|| anyhow!("match-by has no segments"))?;
    let attribute = last
        .strip_prefix('@')
        .ok_or_else(|| anyhow!("match-by must end with '@attr', got {last:?}"))?
        .to_string();
    let steps = parts[..parts.len() - 1]
        .iter()
        .map(|s| parse_step(s))
        .collect::<Result<Vec<_>>>()?;
    Ok(MatchPath { steps, attribute })
}

fn eval_match(elem: &Element, path: &MatchPath) -> Option<String> {
    let mut cur = elem;
    for step in &path.steps {
        let next = cur.children.iter().find_map(|n| match n {
            XMLNode::Element(e) if step_matches(e, step) => Some(e),
            _ => None,
        })?;
        cur = next;
    }
    cur.attributes.get(&path.attribute).cloned()
}

fn default_config_path() -> PathBuf {
    skagedal_dirs::config_dir("intellij-patch").join("config.toml")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_path_with_attribute_predicates() {
        let path = parse_path("project/component[@name='Foo']/option[@name='x'][@value='y']/list")
            .unwrap();
        assert_eq!(path.len(), 4);
        assert_eq!(path[0].tag, "project");
        assert!(path[0].attrs.is_empty());
        assert_eq!(path[1].tag, "component");
        assert_eq!(path[1].attrs, vec![("name".to_string(), "Foo".to_string())]);
        assert_eq!(
            path[2].attrs,
            vec![
                ("name".to_string(), "x".to_string()),
                ("value".to_string(), "y".to_string()),
            ]
        );
    }

    #[test]
    fn parses_match_by_own_attribute() {
        let m = parse_match_by("@name").unwrap();
        assert!(m.steps.is_empty());
        assert_eq!(m.attribute, "name");
    }

    #[test]
    fn parses_match_by_descendant_attribute() {
        let m = parse_match_by("option/@value").unwrap();
        assert_eq!(m.steps.len(), 1);
        assert_eq!(m.steps[0].tag, "option");
        assert_eq!(m.attribute, "value");
    }

    #[test]
    fn rejects_unsupported_predicates() {
        assert!(parse_path("foo[1]").is_err());
        assert!(parse_path("foo[@name]").is_err());
        assert!(parse_path("foo[@name=bar]").is_err());
    }

    #[test]
    fn ensure_children_creates_nested_structure() {
        let mut root =
            Element::parse(br#"<?xml version="1.0"?><project version="4"></project>"# as &[u8])
                .unwrap();
        let ensure = EnsureChildren {
            parent: "project/component[@name='Foo']/option[@name='entries']/list".to_string(),
            match_by: "option/@value".to_string(),
            fragments: vec![
                r#"<Entry><option name="location" value="A"/></Entry>"#.to_string(),
                r#"<Entry><option name="location" value="B"/></Entry>"#.to_string(),
            ],
        };
        apply_ensure_children(&mut root, &ensure).unwrap();

        let component = find_child(&root, "component").unwrap();
        assert_eq!(component.attributes.get("name"), Some(&"Foo".to_string()));
        let option = find_child(component, "option").unwrap();
        let list = find_child(option, "list").unwrap();
        let entries: Vec<&Element> = list
            .children
            .iter()
            .filter_map(|n| match n {
                XMLNode::Element(e) => Some(e),
                _ => None,
            })
            .collect();
        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn ensure_children_is_idempotent() {
        let mut root =
            Element::parse(br#"<?xml version="1.0"?><project version="4"></project>"# as &[u8])
                .unwrap();
        let ensure = EnsureChildren {
            parent: "project/component[@name='Foo']/list".to_string(),
            match_by: "@id".to_string(),
            fragments: vec![
                r#"<Item id="a"/>"#.to_string(),
                r#"<Item id="b"/>"#.to_string(),
            ],
        };
        apply_ensure_children(&mut root, &ensure).unwrap();
        apply_ensure_children(&mut root, &ensure).unwrap();
        let list = find_child(find_child(&root, "component").unwrap(), "list").unwrap();
        let items: Vec<&Element> = list
            .children
            .iter()
            .filter_map(|n| match n {
                XMLNode::Element(e) => Some(e),
                _ => None,
            })
            .collect();
        assert_eq!(items.len(), 2);
    }

    #[test]
    fn ensure_children_appends_to_existing_list() {
        let mut root = Element::parse(
            br##"<?xml version="1.0"?>
<project version="4">
  <component name="Foo">
    <list>
      <Item id="existing"/>
    </list>
  </component>
</project>"## as &[u8],
        )
        .unwrap();
        let ensure = EnsureChildren {
            parent: "project/component[@name='Foo']/list".to_string(),
            match_by: "@id".to_string(),
            fragments: vec![
                r#"<Item id="existing"/>"#.to_string(),
                r#"<Item id="new"/>"#.to_string(),
            ],
        };
        apply_ensure_children(&mut root, &ensure).unwrap();
        let list = find_child(find_child(&root, "component").unwrap(), "list").unwrap();
        let ids: Vec<String> = list
            .children
            .iter()
            .filter_map(|n| match n {
                XMLNode::Element(e) => e.attributes.get("id").cloned(),
                _ => None,
            })
            .collect();
        assert_eq!(ids, vec!["existing", "new"]);
    }

    #[test]
    fn rejects_path_root_mismatch() {
        let mut root =
            Element::parse(br#"<?xml version="1.0"?><project version="4"></project>"# as &[u8])
                .unwrap();
        let ensure = EnsureChildren {
            parent: "module/list".to_string(),
            match_by: "@id".to_string(),
            fragments: vec![],
        };
        assert!(apply_ensure_children(&mut root, &ensure).is_err());
    }

    fn find_child<'a>(parent: &'a Element, tag: &str) -> Option<&'a Element> {
        parent.children.iter().find_map(|n| match n {
            XMLNode::Element(e) if e.name == tag => Some(e),
            _ => None,
        })
    }
}
