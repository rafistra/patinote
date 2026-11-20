//! The read commands (A-01). Each builds its output type; printing is done by
//! the caller, so the MCP server (step 1.8) returns exactly the same JSON.

use std::collections::{BTreeMap, HashMap};

use patinote_core::commands::{
    Backlink, BacklinksOutput, ConflictOut, ConflictsOutput, ErrorBody, ErrorCode, HeadingOut,
    LineRange, LinkOut, LinksOutput, ListOutput, NoteRef, NoteSummary, OrphansOutput, ReadOutput,
    ResolveOutput, SearchOutput, SearchResult, SourceLine, TagOut, TagsOutput, UnresolvedOutput,
    UnresolvedTarget, VaultInfoOutput,
};
use patinote_core::model::LinkKind;
use patinote_core::names::{fold, link_key, nfc, stem};
use patinote_core::time::iso8601_utc;
use patinote_core::{Resolver, ScanOptions, scan};
use patinote_index::SqliteStore;
use patinote_index::query::NoteRow;

use crate::context::{Vault, index_failure, io_failure, warning, warning_message};
use crate::output::Failure;

/// Default number of results for `list` and `search` (A-02).
pub const DEFAULT_LIMIT: usize = 50;

/// What a command needs: the vault and its index.
pub struct Ctx<'a> {
    pub vault: &'a Vault,
    pub store: &'a SqliteStore,
}

fn title_of(row: &NoteRow) -> String {
    row.title
        .clone()
        .unwrap_or_else(|| stem(&row.path).to_owned())
}

fn note_ref(row: &NoteRow) -> NoteRef {
    NoteRef {
        path: row.path.clone(),
        title: title_of(row),
    }
}

/// Splits `[[Target#sub|text]]` (or a bare name) into its target.
fn bare_target(input: &str) -> &str {
    let inner = input
        .trim()
        .trim_start_matches("![[")
        .trim_start_matches("[[")
        .trim_end_matches("]]");
    let inner = inner.split('|').next().unwrap_or(inner);
    inner.split('#').next().unwrap_or(inner)
}

/// Finds the note a `<note>` argument names: an exact path (with or without
/// `.md`), then the wikilink rules (name, path suffix, alias). Ambiguous
/// names exit 4 with candidates (A-01.4); paths leaving the vault exit 2.
pub fn resolve_note(ctx: &Ctx<'_>, arg: &str) -> Result<NoteRow, Failure> {
    let arg = nfc(arg.trim());
    if arg.is_empty() {
        return Err(Failure::new(
            ErrorCode::InvalidArgument,
            "the note name is empty",
        ));
    }
    if arg.starts_with('/') || arg.split('/').any(|p| p == "..") {
        return Err(Failure::new(
            ErrorCode::InvalidArgument,
            format!("paths must stay inside the vault: {arg}"),
        ));
    }
    for candidate in [arg.clone(), format!("{arg}.md")] {
        if let Some(row) = ctx.store.note_by_path(&candidate).map_err(index_failure)? {
            return Ok(row);
        }
    }
    let target = bare_target(&arg);
    let resolver = Resolver::new(ctx.store.link_candidates(target).map_err(index_failure)?);
    let Some(found) = resolver.resolve("", LinkKind::Wiki, target) else {
        return Err(Failure::new(
            ErrorCode::NoteNotFound,
            format!("no note named '{target}'"),
        ));
    };
    if found.is_ambiguous() {
        return Err(Failure::Error(ErrorBody {
            code: ErrorCode::AmbiguousNote,
            message: format!("{} notes are named '{target}'", found.candidates.len()),
            candidates: Some(found.candidates.iter().map(|c| (*c).to_owned()).collect()),
        }));
    }
    let path = found.path.to_owned();
    ctx.store
        .note_by_path(&path)
        .map_err(index_failure)?
        .ok_or_else(|| Failure::new(ErrorCode::NoteNotFound, format!("no note named '{target}'")))
}

/// Summaries for many rows, loading aliases and tags once.
fn summaries(ctx: &Ctx<'_>, rows: &[NoteRow]) -> Result<Vec<NoteSummary>, Failure> {
    let aliases = ctx.store.all_aliases().map_err(index_failure)?;
    let tags = ctx.store.all_tags().map_err(index_failure)?;
    Ok(rows.iter().map(|r| summary(r, &aliases, &tags)).collect())
}

