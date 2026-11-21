//! Reading mode's model (F-27): a note's Markdown turned into blocks to
//! render with Inter. Pure logic; the view is `screens/reading.rs`.
//!
//! Wikilinks, tables, task lists, strikethrough, math and frontmatter use
//! `pulldown-cmark`'s extensions; tags are found in text. Every link gets a
//! number in document order, for `Tab` / `Shift+Tab` (F-27).

use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, LinkType, Options, Parser, Tag, TagEnd};

/// A piece of text in a block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Inline {
    Text {
        text: String,
        bold: bool,
        italic: bool,
        strike: bool,
    },
    Code(String),
    /// A link: shown text, target as written, and its number.
    Link {
        text: String,
        target: String,
        markdown: bool,
        index: usize,
    },
    Tag(String),
    /// `$…$`, shown raw until Phase 6.
    Math(String),
}

/// One list item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub depth: usize,
    /// `Some(done)` for a task; with the 0-based source line to toggle.
    pub task: Option<(bool, usize)>,
    pub ordered: Option<u64>,
    pub inlines: Vec<Inline>,
}

/// A frontmatter value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    Text(String),
    Tag(String),
    /// A link to a note, with its number.
    Link {
        target: String,
        index: usize,
    },
}

/// One block of the rendered note.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Block {
    /// Frontmatter, shown as a small box at the top.
    Properties(Vec<(String, Vec<Value>)>),
    Heading {
        level: u8,
        inlines: Vec<Inline>,
        line: usize,
    },
    Paragraph {
        inlines: Vec<Inline>,
        line: usize,
    },
    List {
        items: Vec<Item>,
        line: usize,
    },
    /// A quote; a callout (`> [!note] Title`) has a title.
    Quote {
        title: Option<String>,
        body: Vec<Vec<Inline>>,
        line: usize,
    },
    Rule {
        line: usize,
    },
    Table {
        header: Vec<Vec<Inline>>,
        rows: Vec<Vec<Vec<Inline>>>,
        line: usize,
    },
    Code {
        text: String,
        line: usize,
    },
    /// `![[image.png]]` or `![](image.png)`.
    Image {
        target: String,
        line: usize,
    },
    /// `![[Note]]`: a link card until Phase 6.
    Embed {
        target: String,
        index: usize,
        line: usize,
    },
}

impl Block {
    /// The 0-based source line the block starts on.
    pub fn line(&self) -> usize {
        match self {
            Self::Properties(_) => 0,
            Self::Heading { line, .. }
            | Self::Paragraph { line, .. }
            | Self::List { line, .. }
            | Self::Quote { line, .. }
            | Self::Rule { line }
            | Self::Table { line, .. }
            | Self::Code { line, .. }
            | Self::Image { line, .. }
            | Self::Embed { line, .. } => *line,
        }
    }
}

/// A table being read: header cells, rows of cells, and its line.
type TableParts = (Vec<Vec<Inline>>, Vec<Vec<Vec<Inline>>>, usize);

/// A link target as written, in document order, for `Tab` and `Enter`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkTarget {
    /// For a Markdown link to a file, the decoded path alone; otherwise the
    /// target as written.
    pub target: String,
    pub markdown: bool,
    /// The decoded `#heading` of a Markdown link to a file.
    pub subpath: Option<String>,
}

/// The rendered note.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Document {
    pub blocks: Vec<Block>,
    pub links: Vec<LinkTarget>,
}

const IMAGE_EXTENSIONS: [&str; 7] = ["png", "jpg", "jpeg", "gif", "webp", "svg", "bmp"];

fn is_image(target: &str) -> bool {
    let t = target.split(['#', '|']).next().unwrap_or(target);
    t.rsplit_once('.')
        .is_some_and(|(_, ext)| IMAGE_EXTENSIONS.contains(&ext.to_lowercase().as_str()))
}

fn is_tag_char(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '_' | '-' | '/')
}

