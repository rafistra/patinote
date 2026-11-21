//! Per-line Markdown coloring in the editor (F-05; 0003 section 3.4).
//! Characters are only colored: nothing is hidden, dimmed or resized.
//! Frontmatter and fenced code need the lines before them, so
//! [`line_kinds`] classifies the whole note first.

use iced::Color;

use crate::theme::tokens::*;

/// What kind of line this is, which needs document context.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineKind {
    Frontmatter,
    Fence,
    Text,
}

/// Classifies every line of a document.
pub fn line_kinds(lines: &[String]) -> Vec<LineKind> {
    let mut kinds = Vec::with_capacity(lines.len());
    // Frontmatter needs a closing `---`, as the parser reads it.
    let front_end = (lines.first().is_some_and(|l| l == "---"))
        .then(|| lines.iter().skip(1).position(|l| l == "---"))
        .flatten()
        .map(|i| i + 1);
    let mut open: Option<(char, usize)> = None;
    for (i, l) in lines.iter().enumerate() {
        if front_end.is_some_and(|end| i <= end) {
            kinds.push(LineKind::Frontmatter);
            continue;
        }
        let fence = fence_of(l);
        match (open, fence) {
            (None, Some((c, n, _))) => {
                open = Some((c, n));
                kinds.push(LineKind::Fence);
            }
            (Some(o), f) => {
                // Closes with the same character, at least as long, and
                // nothing after it (CommonMark).
                if f.is_some_and(|(c, n, rest)| c == o.0 && n >= o.1 && rest.is_empty()) {
                    open = None;
                }
                kinds.push(LineKind::Fence);
            }
            (None, None) => kinds.push(LineKind::Text),
        }
    }
    kinds
}

/// A fence line: its character, how many, and what follows (trimmed).
fn fence_of(line: &str) -> Option<(char, usize, &str)> {
    let t = line.trim_start();
    let c = t.chars().next().filter(|c| matches!(c, '`' | '~'))?;
    let n = t.chars().take_while(|&x| x == c).count();
    let rest = t[n..].trim();
    // A backtick fence's info string has no backticks.
    (n >= 3 && !(c == '`' && rest.contains('`'))).then_some((c, n, rest))
}

/// A colored run of characters `start..end`.
#[derive(Debug, Clone, PartialEq)]
pub struct Seg {
    pub start: usize,
    pub end: usize,
    pub color: Color,
    pub bold: bool,
    /// For wikilinks, the target name, used to mark ambiguous links.
    pub link_target: Option<String>,
}

fn seg(start: usize, end: usize, color: Color) -> Seg {
    Seg {
        start,
        end,
        color,
        bold: false,
        link_target: None,
    }
}

fn is_tag_char(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '_' | '-' | '/')
}

