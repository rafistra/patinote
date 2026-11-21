//! Updating links when notes are renamed or moved (PRD F-10, "Ganti nama dan
//! pindah").
//!
//! [`plan`] reads every source note with the parser, finds the links that
//! point at a moved note, and rewrites only the bytes of those links, by the
//! parser's byte ranges. Every other byte of every file stays as it was
//! (F-10.6). The result becomes one write-path [`Transaction`].
//!
//! Rules:
//! - Wikilinks, embeds, Markdown links and frontmatter links are updated. The
//!   display text, `#heading` or `#^block` subpath, Markdown title and every
//!   other part of the link stay as written; only the target changes.
//! - A target written as a name (`[[Rapat]]`) changes only when the name
//!   changes, or when the name would no longer match exactly the moved note;
//!   then the shortest path suffix that does is written (`[[Proyek/Rapat]]`).
//! - A target written as a path keeps its style: as many folders as before
//!   (more if needed to stay unique), the full path when it was full, a
//!   leading `/`, and `.md` only when it was written.
//! - Markdown targets keep being relative, rooted or `<…>`; outside angle
//!   brackets, spaces are written `%20`.
//! - Spelling written by the user is kept for every folder or name that did
//!   not change (case-insensitively); changed parts use the new path's
//!   spelling.
//! - Links that point elsewhere, do not resolve, or use an alias are never
//!   touched. Relative Markdown links inside a moved note are rewritten from
//!   its new folder.
//! - When a link cannot be located exactly in its raw text, it is left alone.

use std::collections::{HashMap, HashSet};

use crate::model::{Link, LinkKind};
use crate::names::{fold, stem};
use crate::parser::percent_decode;
use crate::store::Hash;
use crate::write::{Actor, Expect, Op, Transaction, content_hash};

/// A rename or move of one note (or every note under a folder):
/// vault-relative paths, `.md` included for notes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Moved {
    pub from: String,
    pub to: String,
}

/// One link to change in one file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkChange {
    /// 1-based line of the link in the original file.
    pub line: usize,
    /// The link's raw text as it is now.
    pub old: String,
    /// The link's raw text after the change.
    pub new: String,
    /// Byte range of `old` in the original file (including any BOM); end
    /// exclusive.
    pub byte_start: usize,
    pub byte_end: usize,
}

/// The new bytes of one file, with what changed in it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileChange {
    /// The file's path before the move.
    pub path: String,
    /// Hash of the bytes that were read; the write expects it.
    pub expect: Hash,
    /// The whole file after the change.
    pub content: Vec<u8>,
    /// Every changed link, in file order.
    pub changes: Vec<LinkChange>,
}

/// What a rename or move will do.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RelinkPlan {
    pub moves: Vec<Moved>,
    /// Files whose links change, in the order of the sources given to [`plan`].
    pub files: Vec<FileChange>,
}

impl RelinkPlan {
    /// The number of links that change.
    pub fn link_count(&self) -> usize {
        self.files.iter().map(|f| f.changes.len()).sum()
    }

    /// The number of files with at least one changed link.
    pub fn note_count(&self) -> usize {
        self.files.iter().filter(|f| !f.changes.is_empty()).count()
    }

    fn file(&self, path: &str) -> Option<&FileChange> {
        self.files.iter().find(|f| f.path == path)
    }

    /// The transaction for moving notes one by one: an [`Op::Move`] for each
    /// moved note, then an [`Op::Write`] for every other changed file with
    /// [`Expect::Hash`] of the bytes read.
    ///
    /// A moved note whose own links change carries the new bytes in its move
    /// and expects the hash of the bytes read; otherwise its expectation comes
    /// from `move_expect(from)`.
    pub fn transaction(&self, actor: Actor, move_expect: impl Fn(&str) -> Expect) -> Transaction {
        let moved: HashSet<&str> = self.moves.iter().map(|m| m.from.as_str()).collect();
        let mut ops: Vec<Op> = self
            .moves
            .iter()
            .map(|m| match self.file(&m.from) {
                Some(f) => Op::Move {
                    from: m.from.clone(),
                    to: m.to.clone(),
                    content: Some(f.content.clone()),
                    expect: Expect::Hash(f.expect),
                },
                None => Op::Move {
                    from: m.from.clone(),
                    to: m.to.clone(),
                    content: None,
                    expect: move_expect(&m.from),
                },
            })
            .collect();
        ops.extend(
            self.files
                .iter()
                .filter(|f| !moved.contains(f.path.as_str()))
                .map(write_op),
        );
        Transaction { actor, ops }
    }

