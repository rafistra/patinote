//! Markdown structure from `pulldown-cmark`: headings, block ID candidates,
//! Markdown links and images, and the code and math ranges that are not read.

use std::ops::Range;

use pulldown_cmark::{Event, LinkType, Options, Parser, Tag};

use crate::model::{Heading, Link, LinkKind};

/// What the Markdown pass found. Lines are filled in by the caller.
pub(super) struct Structure {
    /// Code blocks, inline code and math.
    pub excluded: Vec<Range<usize>>,
    pub headings: Vec<Heading>,
    /// Block IDs and the offset of their `^`.
    pub blocks: Vec<(String, usize)>,
    pub links: Vec<Link>,
}

pub(super) fn scan(text: &str, body_start: usize) -> Structure {
    let body = &text[body_start..];
    let mut s = Structure {
        excluded: Vec::new(),
        headings: Vec::new(),
        blocks: Vec::new(),
        links: Vec::new(),
    };
    let options = Options::ENABLE_MATH | Options::ENABLE_TABLES;
    for (event, range) in Parser::new_ext(body, options).into_offset_iter() {
        let abs = range.start + body_start..range.end + body_start;
        match event {
            Event::Start(Tag::CodeBlock(_))
            | Event::Code(_)
            | Event::InlineMath(_)
            | Event::DisplayMath(_) => s.excluded.push(abs),
            Event::Start(Tag::Heading { level, .. }) => {
                s.headings.push(Heading {
                    level: level as u8,
                    text: heading_text(&text[abs.clone()]),
                    line: 0,
                    byte_start: abs.start,
                });
            }
            Event::Start(Tag::Paragraph | Tag::Item) => {
                if let Some(block) = block_id(text, abs) {
                    s.blocks.push(block);
                }
            }
            Event::Start(Tag::Link {
                link_type: LinkType::Inline,
                dest_url,
                ..
            }) => {
                if let Some(link) = markdown_link(text, abs, &dest_url, LinkKind::Markdown) {
                    s.links.push(link);
                }
            }
            Event::Start(Tag::Image {
                link_type: LinkType::Inline,
                dest_url,
                ..
            }) => {
                if let Some(link) = markdown_link(text, abs, &dest_url, LinkKind::Embed) {
                    s.links.push(link);
                }
            }
            _ => {}
        }
    }
    s.blocks.sort_by_key(|b| b.1);
    s.blocks.dedup_by_key(|b| b.1);
    s
}

/// Heading text without `#` markers (ATX) or the underline (setext).
fn heading_text(source: &str) -> String {
    let trimmed = source.trim_start();
    if let Some(rest) = trimmed.strip_prefix('#') {
        let rest = rest.trim_start_matches('#');
        let line = rest.lines().next().unwrap_or("").trim();
        // A closing sequence of `#` is dropped when preceded by a space.
        let without_close = line.trim_end_matches('#');
        if without_close.len() < line.len()
            && (without_close.is_empty() || without_close.ends_with([' ', '\t']))
        {
            return without_close.trim().to_owned();
        }
        return line.to_owned();
    }
    let lines: Vec<&str> = source.lines().collect();
    let content = match lines.split_last() {
        Some((_, rest)) if !rest.is_empty() => rest,
        _ => &lines[..],
    };
    content
        .iter()
        .map(|l| l.trim())
        .collect::<Vec<_>>()
        .join(" ")
}

/// A `^id` at the end of the last line of a paragraph or list item.
fn block_id(text: &str, range: Range<usize>) -> Option<(String, usize)> {
    let block = text[range.clone()].trim_end();
    let last_start = block.rfind('\n').map_or(0, |i| i + 1);
    let last = &block[last_start..];
    let caret = last.rfind('^')?;
    let id = &last[caret + 1..];
    let before = last[..caret].chars().next_back();
    let valid = !id.is_empty()
        && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        && before.is_some_and(char::is_whitespace);
    valid.then(|| (id.to_owned(), range.start + last_start + caret))
}

/// A Markdown link or image whose destination is a relative path.
fn markdown_link(text: &str, range: Range<usize>, dest: &str, kind: LinkKind) -> Option<Link> {
    let raw = &text[range.clone()];
    if dest.is_empty() || has_scheme(dest) {
        return None;
    }
    let open = if kind == LinkKind::Embed { 2 } else { 1 };
    let close = raw.rfind("](")?;
    let display = raw
        .get(open..close)
        .filter(|d| !d.is_empty())
        .map(str::to_owned);
    let (path, subpath) = match dest.find('#') {
        Some(i) => (&dest[..i], Some(&dest[i + 1..])),
        None => (dest, None),
    };
    Some(Link {
        kind,
        raw: raw.to_owned(),
        target: percent_decode(path),
        subpath: subpath.filter(|s| !s.is_empty()).map(percent_decode),
        display,
        line: 0,
        byte_start: range.start,
        byte_end: range.end,
    })
}

/// `https:`, `mailto:` and other URL schemes (RFC 3986: a letter, then
/// letters, digits, `+`, `-` or `.`, then `:`).
fn has_scheme(dest: &str) -> bool {
    match dest.find(':') {
        Some(i) if i > 0 => {
            let scheme = &dest[..i];
            scheme.starts_with(|c: char| c.is_ascii_alphabetic())
                && scheme
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
        }
        _ => false,
    }
}

/// Decodes `%XX` escapes. Invalid escapes and invalid UTF-8 keep the input.
fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3])
                .ok()
                .and_then(|h| u8::from_str_radix(h, 16).ok());
            if let Some(b) = hex {
                out.push(b);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8(out).unwrap_or_else(|_| s.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heading_texts() {
        assert_eq!(heading_text("## Dua ##"), "Dua");
        assert_eq!(heading_text("# C#"), "C#");
        assert_eq!(heading_text("Judul\n====="), "Judul");
        assert_eq!(heading_text("# Judul\r\n"), "Judul");
    }

    #[test]
    fn schemes_and_decoding() {
        assert!(has_scheme("https://x.y"));
        assert!(has_scheme("mailto:a@b"));
        assert!(!has_scheme("Notes/a:b.md"));
        assert!(!has_scheme("Catatan.md"));
        assert_eq!(percent_decode("A%20B%2"), "A B%2");
        assert_eq!(percent_decode("%C3%A9"), "é");
    }
}