/// Colored runs for one line. Characters not covered use `TEXT_PRIMARY`.
pub fn segments(text: &str, kind: LineKind) -> Vec<Seg> {
    let chars: Vec<char> = text.chars().collect();
    let n = chars.len();
    match kind {
        LineKind::Frontmatter => return vec![seg(0, n, SYNTAX_FRONTMATTER)],
        LineKind::Fence => return vec![seg(0, n, SYNTAX_CODE)],
        LineKind::Text => {}
    }
    let hashes = chars.iter().take_while(|&&c| c == '#').count();
    if (1..=6).contains(&hashes) && chars.get(hashes) == Some(&' ') {
        return vec![Seg {
            bold: true,
            ..seg(0, n, SYNTAX_HEADING)
        }];
    }

    let mut out = Vec::new();
    let mut i = 0;
    // Line markers.
    let indent = chars.iter().take_while(|c| c.is_whitespace()).count();
    let rest: String = chars[indent..].iter().collect();
    if rest.starts_with('>') {
        let end = if rest.starts_with("> [!") {
            rest.find(']').map_or(1, |e| e + 1)
        } else {
            1
        };
        out.push(seg(indent, indent + end, SYNTAX_QUOTE));
        i = indent + end;
    } else if rest.starts_with("- [x] ") || rest.starts_with("- [X] ") {
        out.push(seg(indent, indent + 5, SYNTAX_LIST));
        out.push(seg(indent + 5, n, SYNTAX_TASK_DONE));
        return out;
    } else if rest.starts_with("- [ ] ") {
        out.push(seg(indent, indent + 5, SYNTAX_LIST));
        i = indent + 5;
    } else if rest.starts_with("- ") || rest.starts_with("* ") {
        out.push(seg(indent, indent + 1, SYNTAX_LIST));
        i = indent + 1;
    }

    while i < n {
        let c = chars[i];
        if c == '`'
            && let Some(off) = chars[i + 1..].iter().position(|&d| d == '`')
        {
            let end = i + off + 2;
            out.push(seg(i, end, SYNTAX_CODE));
            i = end;
            continue;
        }
        if c == '['
            && chars.get(i + 1) == Some(&'[')
            && let Some(off) =
                (i + 2..n.saturating_sub(1)).find(|&j| chars[j] == ']' && chars[j + 1] == ']')
        {
            let end = off + 2;
            let inner: String = chars[i + 2..off].iter().collect();
            let target = inner.split(['|', '#']).next().unwrap_or("").to_owned();
            let start = if i > 0 && chars[i - 1] == '!' {
                i - 1
            } else {
                i
            };
            out.push(Seg {
                link_target: Some(target),
                ..seg(start, end, SYNTAX_LINK)
            });
            i = end;
            continue;
        }
        if c == '['
            && let Some(close) = chars[i..].iter().position(|&d| d == ']').map(|p| p + i)
            && chars.get(close + 1) == Some(&'(')
            && let Some(end) = chars[close..]
                .iter()
                .position(|&d| d == ')')
                .map(|p| p + close + 1)
        {
            out.push(seg(i, end, SYNTAX_LINK));
            i = end;
            continue;
        }
        if c == '#'
            && (i == 0 || chars[i - 1].is_whitespace())
            && chars.get(i + 1).is_some_and(|&d| is_tag_char(d))
        {
            let end = (i + 1..n).find(|&j| !is_tag_char(chars[j])).unwrap_or(n);
            out.push(seg(i, end, SYNTAX_TAG));
            i = end;
            continue;
        }
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heading_link_tag_code() {
        assert_eq!(
            segments("## Title", LineKind::Text)[0].color,
            SYNTAX_HEADING
        );
        let s = segments("see [[Rapat]] and #tag and `x`", LineKind::Text);
        assert_eq!(s.len(), 3);
        assert_eq!(s[0].link_target.as_deref(), Some("Rapat"));
        assert_eq!((s[1].start, s[1].end), (18, 22));
        assert_eq!(s[2].color, SYNTAX_CODE);
    }

    fn kinds_of(lines: &[&str]) -> Vec<LineKind> {
        let lines: Vec<String> = lines.iter().map(|s| s.to_string()).collect();
        line_kinds(&lines)
    }

    /// Like the parser: frontmatter needs its closing `---`.
    #[test]
    fn a_rule_on_the_first_line_is_not_frontmatter() {
        use LineKind::*;
        assert_eq!(kinds_of(&["---", "# Judul", "teks"]), [Text, Text, Text]);
    }

    /// A fence closes only with its own character, at least as long, and
    /// nothing after it.
    #[test]
    fn fences_close_like_commonmark() {
        use LineKind::*;
        assert_eq!(
            kinds_of(&["```md", "~~~", "```js", "# bukan judul", "```", "# Judul"]),
            [Fence, Fence, Fence, Fence, Fence, Text]
        );
        assert_eq!(
            kinds_of(&["````", "```", "````", "t"]),
            [Fence, Fence, Fence, Text]
        );
    }

    #[test]
    fn kinds() {
        let lines: Vec<String> = ["---", "a: b", "---", "```", "x", "```", "t"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        use LineKind::*;
        assert_eq!(
            line_kinds(&lines),
            [
                Frontmatter,
                Frontmatter,
                Frontmatter,
                Fence,
                Fence,
                Fence,
                Text
            ]
        );
    }
}