    /// The transaction for moving a whole folder at once (attachments and
    /// subfolders included): an [`Op::Write`] for every changed file at its
    /// current path, then one [`Op::Move`] of the folder. `folder_expect` is
    /// the folder's [`Expect::Tree`] before any change, as
    /// [`crate::write::expect_current`] returns it; the write path checks
    /// every expectation before applying anything.
    pub fn folder_transaction(
        &self,
        actor: Actor,
        from: &str,
        to: &str,
        folder_expect: Expect,
    ) -> Transaction {
        let mut ops: Vec<Op> = self.files.iter().map(write_op).collect();
        ops.push(Op::Move {
            from: from.trim_end_matches('/').to_owned(),
            to: to.trim_end_matches('/').to_owned(),
            content: None,
            expect: folder_expect,
        });
        Transaction { actor, ops }
    }
}

fn write_op(f: &FileChange) -> Op {
    Op::Write {
        path: f.path.clone(),
        content: f.content.clone(),
        expect: Expect::Hash(f.expect),
    }
}

/// The notes moved when folder `from` becomes `to`: one [`Moved`] per path in
/// `note_paths` under `from`, at any depth.
pub fn folder_moves(from: &str, to: &str, note_paths: &[String]) -> Vec<Moved> {
    let from = from.trim_end_matches('/');
    let to = to.trim_end_matches('/');
    if from.is_empty() || to.is_empty() {
        return Vec::new();
    }
    let prefix = format!("{from}/");
    note_paths
        .iter()
        .filter_map(|p| {
            p.strip_prefix(&prefix).map(|rest| Moved {
                from: p.clone(),
                to: format!("{to}/{rest}"),
            })
        })
        .collect()
}

/// Plans updating every link that points at a moved note.
///
/// - `sources`: path and bytes of every note that may link to a moved note
///   (the backlink sources, plus the moved notes themselves). Duplicates are
///   read once; files that are not UTF-8 are skipped.
/// - `resolve(source_path, link)`: the vault path the link resolves to
///   *before* the move, as [`crate::Resolver`] answers it.
/// - `unique_after(target)`: whether a link target (a name, or a path suffix
///   like `Proyek/Rapat`, without `.md`) will match exactly one file *after*
///   the move. It is only asked about the new paths' names and suffixes, so
///   the one match is the moved note.
pub fn plan(
    moves: &[Moved],
    sources: &[(String, Vec<u8>)],
    resolve: impl Fn(&str, &Link) -> Option<String>,
    unique_after: impl Fn(&str) -> bool,
) -> RelinkPlan {
    let moved: HashMap<&str, &str> = moves
        .iter()
        .map(|m| (m.from.as_str(), m.to.as_str()))
        .collect();
    let mut seen = HashSet::new();
    let mut files = Vec::new();
    for (path, bytes) in sources {
        if !seen.insert(path.as_str()) {
            continue;
        }
        let Ok(text) = std::str::from_utf8(bytes) else {
            continue;
        };
        let new_source = moved.get(path.as_str()).copied().unwrap_or(path);
        let mut changes = Vec::new();
        for link in crate::parse(text).links {
            if link.target.is_empty() {
                continue;
            }
            let Some(old_target) = resolve(path, &link) else {
                continue;
            };
            let new_target = moved
                .get(old_target.as_str())
                .copied()
                .unwrap_or(&old_target);
            if new_target == old_target && new_source == path {
                continue;
            }
            let ctx = Ctx {
                text,
                link: &link,
                old_source: path,
                new_source,
                old_target: &old_target,
                new_target,
                unique_after: &unique_after,
            };
            if let Some(new) = ctx.rewrite()
                && new != link.raw
            {
                changes.push(LinkChange {
                    line: link.line,
                    old: link.raw.clone(),
                    new,
                    byte_start: link.byte_start,
                    byte_end: link.byte_end,
                });
            }
        }
        let changes = nest(changes);
        if changes.is_empty() {
            continue;
        }
        let mut content = Vec::with_capacity(bytes.len());
        let mut at = 0;
        for c in &changes {
            content.extend_from_slice(&bytes[at..c.byte_start]);
            content.extend_from_slice(c.new.as_bytes());
            at = c.byte_end;
        }
        content.extend_from_slice(&bytes[at..]);
        files.push(FileChange {
            path: path.clone(),
            expect: content_hash(bytes),
            content,
            changes,
        });
    }
    RelinkPlan {
        moves: moves.to_vec(),
        files,
    }
}