fn summary(
    row: &NoteRow,
    aliases: &HashMap<i64, Vec<String>>,
    tags: &HashMap<i64, Vec<String>>,
) -> NoteSummary {
    NoteSummary {
        path: row.path.clone(),
        title: title_of(row),
        kind: row.kind.clone(),
        aliases: aliases.get(&row.id).cloned().unwrap_or_default(),
        tags: tags.get(&row.id).cloned().unwrap_or_default(),
        modified: iso8601_utc(row.mtime_ns),
        size: row.size,
    }
}

/// `--kind` for `list`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum KindFilter {
    Note,
    Attachment,
    Canvas,
    All,
}

/// `--sort` for `list`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum SortBy {
    /// By path, A to Z.
    Name,
    /// Newest first.
    Modified,
}

/// Filters for `list`.
pub struct ListArgs<'a> {
    pub tag: Option<&'a str>,
    pub folder: Option<&'a str>,
    pub kind: KindFilter,
    pub sort: SortBy,
    /// `0` means no limit.
    pub limit: usize,
}

fn has_tag(tags: &[String], wanted: &str) -> bool {
    tags.iter()
        .any(|t| t == wanted || t.starts_with(&format!("{wanted}/")))
}

/// `note.list`. Files matched by `.patinote/ignore` are not listed (Q-007).
pub fn list(ctx: &Ctx<'_>, args: &ListArgs<'_>) -> Result<ListOutput, Failure> {
    let tags = ctx.store.all_tags().map_err(index_failure)?;
    let wanted_tag = args.tag.map(|t| fold(t.trim_start_matches('#')));
    let folder = args
        .folder
        .map(|f| format!("{}/", nfc(f.trim_matches('/'))));
    let mut rows: Vec<NoteRow> = ctx
        .store
        .all_notes()
        .map_err(index_failure)?
        .into_iter()
        .filter(|r| !r.excluded)
        .filter(|r| match args.kind {
            KindFilter::All => true,
            KindFilter::Note => r.kind == "note",
            KindFilter::Attachment => r.kind == "attachment",
            KindFilter::Canvas => r.kind == "canvas",
        })
        .filter(|r| {
            folder
                .as_ref()
                .is_none_or(|f| r.path.starts_with(f.as_str()))
        })
        .filter(|r| {
            wanted_tag
                .as_ref()
                .is_none_or(|t| has_tag(tags.get(&r.id).map_or(&[][..], Vec::as_slice), t))
        })
        .collect();
    if args.sort == SortBy::Modified {
        rows.sort_by(|a, b| {
            b.mtime_ns
                .cmp(&a.mtime_ns)
                .then_with(|| a.path.cmp(&b.path))
        });
    }
    let total = rows.len();
    if args.limit > 0 {
        rows.truncate(args.limit);
    }
    Ok(ListOutput {
        v: 1,
        total,
        truncated: rows.len() < total,
        notes: summaries(ctx, &rows)?,
        warnings: vec![],
    })
}

/// Parses `--lines 10:40` (1-based, inclusive; either side may be empty).
pub fn parse_lines(spec: &str) -> Result<(Option<usize>, Option<usize>), Failure> {
    let bad = || {
        Failure::new(
            ErrorCode::InvalidArgument,
            format!("--lines must look like 10:40, not {spec:?}"),
        )
    };
    let (a, b) = spec.split_once(':').ok_or_else(bad)?;
    let num = |s: &str| -> Result<Option<usize>, Failure> {
        if s.is_empty() {
            Ok(None)
        } else {
            s.parse::<usize>()
                .ok()
                .filter(|n| *n > 0)
                .map(Some)
                .ok_or_else(bad)
        }
    };
    let (start, end) = (num(a)?, num(b)?);
    if matches!((start, end), (Some(s), Some(e)) if s > e) {
        return Err(bad());
    }
    Ok((start, end))
}