/// Splits plain text into text and `#tags`.
fn push_text(out: &mut Vec<Inline>, text: &str, bold: bool, italic: bool, strike: bool) {
    let chars: Vec<char> = text.chars().collect();
    let mut start = 0;
    let mut i = 0;
    while i < chars.len() {
        let tag_start = chars[i] == '#'
            && (i == 0 || chars[i - 1].is_whitespace())
            && chars
                .get(i + 1)
                .is_some_and(|c| is_tag_char(*c) && !c.is_numeric());
        if tag_start {
            let end = (i + 1..chars.len())
                .find(|&j| !is_tag_char(chars[j]))
                .unwrap_or(chars.len());
            if i > start {
                out.push(Inline::Text {
                    text: chars[start..i].iter().collect(),
                    bold,
                    italic,
                    strike,
                });
            }
            out.push(Inline::Tag(chars[i + 1..end].iter().collect()));
            start = end;
            i = end;
            continue;
        }
        i += 1;
    }
    if start < chars.len() {
        out.push(Inline::Text {
            text: chars[start..].iter().collect(),
            bold,
            italic,
            strike,
        });
    }
}

/// 0-based line of byte `offset`.
fn line_of(starts: &[usize], offset: usize) -> usize {
    starts.partition_point(|&s| s <= offset).saturating_sub(1)
}