/// Sorts changes by position and folds a link inside another
/// (`[![alt](img.png)](Note.md)`) into the outer one's new text, so no two
/// changes overlap. An inner change whose text the outer rewrite no longer
/// holds where it was, or a partial overlap, is dropped: that link stays.
fn nest(mut changes: Vec<LinkChange>) -> Vec<LinkChange> {
    changes.sort_by_key(|c| (c.byte_start, std::cmp::Reverse(c.byte_end)));
    // Each outer change, with how much the inner changes folded into it so
    // far moved the text after them.
    let mut out: Vec<(LinkChange, isize)> = Vec::with_capacity(changes.len());
    for c in changes {
        let Some((outer, shift)) = out.last_mut().filter(|(o, _)| c.byte_start < o.byte_end) else {
            out.push((c, 0));
            continue;
        };
        if c.byte_end > outer.byte_end {
            continue;
        }
        // Rewrites only change a link's target, which comes after any link
        // in its text, so the inner link sits at the same offset, moved by
        // the inner links before it.
        let Some(at) = (c.byte_start - outer.byte_start).checked_add_signed(*shift) else {
            continue;
        };
        if outer.new.get(at..at + c.old.len()) == Some(c.old.as_str()) {
            outer.new.replace_range(at..at + c.old.len(), &c.new);
            *shift += c.new.len() as isize - c.old.len() as isize;
        }
    }
    out.into_iter().map(|(c, _)| c).collect()
}

/// One link being rewritten.
struct Ctx<'a, U> {
    text: &'a str,
    link: &'a Link,
    old_source: &'a str,
    new_source: &'a str,
    old_target: &'a str,
    new_target: &'a str,
    unique_after: &'a U,
}

