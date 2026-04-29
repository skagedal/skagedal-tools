//! `{{ name }}` substitution with strict undefined.

use std::collections::BTreeMap;

use thiserror::Error;

#[derive(Debug, Error)]
#[error("{0}")]
pub struct TemplateError(pub String);

/// Substitute `{{ name }}` placeholders. Whitespace inside the braces is
/// allowed. Returns an error if a referenced placeholder has no value.
pub fn expand_template(
    template: &str,
    vars: &BTreeMap<&str, Option<&str>>,
) -> Result<String, TemplateError> {
    let mut out = String::with_capacity(template.len());
    let bytes = template.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        if i + 1 < bytes.len() && bytes[i] == b'{' && bytes[i + 1] == b'{' {
            // Find the closing }}.
            let body_start = i + 2;
            let Some(close_rel) = template[body_start..].find("}}") else {
                out.push_str(&template[i..]);
                return Ok(out);
            };
            let close_abs = body_start + close_rel;
            let raw = &template[body_start..close_abs];
            let name = raw.trim();
            if !is_identifier(name) {
                out.push_str(&template[i..close_abs + 2]);
                i = close_abs + 2;
                continue;
            }
            match vars.get(name) {
                Some(Some(value)) => out.push_str(value),
                _ => {
                    return Err(TemplateError(format!(
                        "template \"{template}\" uses {{{{ {name} }}}} but no {name} was supplied"
                    )));
                }
            }
            i = close_abs + 2;
        } else {
            out.push(bytes[i] as char);
            i += 1;
        }
    }

    Ok(out)
}

fn is_identifier(s: &str) -> bool {
    let mut iter = s.bytes();
    let Some(first) = iter.next() else {
        return false;
    };
    if !(first.is_ascii_alphabetic() || first == b'_') {
        return false;
    }
    iter.all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vars(pairs: &[(&'static str, &'static str)]) -> BTreeMap<&'static str, Option<&'static str>> {
        pairs.iter().map(|(k, v)| (*k, Some(*v))).collect()
    }

    #[test]
    fn substitutes_with_whitespace_tolerance() {
        assert_eq!(
            expand_template("/prod/team", &vars(&[("env", "x")])).unwrap(),
            "/prod/team"
        );
        assert_eq!(
            expand_template("/{{ env }}/team", &vars(&[("env", "systest")])).unwrap(),
            "/systest/team"
        );
        assert_eq!(
            expand_template("/{{env}}/{{  env  }}", &vars(&[("env", "uat")])).unwrap(),
            "/uat/uat"
        );
        assert_eq!(
            expand_template("/{{ env }}/{{ app }}", &vars(&[("env", "prod"), ("app", "svc")])).unwrap(),
            "/prod/svc"
        );
    }

    #[test]
    fn errors_when_placeholder_missing() {
        assert!(expand_template("/{{ env }}/team", &vars(&[])).is_err());
        assert!(expand_template("filter app = '{{ app }}'", &vars(&[("env", "prod")])).is_err());
    }
}
