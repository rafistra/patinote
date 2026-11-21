//! Links in the editor (F-08), as pure functions over a line of text: the
//! link under the cursor, what `[[` autocomplete should suggest, and the
//! name a link should use. Columns are character indices.

/// A link found under the cursor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Link {
    /// `[[target#subpath|display]]` or `![[…]]`.
    Wiki {
        target: String,
        /// Heading or `^block` after `#`.
        subpath: Option<String>,
    },
    /// `[text](target)` where the target is a file in the vault.
    Markdown { target: String },
    /// A web address (`gx`).
    Url(String),
}

/// The link that covers column `col`, and the column it starts at.
pub fn link_at(line: &str, col: usize) -> Option<(Link, usize)> {
    let chars: Vec<char> = line.chars().collect();
    let n = chars.len();
    let mut i = 0;
    while i < n {
        // Wikilinks and embeds.
        if chars[i] == '[' && chars.get(i + 1) == Some(&'[') {
            let start = if i > 0 && chars[i - 1] == '!' {
                i - 1
            } else {
                i
            };
            if let Some(close) =
                (i + 2..n.saturating_sub(1)).find(|&j| chars[j] == ']' && chars[j + 1] == ']')
            {
                let end = close + 2;
                if (start..end).contains(&col) {
                    let inner: String = chars[i + 2..close].iter().collect();
                    let inner = inner.split('|').next().unwrap_or_default();
                    let (target, subpath) = match inner.split_once('#') {
                        Some((t, s)) => (t.trim().to_owned(), Some(s.trim().to_owned())),
                        None => (inner.trim().to_owned(), None),
                    };
                    return Some((Link::Wiki { target, subpath }, start));
                }
                i = end;
                continue;
            }
        }
        // Markdown links: [text](target).
        if chars[i] == '['
            && let Some(close) = (i + 1..n).find(|&j| chars[j] == ']')
            && chars.get(close + 1) == Some(&'(')
            && let Some(end) = (close + 2..n).find(|&j| chars[j] == ')')
        {
            if (i..=end).contains(&col) {
                let target: String = chars[close + 2..end].iter().collect();
                let target = target.trim().to_owned();
                let link = if is_url(&target) {
                    Link::Url(target)
                } else {
                    Link::Markdown {
                        target: percent_decode(&target),
                    }
                };
                return Some((link, i));
            }
            i = end + 1;
            continue;
        }
        // Bare web addresses.
        if (chars[i] == 'h' || chars[i] == 'w')
            && (i == 0 || chars[i - 1].is_whitespace() || chars[i - 1] == '(')
        {
            let rest: String = chars[i..].iter().take(8).collect();
            if rest.starts_with("http://")
                || rest.starts_with("https://")
                || rest.starts_with("www.")
            {
                let end = (i..n)
                    .find(|&j| chars[j].is_whitespace() || matches!(chars[j], ')' | '>' | '"'))
                    .unwrap_or(n);
                if (i..end).contains(&col) {
                    let url: String = chars[i..end].iter().collect();
                    let url = url.trim_end_matches(['.', ',', ';', ':']).to_owned();
                    return Some((Link::Url(url), i));
                }
                i = end;
                continue;
            }
        }
        i += 1;
    }
    None
}

fn is_url(s: &str) -> bool {
    s.starts_with("http://") || s.starts_with("https://") || s.starts_with("mailto:")
}

/// `%20` → space, for Markdown link targets.
fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%'
            && i + 2 < b.len()
            && let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16)
        {
            out.push(v);
            i += 3;
            continue;
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8(out).unwrap_or_else(|_| s.to_owned())
}

/// What `[[` autocomplete should suggest at the cursor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Suggest {
    /// Notes (and attachments after `![[`) matching `query`.
    Notes { query: String, embed: bool },
    /// Headings of `note` matching `query` (`[[note#`).
    Headings { note: String, query: String },
    /// Block IDs of `note` matching `query` (`[[note#^`).
    Blocks { note: String, query: String },
}

/// The autocomplete context at column `col` (the insert cursor), with the
/// column right after `[[` where the link's text starts. `None` outside a
/// link, and after `|` (free text, F-08).
pub fn suggest_at(line: &str, col: usize) -> Option<(Suggest, usize)> {
    let chars: Vec<char> = line.chars().collect();
    let col = col.min(chars.len());
    let before = &chars[..col];
    let open = (0..before.len().saturating_sub(1))
        .rev()
        .find(|&j| before[j] == '[' && before[j + 1] == '[')?;
    let text: String = before[open + 2..].iter().collect();
    if text.contains("]]") || text.contains('|') || text.contains('\n') {
        return None;
    }
    let embed = open > 0 && before[open - 1] == '!';
    let start = open + 2;
    let suggest = match text.split_once('#') {
        None => Suggest::Notes { query: text, embed },
        Some((note, rest)) => match rest.strip_prefix('^') {
            Some(q) => Suggest::Blocks {
                note: note.to_owned(),
                query: q.to_owned(),
            },
            None => Suggest::Headings {
                note: note.to_owned(),
                query: rest.to_owned(),
            },
        },
    };
    Some((suggest, start))
}