impl<U: Fn(&str) -> bool> Ctx<'_, U> {
    /// The link's new raw text, or `None` to leave it alone.
    fn rewrite(&self) -> Option<String> {
        let wiki_syntax = match self.link.kind {
            LinkKind::Wiki | LinkKind::Frontmatter => true,
            LinkKind::Markdown => false,
            LinkKind::Embed => self.link.raw.starts_with("![["),
        };
        if wiki_syntax {
            self.rewrite_wiki()
        } else {
            self.rewrite_markdown()
        }
    }

    fn rewrite_wiki(&self) -> Option<String> {
        if self.new_target == self.old_target {
            // Only the source moved; wikilinks do not depend on its folder.
            return None;
        }
        let raw = &self.link.raw;
        let start = raw.find("[[")? + 2;
        let inner = raw.get(start..raw.len().checked_sub(2)?)?;
        let end = wiki_target_end(inner);
        let written = &inner[..end];
        if written != self.link.target {
            return None;
        }
        let mut target = self.wiki_target(written)?;
        if self.link.kind == LinkKind::Frontmatter && self.single_quoted() {
            target = target.replace('\'', "''");
        }
        Some(format!("{}{target}{}", &raw[..start], &inner[end..]) + "]]")
    }

    /// Whether the frontmatter value is inside YAML single quotes.
    fn single_quoted(&self) -> bool {
        self.link
            .byte_start
            .checked_sub(1)
            .and_then(|i| self.text.as_bytes().get(i))
            == Some(&b'\'')
    }

    /// The new wikilink target for `written`, or `None` to leave it.
    fn wiki_target(&self, written: &str) -> Option<String> {
        let (body, ext) = split_md(written, self.old_target);
        let slash = body.starts_with('/');
        let body = body.trim_start_matches('/');
        let key = fold(body);
        let old_segs = segments(self.old_target);
        let new_segs = segments(self.new_target);
        let k = body.split('/').count();
        let new_body = if !key.contains('/') {
            if key != fold(old_segs.last()?) {
                return None; // an alias
            }
            self.suffix(&new_segs, 1, body)
        } else {
            let old_full = fold(&old_segs.join("/"));
            if old_full != key && !old_full.ends_with(&format!("/{key}")) {
                return None;
            }
            if slash || k >= old_segs.len() {
                keep_case(body, &new_segs.join("/"))
            } else {
                self.suffix(&new_segs, k, body)
            }
        };
        let lead = if slash { "/" } else { "" };
        Some(format!("{lead}{new_body}{ext}"))
    }

    /// The shortest suffix of `segs` with at least `min` segments that
    /// matches only the moved file, spelled like `written` where unchanged.
    fn suffix(&self, segs: &[&str], min: usize, written: &str) -> String {
        let n = segs.len();
        let mut k = min.clamp(1, n.max(1));
        loop {
            let candidate = keep_case(written, &segs[n.saturating_sub(k)..].join("/"));
            if k >= n || (self.unique_after)(&candidate) {
                return candidate;
            }
            k += 1;
        }
    }

    fn rewrite_markdown(&self) -> Option<String> {
        let raw = &self.link.raw;
        let open = raw.rfind("](")? + 2;
        if !raw.ends_with(')') {
            return None;
        }
        let close = raw.len() - 1;
        let region = raw.get(open..close)?;
        let ws = region.len() - region.trim_start().len();
        let rest = &region[ws..];
        let (angle, dest_start, dest_len) = if let Some(r) = rest.strip_prefix('<') {
            (true, open + ws + 1, r.find('>')?)
        } else {
            (
                false,
                open + ws,
                rest.find(char::is_whitespace).unwrap_or(rest.len()),
            )
        };
        let dest = &raw[dest_start..dest_start + dest_len];
        let path_len = dest.find('#').unwrap_or(dest.len());
        let written_raw = &dest[..path_len];
        let written = if angle {
            written_raw.to_owned()
        } else {
            percent_decode(written_raw)
        };
        if written != self.link.target {
            return None;
        }
        let new_path = self.markdown_target(&written)?;
        let new_dest = if angle {
            new_path
        } else {
            percent_encode(&new_path)
        };
        let new_dest_full = format!("{new_dest}{}", &dest[path_len..]);
        if new_dest_full == dest {
            return None;
        }
        Some(format!(
            "{}{new_dest_full}{}",
            &raw[..dest_start],
            &raw[dest_start + dest_len..]
        ))
    }

    /// The new decoded Markdown destination path for `written`.
    fn markdown_target(&self, written: &str) -> Option<String> {
        let (body, ext) = split_md(written, self.old_target);
        let dot = body.starts_with("./");
        let slash = body.starts_with('/');
        let is_old = |p: String| same_path(p, self.old_target);
        let relative =
            !slash && normalize(&join(dir_of(self.old_source), written)).is_some_and(is_old);
        let rooted = !relative && normalize(written).is_some_and(is_old);
        if !relative && !rooted {
            // Resolved by name: only rewrite when the name no longer works.
            let name = stem_of(self.new_target);
            let old_name = stem_of(self.old_target);
            if fold(name) == fold(old_name) && (self.unique_after)(name) {
                return None;
            }
        }
        let new_body = without_md(self.new_target);
        let out = if rooted {
            let lead = if slash { "/" } else { "" };
            format!(
                "{lead}{}",
                keep_case(body.trim_start_matches('/'), new_body)
            )
        } else {
            let rel = relative_path(dir_of(self.new_source), new_body);
            let rel = keep_case(body.trim_start_matches("./"), &rel);
            if dot && !rel.starts_with("../") {
                format!("./{rel}")
            } else {
                rel
            }
        };
        Some(format!("{out}{ext}"))
    }
}

/// Where the target ends in a wikilink's inner text (`target#sub|display`),
/// matching the parser's split.
fn wiki_target_end(inner: &str) -> usize {
    let link_end = match inner.find('|') {
        Some(i) if inner[..i].ends_with('\\') => i - 1,
        Some(i) => i,
        None => inner.len(),
    };
    inner[..link_end].find('#').unwrap_or(link_end)
}

/// Splits `.md` (as spelled) off a written target when the target file is a
/// note; other files keep their extension in the body.
fn split_md<'a>(written: &'a str, path: &str) -> (&'a str, &'a str) {
    if is_note(path) && written.len() > 3 {
        let at = written.len() - 3;
        if written.is_char_boundary(at) && written[at..].eq_ignore_ascii_case(".md") {
            return (&written[..at], &written[at..]);
        }
    }
    (written, "")
}

fn is_note(path: &str) -> bool {
    path.len() > 3
        && path.is_char_boundary(path.len() - 3)
        && path[path.len() - 3..].eq_ignore_ascii_case(".md")
}

/// The path without `.md` for notes, as is for other files.
fn without_md(path: &str) -> &str {
    if is_note(path) {
        &path[..path.len() - 3]
    } else {
        path
    }
}

/// The name a link uses for a file: the stem of a note, the file name of
/// anything else.
fn stem_of(path: &str) -> &str {
    if is_note(path) {
        stem(path)
    } else {
        path.rsplit('/').next().unwrap_or(path)
    }
}

/// Path segments, without `.md` for notes.
fn segments(path: &str) -> Vec<&str> {
    without_md(path).split('/').collect()
}