/// Turns a note's text into blocks.
pub fn render(text: &str) -> Document {
    let parsed = patinote_core::parse(text);
    let starts: Vec<usize> = std::iter::once(0)
        .chain(text.match_indices('\n').map(|(i, _)| i + 1))
        .collect();
    let mut doc = Document::default();

    // Frontmatter, from the core parser's properties.
    if !parsed.properties.is_empty() {
        let mut props: Vec<(String, Vec<Value>)> = Vec::new();
        for p in &parsed.properties {
            let value = match &p.value {
                patinote_core::PropertyValue::Text(t) => {
                    if p.key.eq_ignore_ascii_case("tags") || p.key.eq_ignore_ascii_case("tag") {
                        Value::Tag(t.trim_start_matches('#').to_owned())
                    } else if let Some(inner) =
                        t.strip_prefix("[[").and_then(|t| t.strip_suffix("]]"))
                    {
                        let target = inner.split('|').next().unwrap_or(inner).to_owned();
                        doc.links.push(LinkTarget {
                            target: target.clone(),
                            markdown: false,
                            subpath: None,
                        });
                        Value::Link {
                            target,
                            index: doc.links.len() - 1,
                        }
                    } else {
                        Value::Text(t.clone())
                    }
                }
                patinote_core::PropertyValue::Integer(n) => Value::Text(n.to_string()),
                patinote_core::PropertyValue::Float(n) => Value::Text(n.to_string()),
                patinote_core::PropertyValue::Bool(b) => Value::Text(b.to_string()),
                patinote_core::PropertyValue::Date(d)
                | patinote_core::PropertyValue::DateTime(d) => Value::Text(d.clone()),
                patinote_core::PropertyValue::Null => Value::Text(String::new()),
            };
            match props.iter_mut().find(|(k, _)| *k == p.key) {
                Some((_, values)) => values.push(value),
                None => props.push((p.key.clone(), vec![value])),
            }
        }
        doc.blocks.push(Block::Properties(props));
    }

    let options = Options::ENABLE_TABLES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_YAML_STYLE_METADATA_BLOCKS
        | Options::ENABLE_MATH
        | Options::ENABLE_WIKILINKS;
    let mut inlines: Vec<Inline> = Vec::new();
    let (mut bold, mut italic, mut strike) = (false, false, false);
    let mut link: Option<(String, Option<String>, bool)> = None;
    let mut link_text = String::new();
    let mut image: Option<String> = None;
    let mut list: Vec<Item> = Vec::new();
    let mut list_line = 0;
    let mut depth = 0usize;
    let mut ordered: Vec<Option<u64>> = Vec::new();
    let mut task: Option<(bool, usize)> = None;
    let mut quote: Option<(Vec<Vec<Inline>>, usize)> = None;
    // How many links came before the quote, to number a callout's links.
    let mut quote_links = 0;
    let mut table: Option<TableParts> = None;
    let mut row: Vec<Vec<Inline>> = Vec::new();
    let mut in_head = false;
    let mut code: Option<(String, usize)> = None;
    let mut in_meta = false;
    let mut block_line = 0;

    let flush_paragraph = |inlines: &mut Vec<Inline>,
                           quote: &mut Option<(Vec<Vec<Inline>>, usize)>,
                           doc: &mut Document,
                           line: usize| {
        if inlines.is_empty() {
            return;
        }
        let taken = std::mem::take(inlines);
        match quote {
            Some((body, _)) => body.push(taken),
            None => doc.blocks.push(Block::Paragraph {
                inlines: taken,
                line,
            }),
        }
    };

    for (event, range) in Parser::new_ext(text, options).into_offset_iter() {
        let line = line_of(&starts, range.start);
        match event {
            Event::Start(Tag::MetadataBlock(_)) => in_meta = true,
            Event::End(TagEnd::MetadataBlock(_)) => in_meta = false,
            _ if in_meta => {}
            Event::Start(Tag::Paragraph) => block_line = line,
            Event::End(TagEnd::Paragraph) => {
                if list.is_empty() || depth == 0 {
                    flush_paragraph(&mut inlines, &mut quote, &mut doc, block_line);
                }
            }
            Event::Start(Tag::Heading { .. }) => block_line = line,
            Event::End(TagEnd::Heading(level)) => {
                let level = match level {
                    HeadingLevel::H1 => 1,
                    HeadingLevel::H2 => 2,
                    HeadingLevel::H3 => 3,
                    HeadingLevel::H4 => 4,
                    HeadingLevel::H5 => 5,
                    HeadingLevel::H6 => 6,
                };
                doc.blocks.push(Block::Heading {
                    level,
                    inlines: std::mem::take(&mut inlines),
                    line: block_line,
                });
            }
            Event::Start(Tag::Strong) => bold = true,
            Event::End(TagEnd::Strong) => bold = false,
            Event::Start(Tag::Emphasis) => italic = true,
            Event::End(TagEnd::Emphasis) => italic = false,
            Event::Start(Tag::Strikethrough) => strike = true,
            Event::End(TagEnd::Strikethrough) => strike = false,
            Event::Start(Tag::Link {
                link_type,
                dest_url,
                ..
            }) => {
                let markdown = !matches!(link_type, LinkType::WikiLink { .. });
                let (target, subpath) = destination(&dest_url, markdown);
                link = Some((target, subpath, markdown));
                link_text.clear();
            }
            Event::End(TagEnd::Link) => {
                if let Some((target, subpath, markdown)) = link.take() {
                    doc.links.push(LinkTarget {
                        target: target.clone(),
                        markdown,
                        subpath,
                    });
                    let text = if link_text.is_empty() {
                        target.clone()
                    } else {
                        std::mem::take(&mut link_text)
                    };
                    inlines.push(Inline::Link {
                        text,
                        target,
                        markdown,
                        index: doc.links.len() - 1,
                    });
                }
            }
            Event::Start(Tag::Image {
                link_type,
                dest_url,
                ..
            }) => {
                let markdown = !matches!(link_type, LinkType::WikiLink { .. });
                image = Some(destination(&dest_url, markdown).0);
                block_line = line;
            }
            Event::End(TagEnd::Image) => {
                if let Some(target) = image.take() {
                    flush_paragraph(&mut inlines, &mut quote, &mut doc, block_line);
                    if is_image(&target) {
                        doc.blocks.push(Block::Image { target, line });
                    } else {
                        doc.links.push(LinkTarget {
                            target: target.clone(),
                            markdown: false,
                            subpath: None,
                        });
                        doc.blocks.push(Block::Embed {
                            target,
                            index: doc.links.len() - 1,
                            line,
                        });
                    }
                }
            }
            Event::Start(Tag::List(start)) => {
                if depth == 0 {
                    flush_paragraph(&mut inlines, &mut quote, &mut doc, block_line);
                    list_line = line;
                }
                depth += 1;
                ordered.push(start);
            }
            Event::End(TagEnd::List(_)) => {
                depth = depth.saturating_sub(1);
                ordered.pop();
                if depth == 0 && !list.is_empty() {
                    doc.blocks.push(Block::List {
                        items: std::mem::take(&mut list),
                        line: list_line,
                    });
                }
            }
            Event::Start(Tag::Item) => {
                if !inlines.is_empty() {
                    // A nested list starts inside an item: close the item's text.
                    list.push(Item {
                        depth: depth.saturating_sub(2),
                        task: task.take(),
                        ordered: None,
                        inlines: std::mem::take(&mut inlines),
                    });
                }
                task = None;
            }
            Event::TaskListMarker(done) => task = Some((done, line)),
            Event::End(TagEnd::Item) => {
                let n = ordered.last_mut().and_then(|o| {
                    let v = *o;
                    if let Some(x) = o {
                        *x += 1;
                    }
                    v
                });
                if !inlines.is_empty() || task.is_some() {
                    list.push(Item {
                        depth: depth.saturating_sub(1),
                        task: task.take(),
                        ordered: n,
                        inlines: std::mem::take(&mut inlines),
                    });
                }
            }
            Event::Start(Tag::BlockQuote(_)) => {
                flush_paragraph(&mut inlines, &mut quote, &mut doc, block_line);
                quote = Some((Vec::new(), line));
                quote_links = doc.links.len();
            }
            Event::End(TagEnd::BlockQuote(_)) => {
                flush_paragraph(&mut inlines, &mut quote, &mut doc, block_line);
                if let Some((body, line)) = quote.take() {
                    // `> [!note] Title` is a callout: a bold title, then the
                    // rest of the quote as its body.
                    let source: Vec<&str> = text[range.clone()]
                        .lines()
                        .map(|l| l.trim_start().trim_start_matches('>').trim_start())
                        .collect();
                    let callout = source.first().and_then(|first| {
                        let rest = first.strip_prefix("[!")?;
                        let (kind, after) = rest.split_once(']')?;
                        let after = after.trim_start_matches(['+', '-']).trim();
                        Some(if after.is_empty() {
                            let mut k = kind.to_owned();
                            if let Some(c) = k.get_mut(0..1) {
                                c.make_ascii_uppercase();
                            }
                            k
                        } else {
                            after.to_owned()
                        })
                    });
                    match callout {
                        Some(title) => {
                            let inner = render(&source[1..].join("\n"));
                            // The body is parsed again on its own, so its
                            // links replace the ones found in the quote and
                            // are numbered after the links before it.
                            doc.links.truncate(quote_links);
                            let offset = doc.links.len();
                            doc.links.extend(inner.links);
                            let body = inner
                                .blocks
                                .into_iter()
                                .filter_map(|b| match b {
                                    Block::Paragraph { mut inlines, .. } => {
                                        for i in &mut inlines {
                                            if let Inline::Link { index, .. } = i {
                                                *index += offset;
                                            }
                                        }
                                        Some(inlines)
                                    }
                                    _ => None,
                                })
                                .collect();
                            doc.blocks.push(Block::Quote {
                                title: Some(title),
                                body,
                                line,
                            });
                        }
                        None => doc.blocks.push(Block::Quote {
                            title: None,
                            body,
                            line,
                        }),
                    }
                }
            }
            Event::Rule => doc.blocks.push(Block::Rule { line }),
            Event::Start(Tag::CodeBlock(kind)) => {
                let _ = matches!(kind, CodeBlockKind::Fenced(_));
                code = Some((String::new(), line));
            }
            Event::End(TagEnd::CodeBlock) => {
                if let Some((text, line)) = code.take() {
                    doc.blocks.push(Block::Code {
                        text: text.trim_end_matches('\n').to_owned(),
                        line,
                    });
                }
            }
            Event::Start(Tag::Table(_)) => table = Some((Vec::new(), Vec::new(), line)),
            Event::End(TagEnd::Table) => {
                if let Some((header, rows, line)) = table.take() {
                    doc.blocks.push(Block::Table { header, rows, line });
                }
            }
            Event::Start(Tag::TableHead) => in_head = true,
            Event::End(TagEnd::TableHead) => {
                in_head = false;
                if let Some((h, _, _)) = &mut table {
                    *h = std::mem::take(&mut row);
                }
            }
            Event::End(TagEnd::TableRow) => {
                if let Some((_, rows, _)) = &mut table {
                    rows.push(std::mem::take(&mut row));
                }
            }
            Event::End(TagEnd::TableCell) => row.push(std::mem::take(&mut inlines)),
            Event::Text(t) => {
                if let Some((buf, _)) = &mut code {
                    buf.push_str(&t);
                } else if image.is_some() {
                } else if link.is_some() {
                    link_text.push_str(&t);
                } else {
                    push_text(&mut inlines, &t, bold, italic, strike);
                }
            }
            Event::Code(t) => {
                if link.is_some() {
                    link_text.push_str(&t);
                } else {
                    inlines.push(Inline::Code(t.to_string()));
                }
            }
            Event::InlineMath(t) => inlines.push(Inline::Math(format!("${t}$"))),
            Event::DisplayMath(t) => inlines.push(Inline::Math(format!("$${t}$$"))),
            Event::SoftBreak | Event::HardBreak => {
                if link.is_some() {
                    link_text.push(' ');
                } else {
                    push_text(&mut inlines, " ", bold, italic, strike);
                }
            }
            _ => {}
        }
        let _ = in_head;
    }
    flush_paragraph(&mut inlines, &mut quote, &mut doc, block_line);
    doc
}

