//! Obsidian syntax that `pulldown-cmark` does not know: `%%` comments,
//! wikilinks, embeds and tags (docs/design/0004, sections 3–5).

use std::ops::Range;

use super::{Excluded, LineIndex, is_escaped, split_wikilink};
use crate::model::{Link, LinkKind, Tag, TagSource};

/// `%%…%%` comments outside code, on one line or across lines. An unclosed
/// `%%` is not a comment.
pub(super) fn comments(text: &str, body_start: usize, code: &[Range<usize>]) -> Vec<Range<usize>> {
    let in_code = |p: usize| code.iter().any(|r| r.contains(&p));
    let marks: Vec<usize> = text[body_start..]
        .match_indices("%%")
        .map(|(i, _)| body_start + i)
        .filter(|&p| !in_code(p) && !is_escaped(text, p))
        .collect();
    // `match_indices` does not overlap, so `%%%%` gives two marks.
    marks
        .as_chunks::<2>()
        .0
        .iter()
        .map(|[open, close]| *open..close + 2)
        .collect()
}

/// Wikilinks and embeds outside excluded ranges.
pub(super) fn wikilinks(
    text: &str,
    body_start: usize,
    excluded: &Excluded,
    lines: &LineIndex,
) -> Vec<Link> {
    let mut out = Vec::new();
    let mut i = body_start;
    while let Some(off) = text[i..].find("[[") {
        let p = i + off;
        if let Some(end) = excluded.skip(p) {
            i = end;
            continue;
        }
        i = p + 2;
        if is_escaped(text, p) {
            continue;
        }
        let line_end = text[p..].find('\n').map_or(text.len(), |e| p + e);
        let Some(close) = text[p + 2..line_end].find("]]") else {
            continue;
        };
        let inner = &text[p + 2..p + 2 + close];
        if inner.contains("[[") {
            continue;
        }
        let Some(parts) = split_wikilink(inner) else {
            continue;
        };
        let embed = p > body_start && text.as_bytes()[p - 1] == b'!' && !is_escaped(text, p - 1);
        let start = if embed { p - 1 } else { p };
        let end = p + 2 + close + 2;
        if excluded.overlaps(start..end) {
            continue;
        }
        out.push(Link {
            kind: if embed {
                LinkKind::Embed
            } else {
                LinkKind::Wiki
            },
            raw: text[start..end].to_owned(),
            target: parts.target,
            subpath: parts.subpath,
            display: parts.display,
            line: lines.line_of(start),
            byte_start: start,
            byte_end: end,
        });
        i = end;
    }
    out
}

fn is_tag_char(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '_' | '-' | '/')
}

/// Inline `#tags`: after a line start or whitespace, at least one character
/// that is not a digit.
pub(super) fn tags(
    text: &str,
    body_start: usize,
    excluded: &Excluded,
    lines: &LineIndex,
) -> Vec<Tag> {
    let mut out = Vec::new();
    for (off, _) in text[body_start..].match_indices('#') {
        let p = body_start + off;
        if excluded.contains(p) {
            continue;
        }
        let after_space = p == body_start
            || text[..p]
                .chars()
                .next_back()
                .is_some_and(char::is_whitespace);
        if !after_space {
            continue;
        }
        let name: String = text[p + 1..]
            .chars()
            .take_while(|&c| is_tag_char(c))
            .collect();
        if name.is_empty() || name.chars().all(|c| c.is_ascii_digit()) {
            continue;
        }
        out.push(Tag {
            tag: name.to_lowercase(),
            display: name,
            source: TagSource::Inline,
            line: Some(lines.line_of(p)),
        });
    }
    out
}
