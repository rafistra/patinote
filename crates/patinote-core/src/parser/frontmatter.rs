//! Frontmatter: detection, properties, tags, aliases, title and links
//! (docs/design/0004, sections 3–6).

use std::ops::Range;

use serde_yaml_ng::Value;

use super::{LineIndex, split_wikilink};
use crate::model::{
    Link, LinkKind, ParsedNote, Property, PropertyValue, Tag, TagSource, Warning, WarningCode,
};

/// Where the frontmatter is.
pub(super) struct Frontmatter {
    /// The YAML between the `---` lines.
    pub yaml: Range<usize>,
    /// First byte after the closing `---` line.
    pub body_start: usize,
}

fn line_ends(text: &str, from: usize) -> impl Iterator<Item = (usize, usize, usize)> + '_ {
    // (line start, content end without `\r\n`, next line start)
    let mut start = from;
    std::iter::from_fn(move || {
        if start >= text.len() {
            return None;
        }
        let next = text[start..]
            .find('\n')
            .map_or(text.len(), |i| start + i + 1);
        let mut end = next;
        if text[..end].ends_with('\n') {
            end -= 1;
        }
        if text[..end].ends_with('\r') {
            end -= 1;
        }
        let item = (start, end, next);
        start = next;
        Some(item)
    })
}

/// Finds frontmatter: the first line (after any BOM) is exactly `---` and a
/// later line is exactly `---`.
pub(super) fn split(text: &str, bom: usize) -> Option<Frontmatter> {
    let mut lines = line_ends(text, bom);
    let (start, end, yaml_start) = lines.next()?;
    if &text[start..end] != "---" {
        return None;
    }
    lines
        .find(|&(s, e, _)| &text[s..e] == "---")
        .map(|(s, _, next)| Frontmatter {
            yaml: yaml_start..s,
            body_start: next,
        })
}

/// Reads the frontmatter into `note`.
pub(super) fn read(text: &str, fm: &Frontmatter, lines: &LineIndex, note: &mut ParsedNote) {
    let yaml = &text[fm.yaml.clone()];
    if yaml.trim().is_empty() {
        return;
    }
    let map = match serde_yaml_ng::from_str::<Value>(yaml) {
        Ok(Value::Mapping(map)) => map,
        Ok(Value::Null) => return,
        Ok(_) | Err(_) => {
            note.warnings.push(Warning {
                code: WarningCode::FrontmatterInvalid,
            });
            return;
        }
    };

    let mut cursor = fm.yaml.start;
    for (key, value) in &map {
        let Some(key) = key_string(key) else { continue };
        let value = untag(value);

        match value {
            Value::Sequence(items) => {
                for (idx, item) in items.iter().enumerate() {
                    if let Some(v) = scalar(untag(item)) {
                        push_link(text, fm, &mut cursor, &v, lines, note);
                        note.properties.push(Property {
                            key: key.clone(),
                            idx,
                            value: v,
                        });
                    }
                }
            }
            other => {
                if let Some(v) = scalar(other) {
                    push_link(text, fm, &mut cursor, &v, lines, note);
                    note.properties.push(Property {
                        key: key.clone(),
                        idx: 0,
                        value: v,
                    });
                }
            }
        }

        match key.as_str() {
            "tags" | "tag" => {
                for t in list(value) {
                    let t = t.trim_start_matches('#');
                    if !t.is_empty() && !t.chars().all(|c| c.is_ascii_digit()) {
                        note.tags.push(Tag {
                            tag: t.to_lowercase(),
                            display: t.to_owned(),
                            source: TagSource::Frontmatter,
                            line: None,
                        });
                    }
                }
            }
            "aliases" | "alias" => note.aliases.extend(list(value)),
            "title" => {
                if let Value::String(s) = value {
                    note.title = Some(s.clone());
                }
            }
            _ => {}
        }
    }
}

fn untag(v: &Value) -> &Value {
    match v {
        Value::Tagged(t) => untag(&t.value),
        other => other,
    }
}