/// The name a link to `path` should use: the note's name when no other
/// note has it, else the shortest end of its path that is unique (F-08.3:
/// `Proyek/Rapat` when there are two "Rapat"). `.md` is left out.
pub fn link_name<'a>(path: &str, all: impl IntoIterator<Item = &'a str>) -> String {
    let strip = |p: &str| p.strip_suffix(".md").unwrap_or(p).to_owned();
    let own = strip(path);
    let segments: Vec<&str> = own.split('/').collect();
    let others: Vec<String> = all
        .into_iter()
        .filter(|p| *p != path)
        .map(|p| strip(p).to_lowercase())
        .collect();
    for k in 1..=segments.len() {
        let suffix = segments[segments.len() - k..].join("/");
        let lower = suffix.to_lowercase();
        let clash = others
            .iter()
            .any(|o| *o == lower || o.ends_with(&format!("/{lower}")));
        if !clash {
            return suffix;
        }
    }
    own
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wiki(target: &str, subpath: Option<&str>) -> Link {
        Link::Wiki {
            target: target.into(),
            subpath: subpath.map(Into::into),
        }
    }

    #[test]
    fn finds_the_link_under_the_cursor() {
        let line = "- Catatan dari [[Rapat]] dan [[Patinote#Tabel inti|tabel]] ![[gambar.png]]";
        assert_eq!(link_at(line, 15), Some((wiki("Rapat", None), 15)));
        assert_eq!(link_at(line, 23), Some((wiki("Rapat", None), 15)));
        assert_eq!(link_at(line, 24), None);
        assert_eq!(
            link_at(line, 35),
            Some((wiki("Patinote", Some("Tabel inti")), 29))
        );
        assert_eq!(
            link_at(line, 62).map(|l| l.0),
            Some(wiki("gambar.png", None))
        );
        assert_eq!(link_at("no links here", 3), None);
    }

    #[test]
    fn finds_markdown_links_and_urls() {
        let line = "Lihat [doc](Proyek/Desain%20indeks.md) dan https://example.org/a. Juga [web](https://x.y)";
        assert_eq!(
            link_at(line, 8).map(|l| l.0),
            Some(Link::Markdown {
                target: "Proyek/Desain indeks.md".into()
            })
        );
        assert_eq!(
            link_at(line, 50).map(|l| l.0),
            Some(Link::Url("https://example.org/a".into()))
        );
        assert_eq!(
            link_at(line, 75).map(|l| l.0),
            Some(Link::Url("https://x.y".into()))
        );
    }

    #[test]
    fn suggests_notes_headings_and_blocks() {
        assert_eq!(
            suggest_at("bahas di [[desa]] setelah", 15),
            Some((
                Suggest::Notes {
                    query: "desa".into(),
                    embed: false
                },
                11
            ))
        );
        assert_eq!(
            suggest_at("![[fo", 5),
            Some((
                Suggest::Notes {
                    query: "fo".into(),
                    embed: true
                },
                3
            ))
        );
        assert_eq!(
            suggest_at("[[Patinote#Tab", 14),
            Some((
                Suggest::Headings {
                    note: "Patinote".into(),
                    query: "Tab".into()
                },
                2
            ))
        );
        assert_eq!(
            suggest_at("[[Patinote#^ab", 14),
            Some((
                Suggest::Blocks {
                    note: "Patinote".into(),
                    query: "ab".into()
                },
                2
            ))
        );
        assert_eq!(suggest_at("[[Patinote|teks", 15), None, "free text after |");
        assert_eq!(
            suggest_at("[[Patinote]] lalu", 17),
            None,
            "after a closed link"
        );
        assert_eq!(suggest_at("tanpa link", 5), None);
    }

    #[test]
    fn f08_3_link_names_include_the_folder_only_when_needed() {
        let all = [
            "Proyek/Rapat.md",
            "Arsip/2025/Rapat.md",
            "Proyek/Patinote.md",
        ];
        assert_eq!(link_name("Proyek/Patinote.md", all), "Patinote");
        assert_eq!(link_name("Proyek/Rapat.md", all), "Proyek/Rapat");
        assert_eq!(link_name("Arsip/2025/Rapat.md", all), "2025/Rapat");
    }
}