/// `note.read`. `max_bytes` cuts the content at a line boundary (MCP, A-04).
pub fn read(
    ctx: &Ctx<'_>,
    row: &NoteRow,
    lines: (Option<usize>, Option<usize>),
    max_bytes: Option<usize>,
) -> Result<ReadOutput, Failure> {
    let path = ctx.vault.root.join(&row.path);
    let bytes =
        std::fs::read(&path).map_err(|e| io_failure(&format!("cannot read {}", row.path), e))?;
    let text = String::from_utf8(bytes).map_err(|_| {
        Failure::new(
            ErrorCode::NotUtf8,
            format!("{} is not valid UTF-8", row.path),
        )
    })?;
    let all: Vec<&str> = text.lines().collect();
    let total = all.len();
    let start = lines.0.unwrap_or(1);
    let mut end = lines.1.unwrap_or(total).min(total);
    if total > 0 && start > total {
        return Err(Failure::new(
            ErrorCode::InvalidArgument,
            format!("the note has only {total} lines"),
        ));
    }
    let mut truncated = false;
    let mut content = String::new();
    if total > 0 {
        for (i, line) in all[start - 1..end].iter().enumerate() {
            let extra = line.len() + usize::from(i > 0);
            if max_bytes.is_some_and(|max| content.len() + extra > max) && i > 0 {
                end = start + i - 1;
                truncated = true;
                break;
            }
            if i > 0 {
                content.push('\n');
            }
            content.push_str(line);
        }
    } else {
        end = 0;
    }

    let parsed = patinote_core::parse(&text);
    let frontmatter = parsed
        .frontmatter_json
        .as_deref()
        .and_then(|j| serde_json::from_str(j).ok());
    let headings = parsed
        .headings
        .iter()
        .map(|h| HeadingOut {
            level: h.level,
            text: h.text.clone(),
            line: h.line,
        })
        .collect();
    let links = link_outs(ctx, row)?;
    let (aliases, tags) = ctx.store.aliases_and_tags(row.id).map_err(index_failure)?;
    let aliases = HashMap::from([(row.id, aliases)]);
    let tags = HashMap::from([(row.id, tags)]);
    Ok(ReadOutput {
        v: 1,
        note: summary(row, &aliases, &tags),
        content,
        lines: LineRange {
            start: start.min(total.max(1)),
            end,
            total,
        },
        truncated,
        frontmatter,
        headings,
        links,
        warnings: vec![],
    })
}

fn link_outs(ctx: &Ctx<'_>, row: &NoteRow) -> Result<Vec<LinkOut>, Failure> {
    Ok(ctx
        .store
        .outgoing(row.id)
        .map_err(index_failure)?
        .into_iter()
        .map(|l| LinkOut {
            target: l.target,
            path: l.destination,
            kind: l.kind,
            subpath: l.subpath,
            display: l.display,
            line: l.line,
            raw: l.raw,
        })
        .collect())
}

/// Words and `"phrases"` of a query, as typed.
fn terms(query: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = query.trim();
    while !rest.is_empty() {
        if let Some(after) = rest.strip_prefix('"') {
            let end = after.find('"').unwrap_or(after.len());
            let phrase = after[..end].trim();
            if !phrase.is_empty() {
                out.push(phrase.to_owned());
            }
            rest = after.get(end + 1..).unwrap_or("").trim_start();
        } else {
            let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
            out.push(rest[..end].to_owned());
            rest = rest[end..].trim_start();
        }
    }
    out
}

/// An FTS5 query where every term is a quoted string, so user input never
/// becomes FTS5 syntax (A-01 `search`: words and phrases).
fn fts_query(terms: &[String]) -> String {
    terms
        .iter()
        .map(|t| format!("\"{}\"", t.replace('"', "\"\"")))
        .collect::<Vec<_>>()
        .join(" ")
}