/// A link's target as the index reads it, and its `#heading`. A Markdown
/// destination to a file is split at `#` first, then each part decoded
/// (`%20` is a space, `%23` a `#` in the name); web addresses and
/// wikilinks stay as written.
fn destination(dest: &str, markdown: bool) -> (String, Option<String>) {
    use patinote_core::parser::percent_decode;
    if markdown && !dest.contains("://") && !dest.starts_with("mailto:") {
        match dest.split_once('#') {
            Some((path, sub)) => (percent_decode(path), Some(percent_decode(sub))),
            None => (percent_decode(dest), None),
        }
    } else {
        (dest.to_owned(), None)
    }
}

/// The text with the task on 0-based `line` checked or unchecked (F-27.3):
/// only `[ ]` / `[x]` on that line changes.
pub fn toggle_task(line_text: &str) -> Option<String> {
    // The task's own box is the first one on the line.
    let (i, new) = ["[ ]", "[x]", "[X]"]
        .iter()
        .filter_map(|m| line_text.find(m).map(|i| (i, *m)))
        .min_by_key(|(i, _)| *i)
        .map(|(i, m)| (i, if m == "[ ]" { "[x]" } else { "[ ]" }))?;
    let mut out = line_text.to_owned();
    out.replace_range(i..i + 3, new);
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOTE: &str =
        include_str!("../../../fixtures/vault-mockup/Proyek/Desain indeks Patinote.md");

    #[test]
    fn f27_1_the_sample_note_has_every_kind_of_block() {
        let doc = render(NOTE);
        let kinds: Vec<&str> = doc
            .blocks
            .iter()
            .map(|b| match b {
                Block::Properties(_) => "properties",
                Block::Heading { .. } => "heading",
                Block::Paragraph { .. } => "paragraph",
                Block::List { .. } => "list",
                Block::Quote { .. } => "quote",
                Block::Rule { .. } => "rule",
                Block::Table { .. } => "table",
                Block::Code { .. } => "code",
                Block::Image { .. } => "image",
                Block::Embed { .. } => "embed",
            })
            .collect();
        for kind in [
            "properties",
            "heading",
            "paragraph",
            "list",
            "table",
            "code",
            "image",
            "embed",
            "quote",
        ] {
            assert!(kinds.contains(&kind), "{kind} missing in {kinds:?}");
        }
        assert!(doc.links.iter().any(|l| l.target == "Patinote"));
        let tasks: Vec<bool> = doc
            .blocks
            .iter()
            .filter_map(|b| match b {
                Block::List { items, .. } => Some(
                    items
                        .iter()
                        .filter_map(|i| i.task.map(|t| t.0))
                        .collect::<Vec<_>>(),
                ),
                _ => None,
            })
            .flatten()
            .collect();
        assert_eq!(tasks, [false, true]);
        let callout = doc.blocks.iter().find_map(|b| match b {
            Block::Quote { title, .. } => title.clone(),
            _ => None,
        });
        assert_eq!(callout.as_deref(), Some("Indeks hanyalah cache"));
    }

    #[test]
    fn tags_in_text_become_labels() {
        let doc = render("Catatan #proyek/patinote dan #rust, bukan #1.\n");
        let Block::Paragraph { inlines, .. } = &doc.blocks[0] else {
            panic!("a paragraph")
        };
        let tags: Vec<&str> = inlines
            .iter()
            .filter_map(|i| match i {
                Inline::Tag(t) => Some(t.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(tags, ["proyek/patinote", "rust"]);
    }

    #[test]
    fn f27_3_toggling_a_task_changes_only_its_box() {
        assert_eq!(
            toggle_task("- [ ] Finalisasi").as_deref(),
            Some("- [x] Finalisasi")
        );
        assert_eq!(
            toggle_task("  - [x] Uji [ ] ini").as_deref(),
            Some("  - [ ] Uji [ ] ini")
        );
        assert_eq!(toggle_task("- [X] besar").as_deref(), Some("- [ ] besar"));
        assert_eq!(toggle_task("no task"), None);
    }

    /// Every link's number points at its own target, callouts included.
    #[test]
    fn f27_links_in_callouts_are_numbered_with_the_rest() {
        let doc = render("Lihat [[A]]\n\n> [!note] Catatan\n> Ke [[B]] dan [[C]]\n\nLalu [[D]]\n");
        let mut seen = Vec::new();
        let mut check = |inlines: &[Inline]| {
            for i in inlines {
                if let Inline::Link { target, index, .. } = i {
                    assert_eq!(&doc.links[*index].target, target, "{:?}", doc.links);
                    seen.push(target.clone());
                }
            }
        };
        for b in &doc.blocks {
            match b {
                Block::Paragraph { inlines, .. } => check(inlines),
                Block::Quote { body, .. } => body.iter().for_each(|p| check(p)),
                _ => {}
            }
        }
        assert_eq!(seen, ["A", "B", "C", "D"]);
        let targets: Vec<&str> = doc.links.iter().map(|l| l.target.as_str()).collect();
        assert_eq!(
            targets,
            ["A", "B", "C", "D"],
            "Tab goes through them in order"
        );
    }

    /// Markdown destinations are decoded as the index decodes them, so
    /// `%20` names the same note; web addresses stay as written.
    #[test]
    fn f27_markdown_link_targets_are_percent_decoded() {
        let doc = render(
            "[doc](Proyek/Desain%20indeks.md) [web](https://x.y/a%20b)\n\n![](Lampiran/foto%20saya.png)\n",
        );
        let targets: Vec<&str> = doc.links.iter().map(|l| l.target.as_str()).collect();
        assert_eq!(targets, ["Proyek/Desain indeks.md", "https://x.y/a%20b"]);
        assert!(
            doc.blocks.iter().any(
                |b| matches!(b, Block::Image { target, .. } if target == "Lampiran/foto saya.png")
            ),
            "{:?}",
            doc.blocks
        );
    }

    /// `%23` is a `#` in the file name, not the start of a heading.
    #[test]
    fn f27_an_encoded_hash_stays_in_the_file_name() {
        let doc = render("[c](C%23%20notes.md#Bagian%20dua) [d](a%7Cb.md)\n");
        let got: Vec<(&str, Option<&str>)> = doc
            .links
            .iter()
            .map(|l| (l.target.as_str(), l.subpath.as_deref()))
            .collect();
        assert_eq!(got, [("C# notes.md", Some("Bagian dua")), ("a|b.md", None)]);
    }
}