fn dir_of(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(d, _)| d)
}

fn join(dir: &str, rel: &str) -> String {
    if dir.is_empty() {
        rel.to_owned()
    } else {
        format!("{dir}/{rel}")
    }
}

/// Resolves `.` and `..` like the resolver; `None` if the path leaves the
/// vault.
fn normalize(path: &str) -> Option<String> {
    let mut parts: Vec<&str> = Vec::new();
    for seg in path.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            s => parts.push(s),
        }
    }
    (!parts.is_empty()).then(|| parts.join("/"))
}

/// Whether a normalized destination names `path`, as the resolver matches it
/// (case-insensitively, `.md` optional for a name without extension).
fn same_path(candidate: String, path: &str) -> bool {
    let (c, p) = (fold(&candidate), fold(path));
    let name = c.rsplit('/').next().unwrap_or(&c);
    c == p || (!name.contains('.') && format!("{c}.md") == p)
}

/// `target` relative to folder `from` (both vault-relative).
fn relative_path(from: &str, target: &str) -> String {
    let from: Vec<&str> = from.split('/').filter(|s| !s.is_empty()).collect();
    let to: Vec<&str> = target.split('/').collect();
    let common = from
        .iter()
        .zip(&to)
        .take_while(|(a, b)| a == b)
        .count()
        .min(to.len().saturating_sub(1));
    let mut parts: Vec<&str> = vec![".."; from.len() - common];
    parts.extend(&to[common..]);
    parts.join("/")
}

/// `new`, with each trailing segment that matches `written`'s segment at the
/// same distance from the end (case-insensitively) spelled as written.
fn keep_case(written: &str, new: &str) -> String {
    let old: Vec<&str> = written.split('/').collect();
    let segs: Vec<&str> = new.split('/').collect();
    let n = segs.len();
    segs.iter()
        .enumerate()
        .map(|(i, seg)| {
            let from_end = n - 1 - i;
            match old.len().checked_sub(1 + from_end).map(|j| old[j]) {
                Some(o) if fold(o) == fold(seg) => o,
                _ => seg,
            }
        })
        .collect::<Vec<_>>()
        .join("/")
}

/// Escapes what a bare Markdown destination cannot hold: spaces, `<`, `>`,
/// and a `%` that would read as an escape.
fn percent_encode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    for (i, c) in s.char_indices() {
        match c {
            ' ' => out.push_str("%20"),
            '<' => out.push_str("%3C"),
            '>' => out.push_str("%3E"),
            '%' if bytes
                .get(i + 1..i + 3)
                .is_some_and(|h| h.iter().all(u8::is_ascii_hexdigit)) =>
            {
                out.push_str("%25")
            }
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn helpers() {
        assert_eq!(relative_path("Proyek", "Proyek/Rapat.md"), "Rapat.md");
        assert_eq!(
            relative_path("Proyek", "Arsip/Rapat.md"),
            "../Arsip/Rapat.md"
        );
        assert_eq!(relative_path("", "Arsip/Rapat.md"), "Arsip/Rapat.md");
        assert_eq!(relative_path("A/B", "A"), "../../A");
        assert_eq!(keep_case("proyek/rapat", "Arsip/Rapat"), "Arsip/rapat");
        assert_eq!(keep_case("rapat", "Rapat IKN"), "Rapat IKN");
        assert_eq!(wiki_target_end("A#B|c"), 1);
        assert_eq!(wiki_target_end(r"A\|c"), 1);
        assert_eq!(wiki_target_end("A|c#d"), 1);
        assert_eq!(percent_encode("A B%20.md"), "A%20B%2520.md");
        assert_eq!(percent_decode("A%20B"), "A B");
        assert_eq!(split_md("Rapat.MD", "x/Rapat.md"), ("Rapat", ".MD"));
        assert_eq!(split_md("a.png", "a.png"), ("a.png", ""));
    }

    #[test]
    fn folder_moves_lists_nested_notes_only() {
        let paths: Vec<String> = ["Proyek/a.md", "Proyek/Sub/b.md", "Proyekan/c.md", "d.md"]
            .map(String::from)
            .to_vec();
        let m = folder_moves("Proyek/", "Arsip/Proyek", &paths);
        assert_eq!(
            m,
            [
                Moved {
                    from: "Proyek/a.md".into(),
                    to: "Arsip/Proyek/a.md".into()
                },
                Moved {
                    from: "Proyek/Sub/b.md".into(),
                    to: "Arsip/Proyek/Sub/b.md".into()
                },
            ]
        );
    }
}