/// `note.search`.
pub fn search(ctx: &Ctx<'_>, query: &str, limit: usize) -> Result<SearchOutput, Failure> {
    let terms = terms(query);
    if terms.is_empty() {
        return Err(Failure::new(
            ErrorCode::InvalidArgument,
            "the search query is empty",
        ));
    }
    let (total, hits) = ctx
        .store
        .search(&fts_query(&terms), (limit > 0).then_some(limit))
        .map_err(index_failure)?;
    let needles: Vec<String> = terms.iter().map(|t| t.to_lowercase()).collect();
    let mut results = Vec::with_capacity(hits.len());
    for h in hits {
        let body = ctx
            .store
            .body(h.id)
            .map_err(index_failure)?
            .unwrap_or_default();
        let line = body
            .lines()
            .position(|l| {
                let l = l.to_lowercase();
                needles.iter().any(|n| l.contains(n.as_str()))
            })
            .map_or(1, |i| i + 1);
        results.push(SearchResult {
            title: h.title.unwrap_or_else(|| stem(&h.path).to_owned()),
            path: h.path,
            line,
            snippet: h.snippet,
        });
    }
    Ok(SearchOutput {
        v: 1,
        query: query.to_owned(),
        total,
        truncated: results.len() < total,
        results,
        warnings: vec![],
    })
}

/// `note.backlinks`: sources most recently modified first (F-09, A-01.7).
pub fn backlinks(ctx: &Ctx<'_>, row: &NoteRow) -> Result<BacklinksOutput, Failure> {
    let rows = ctx.store.backlinks(row.id).map_err(index_failure)?;
    let mut bodies: HashMap<i64, Vec<String>> = HashMap::new();
    let mut out = Vec::with_capacity(rows.len());
    for b in rows {
        let lines = match bodies.entry(b.source_id) {
            std::collections::hash_map::Entry::Occupied(e) => e.into_mut(),
            std::collections::hash_map::Entry::Vacant(e) => {
                let body = ctx
                    .store
                    .body(b.source_id)
                    .map_err(index_failure)?
                    .unwrap_or_default();
                e.insert(body.lines().map(str::to_owned).collect())
            }
        };
        let context = lines
            .get(b.line.saturating_sub(1))
            .map_or("", |l| l.trim())
            .to_owned();
        out.push(Backlink {
            path: b.source,
            line: b.line,
            kind: b.kind,
            subpath: b.subpath,
            context,
        });
    }
    Ok(BacklinksOutput {
        v: 1,
        note: note_ref(row),
        total: out.len(),
        backlinks: out,
        warnings: vec![],
    })
}

/// `note.links`.
pub fn links(ctx: &Ctx<'_>, row: &NoteRow) -> Result<LinksOutput, Failure> {
    let links = link_outs(ctx, row)?;
    Ok(LinksOutput {
        v: 1,
        note: note_ref(row),
        total: links.len(),
        links,
        warnings: vec![],
    })
}

/// `tag.list`, optionally only a tag and its nested tags.
pub fn tags(ctx: &Ctx<'_>, prefix: Option<&str>) -> Result<TagsOutput, Failure> {
    let prefix = prefix.map(|p| fold(p.trim_start_matches('#')));
    let tags: Vec<TagOut> = ctx
        .store
        .tag_counts()
        .map_err(index_failure)?
        .into_iter()
        .filter(|t| {
            prefix
                .as_ref()
                .is_none_or(|p| t.tag == *p || t.tag.starts_with(&format!("{p}/")))
        })
        .map(|t| TagOut {
            tag: t.tag,
            display: t.display,
            uses: t.uses,
            notes: t.notes,
        })
        .collect();
    Ok(TagsOutput {
        v: 1,
        total: tags.len(),
        tags,
        warnings: vec![],
    })
}

/// `note.orphans`.
pub fn orphans(ctx: &Ctx<'_>) -> Result<OrphansOutput, Failure> {
    let notes: Vec<NoteRef> = ctx
        .store
        .orphans()
        .map_err(index_failure)?
        .iter()
        .map(note_ref)
        .collect();
    Ok(OrphansOutput {
        v: 1,
        total: notes.len(),
        notes,
        warnings: vec![],
    })
}

