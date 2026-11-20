//! Note parser (PRD F-02). The rules are in
//! docs/design/0004-korpus-uji-parser.md and the golden corpus in
//! `tests/golden/` is the reference.
//!
//! Parsing never fails: problems become [`Warning`]s. Byte offsets refer to
//! the text exactly as given, so a leading BOM counts.

mod frontmatter;
mod inline;
mod markdown;

use std::ops::Range;

use crate::model::{Heading, ParsedNote};

/// Parses one note.
pub fn parse(text: &str) -> ParsedNote {
    let lines = LineIndex::new(text);
    let bom = if text.starts_with('\u{feff}') {
        '\u{feff}'.len_utf8()
    } else {
        0
    };
    let mut note = ParsedNote::default();

    let body_start = match frontmatter::split(text, bom) {
        Some(fm) => {
            frontmatter::read(text, &fm, &lines, &mut note);
            fm.body_start
        }
        None => bom,
    };

    let structure = markdown::scan(text, body_start);
    let mut excluded = structure.excluded.clone();
    excluded.extend(inline::comments(text, body_start, &structure.excluded));
    let excluded = Excluded::new(excluded);

    let mut body_links = inline::wikilinks(text, body_start, &excluded, &lines);
    body_links.extend(
        structure
            .links
            .into_iter()
            .filter(|l| !excluded.overlaps(l.byte_start..l.byte_end))
            .map(|mut l| {
                l.line = lines.line_of(l.byte_start);
                l
            }),
    );
    body_links.sort_by_key(|l| l.byte_start);
    note.links.extend(body_links);

    note.tags
        .extend(inline::tags(text, body_start, &excluded, &lines));

    for h in structure.headings {
        if excluded.contains(h.byte_start) {
            continue;
        }
        note.headings.push(Heading {
            line: lines.line_of(h.byte_start),
            ..h
        });
    }
    if note.title.is_none() {
        note.title = note
            .headings
            .iter()
            .find(|h| h.level == 1)
            .map(|h| h.text.clone());
    }

    for (id, offset) in structure.blocks {
        if !excluded.contains(offset) {
            note.blocks.push(crate::model::Block {
                block_id: id,
                line: lines.line_of(offset),
            });
        }
    }
    note
}

/// Maps byte offsets to 1-based line numbers.
pub(crate) struct LineIndex {
    newlines: Vec<usize>,
}

impl LineIndex {
    fn new(text: &str) -> Self {
        Self {
            newlines: text
                .bytes()
                .enumerate()
                .filter(|&(_, b)| b == b'\n')
                .map(|(i, _)| i)
                .collect(),
        }
    }

    /// The 1-based line containing `offset`.
    pub(crate) fn line_of(&self, offset: usize) -> usize {
        self.newlines.partition_point(|&n| n < offset) + 1
    }
}

/// Byte ranges whose links and tags are not read (code, math, comments).
pub(crate) struct Excluded {
    ranges: Vec<Range<usize>>,
}

impl Excluded {
    fn new(mut ranges: Vec<Range<usize>>) -> Self {
        ranges.sort_by_key(|r| r.start);
        Self { ranges }
    }

    /// Whether `offset` lies in an excluded range.
    pub(crate) fn contains(&self, offset: usize) -> bool {
        self.ranges.iter().any(|r| r.contains(&offset))
    }

    /// Whether `range` touches an excluded range.
    pub(crate) fn overlaps(&self, range: Range<usize>) -> bool {
        self.ranges
            .iter()
            .any(|r| r.start < range.end && range.start < r.end)
    }

    /// The end of the excluded range containing `offset`, if any.
    pub(crate) fn skip(&self, offset: usize) -> Option<usize> {
        self.ranges
            .iter()
            .find(|r| r.contains(&offset))
            .map(|r| r.end)
    }
}

/// Whether the character at `offset` is escaped by an odd number of backslashes.
pub(crate) fn is_escaped(text: &str, offset: usize) -> bool {
    text.as_bytes()[..offset]
        .iter()
        .rev()
        .take_while(|&&b| b == b'\\')
        .count()
        % 2
        == 1
}

/// The parts of a wikilink's inner text (`target#subpath|display`).
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct WikiParts {
    pub target: String,
    pub subpath: Option<String>,
    pub display: Option<String>,
}

/// Splits the text between `[[` and `]]`. Returns `None` when there is neither
/// a target nor a subpath. In tables the pipe is written `\|`; the backslash
/// is dropped from the target.
pub(crate) fn split_wikilink(inner: &str) -> Option<WikiParts> {
    let (link, display) = match inner.find('|') {
        Some(i) => {
            let link = inner[..i].strip_suffix('\\').unwrap_or(&inner[..i]);
            (link, Some(&inner[i + 1..]))
        }
        None => (inner, None),
    };
    let (target, subpath) = match link.find('#') {
        Some(i) => (&link[..i], Some(&link[i + 1..])),
        None => (link, None),
    };
    let subpath = subpath.filter(|s| !s.is_empty());
    if target.is_empty() && subpath.is_none() {
        return None;
    }
    Some(WikiParts {
        target: target.to_owned(),
        subpath: subpath.map(str::to_owned),
        display: display.filter(|d| !d.is_empty()).map(str::to_owned),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_index() {
        let idx = LineIndex::new("a\nbc\n\nd");
        assert_eq!(
            [0, 1, 2, 4, 5, 6].map(|o| idx.line_of(o)),
            [1, 1, 2, 2, 3, 4]
        );
    }

    #[test]
    fn escapes() {
        assert!(is_escaped(r"a\[", 2));
        assert!(!is_escaped(r"a\\[", 3));
    }

    #[test]
    fn wikilink_parts() {
        let p = split_wikilink("A#B#C|text").unwrap();
        assert_eq!(
            (
                p.target.as_str(),
                p.subpath.as_deref(),
                p.display.as_deref()
            ),
            ("A", Some("B#C"), Some("text"))
        );
        let p = split_wikilink(r"A\|t").unwrap();
        assert_eq!((p.target.as_str(), p.display.as_deref()), ("A", Some("t")));
        assert_eq!(split_wikilink("#^id").unwrap().target, "");
        assert!(split_wikilink("").is_none());
        assert!(split_wikilink("|x").is_none());
    }
}