fn key_string(key: &Value) -> Option<String> {
    match untag(key) {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

/// A scalar YAML value as a property value; `None` for lists and maps.
fn scalar(v: &Value) -> Option<PropertyValue> {
    Some(match v {
        Value::Null => PropertyValue::Null,
        Value::Bool(b) => PropertyValue::Bool(*b),
        Value::Number(n) => match n.as_i64() {
            Some(i) => PropertyValue::Integer(i),
            None => PropertyValue::Float(n.as_f64()?),
        },
        Value::String(s) if is_date(s) => PropertyValue::Date(s.clone()),
        Value::String(s) if is_datetime(s) => PropertyValue::DateTime(s.clone()),
        Value::String(s) => PropertyValue::Text(s.clone()),
        Value::Sequence(_) | Value::Mapping(_) | Value::Tagged(_) => return None,
    })
}

/// Text items of a list, or of a comma-separated string.
fn list(v: &Value) -> Vec<String> {
    let item = |v: &Value| match untag(v) {
        Value::String(s) => Some(s.trim().to_owned()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    };
    match v {
        Value::Sequence(items) => items
            .iter()
            .filter_map(item)
            .filter(|s| !s.is_empty())
            .collect(),
        Value::String(s) => s
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .collect(),
        other => item(other).into_iter().collect(),
    }
}

fn is_date(s: &str) -> bool {
    is_date_bytes(s.as_bytes())
}

/// Works on bytes so that non-ASCII text never splits a character.
fn is_date_bytes(b: &[u8]) -> bool {
    b.len() == 10
        && b.iter().enumerate().all(|(i, c)| {
            if i == 4 || i == 7 {
                *c == b'-'
            } else {
                c.is_ascii_digit()
            }
        })
}

fn is_datetime(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() >= 16
        && is_date_bytes(&b[..10])
        && b[10] == b'T'
        && b[11].is_ascii_digit()
        && b[12].is_ascii_digit()
        && b[13] == b':'
        && b[14].is_ascii_digit()
        && b[15].is_ascii_digit()
        && b[16..]
            .iter()
            .all(|c| c.is_ascii_digit() || b":.+-Z".contains(c))
}

/// Adds a frontmatter link when the whole value is one wikilink.
fn push_link(
    text: &str,
    fm: &Frontmatter,
    cursor: &mut usize,
    value: &PropertyValue,
    lines: &LineIndex,
    note: &mut ParsedNote,
) {
    let PropertyValue::Text(s) = value else {
        return;
    };
    let raw = s.trim();
    let Some(inner) = raw.strip_prefix("[[").and_then(|r| r.strip_suffix("]]")) else {
        return;
    };
    if inner.contains("[[") || inner.contains("]]") {
        return;
    }
    let Some(parts) = split_wikilink(inner) else {
        return;
    };
    // The value as YAML returns it is located in the source to get its position.
    let Some(found) = text[*cursor..fm.yaml.end].find(raw) else {
        return;
    };
    let start = *cursor + found;
    *cursor = start + raw.len();
    note.links.push(Link {
        kind: LinkKind::Frontmatter,
        raw: raw.to_owned(),
        target: parts.target,
        subpath: parts.subpath,
        display: parts.display,
        line: lines.line_of(start),
        byte_start: start,
        byte_end: start + raw.len(),
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates() {
        assert!(is_date("2026-11-19"));
        assert!(!is_date("2026-1-19"));
        assert!(is_datetime("2026-11-19T10:30"));
        assert!(is_datetime("2026-11-19T10:30:00+07:00"));
        assert!(!is_datetime("2026-11-19 10:30"));
        // Regression: non-ASCII text must not panic (found by the vaultgen cross-check).
        assert!(!is_datetime("Kabut naïf alias 84 dan lainnya"));
        assert!(!is_date("naïf café"));
    }

    #[test]
    fn split_requires_first_line() {
        assert!(split("---\na: 1\n---\nbody", 0).is_some());
        assert!(split("\n---\na: 1\n---\n", 0).is_none());
        assert!(split("---\na: 1\n", 0).is_none());
        let fm = split("---\r\na: 1\r\n---\r\nx", 0).unwrap();
        assert_eq!(fm.body_start, 16);
    }
}