/// `link.unresolved`, grouped by target name (case-insensitive).
pub fn unresolved(ctx: &Ctx<'_>) -> Result<UnresolvedOutput, Failure> {
    let mut groups: BTreeMap<String, UnresolvedTarget> = BTreeMap::new();
    for r in ctx.store.unresolved().map_err(index_failure)? {
        let g = groups
            .entry(link_key(&r.target))
            .or_insert_with(|| UnresolvedTarget {
                target: r.target.clone(),
                count: 0,
                sources: Vec::new(),
            });
        g.count += 1;
        g.sources.push(SourceLine {
            path: r.source,
            line: r.line,
        });
    }
    let targets: Vec<UnresolvedTarget> = groups.into_values().collect();
    Ok(UnresolvedOutput {
        v: 1,
        total: targets.len(),
        targets,
        warnings: vec![],
    })
}

/// `link.resolve`. A name that matches nothing is not an error: `path` is
/// `null` (exit 0), because the question was answered.
pub fn resolve(ctx: &Ctx<'_>, input: &str) -> Result<ResolveOutput, Failure> {
    let target = bare_target(input);
    let resolver = Resolver::new(ctx.store.link_candidates(target).map_err(index_failure)?);
    let found = resolver.resolve("", LinkKind::Wiki, target);
    Ok(ResolveOutput {
        v: 1,
        input: input.to_owned(),
        path: found.as_ref().map(|f| f.path.to_owned()),
        ambiguous: found.as_ref().is_some_and(|f| f.is_ambiguous()),
        candidates: found
            .map(|f| f.candidates.iter().map(|c| (*c).to_owned()).collect())
            .unwrap_or_default(),
        warnings: vec![],
    })
}

/// `vault.info`: counts from the index, the Syncthing conflicts on disk, and
/// the per-file warnings stored in the index.
pub fn vault_info(ctx: &Ctx<'_>) -> Result<VaultInfoOutput, Failure> {
    let counts = ctx.store.counts().map_err(index_failure)?;
    let conflicts = scan(&ctx.vault.root, &ScanOptions::default()).map_or(0, |s| s.conflicts.len());
    let warnings = ctx
        .store
        .warnings()
        .map_err(index_failure)?
        .into_iter()
        .map(|w| warning(&w.code, Some(&w.path), w.line, warning_message(&w.code)))
        .collect();
    Ok(VaultInfoOutput {
        v: 1,
        vault: ctx.vault.root.to_string_lossy().into_owned(),
        vault_id: ctx.vault.id.clone(),
        index: ctx.vault.index_path.to_string_lossy().into_owned(),
        notes: counts.notes,
        attachments: counts.attachments,
        canvases: counts.canvases,
        excluded: counts.excluded,
        links: counts.links,
        unresolved_links: counts.unresolved_links,
        sync_conflicts: conflicts,
        warnings,
    })
}

/// `conflict.list`.
pub fn conflicts(ctx: &Ctx<'_>) -> Result<ConflictsOutput, Failure> {
    let s = scan(&ctx.vault.root, &ScanOptions::default())
        .map_err(|e| io_failure("cannot scan the vault", e))?;
    let conflicts: Vec<ConflictOut> = s
        .conflicts
        .into_iter()
        .map(|c| ConflictOut {
            path: c.path,
            original: c.original,
            modified: iso8601_utc(c.mtime_ns),
            size: c.size,
        })
        .collect();
    Ok(ConflictsOutput {
        v: 1,
        total: conflicts.len(),
        conflicts,
        warnings: vec![],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_terms_never_become_fts_syntax() {
        assert_eq!(
            terms(r#"indeks "skema SQLite" OR"#),
            ["indeks", "skema SQLite", "OR"]
        );
        // A stray quote is escaped by doubling it, so it stays inside the term.
        assert_eq!(fts_query(&terms(r#"a" NEAR(b*)"#)), r#""a""" "NEAR(b*)""#);
        assert!(terms("   ").is_empty());
    }

    #[test]
    fn line_ranges() {
        assert_eq!(parse_lines("10:40").ok(), Some((Some(10), Some(40))));
        assert_eq!(parse_lines(":5").ok(), Some((None, Some(5))));
        assert!(
            parse_lines("5").is_err() && parse_lines("9:2").is_err() && parse_lines("0:2").is_err()
        );
    }

    #[test]
    fn bare_targets() {
        assert_eq!(bare_target("[[Rapat#Agenda|rapat]]"), "Rapat");
        assert_eq!(bare_target("Proyek/Rapat"), "Proyek/Rapat");
    }
}
