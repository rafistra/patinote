//! Read queries for the CLI and MCP commands (PRD A-01). Each returns plain
//! rows; the commands shape them into the JSON contract.

use std::collections::HashMap;

use patinote_core::Target;
use patinote_core::names::{link_key, stem};
use patinote_core::scan::FileKind;
use rusqlite::{OptionalExtension, params};

use crate::{IndexError, SqliteStore};

/// One file in the index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoteRow {
    pub id: i64,
    pub path: String,
    pub title: Option<String>,
    /// `note`, `attachment` or `canvas`.
    pub kind: String,
    pub size: u64,
    pub mtime_ns: i64,
    pub excluded: bool,
}

/// One search hit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchHit {
    pub id: i64,
    pub path: String,
    pub title: Option<String>,
    pub snippet: String,
}

/// A link pointing at a note.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BacklinkRow {
    pub source_id: i64,
    pub source: String,
    pub source_mtime_ns: i64,
    pub kind: String,
    pub subpath: Option<String>,
    pub line: usize,
}

/// A link written in a note.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkRow {
    pub kind: String,
    pub raw: String,
    pub target: String,
    pub subpath: Option<String>,
    pub display: Option<String>,
    pub line: usize,
    /// Where it points, or `None` for a note that does not exist.
    pub destination: Option<String>,
}

/// A tag with how often it is used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TagRow {
    pub tag: String,
    /// The most common spelling.
    pub display: String,
    pub uses: usize,
    pub notes: usize,
}

/// A link to a note that does not exist.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnresolvedRow {
    pub target: String,
    pub source: String,
    pub line: usize,
}

fn note_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<NoteRow> {
    Ok(NoteRow {
        id: r.get(0)?,
        path: r.get(1)?,
        title: r.get(2)?,
        kind: r.get(3)?,
        size: r.get::<_, i64>(4)? as u64,
        mtime_ns: r.get(5)?,
        excluded: r.get(6)?,
    })
}

const NOTE_COLUMNS: &str = "id, path, title, kind, size, mtime_ns, excluded";

impl SqliteStore {
    /// Every file, sorted by path.
    pub fn all_notes(&self) -> Result<Vec<NoteRow>, IndexError> {
        let mut stmt = self
            .conn
            .prepare_cached(&format!("SELECT {NOTE_COLUMNS} FROM notes ORDER BY path"))?;
        Ok(stmt
            .query_map([], note_row)?
            .collect::<rusqlite::Result<_>>()?)
    }

    /// The file at an exact path.
    pub fn note_by_path(&self, path: &str) -> Result<Option<NoteRow>, IndexError> {
        Ok(self
            .conn
            .prepare_cached(&format!("SELECT {NOTE_COLUMNS} FROM notes WHERE path = ?1"))?
            .query_row([path], note_row)
            .optional()?)
    }

    /// Aliases of every note, by note id.
    pub fn all_aliases(&self) -> Result<HashMap<i64, Vec<String>>, IndexError> {
        let mut out: HashMap<i64, Vec<String>> = HashMap::new();
        let mut stmt = self
            .conn
            .prepare_cached("SELECT note_id, alias FROM aliases ORDER BY note_id, alias_lower")?;
        for row in stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))? {
            let (id, alias) = row?;
            out.entry(id).or_default().push(alias);
        }
        Ok(out)
    }

    /// Aliases and distinct tags of one note, ordered like [`Self::all_aliases`]
    /// and [`Self::all_tags`], for commands about a single note.
    pub fn aliases_and_tags(&self, id: i64) -> Result<(Vec<String>, Vec<String>), IndexError> {
        let aliases = self
            .conn
            .prepare_cached("SELECT alias FROM aliases WHERE note_id = ?1 ORDER BY alias_lower")?
            .query_map([id], |r| r.get(0))?
            .collect::<rusqlite::Result<Vec<String>>>()?;
        let mut tags: Vec<String> = Vec::new();
        let mut stmt = self
            .conn
            .prepare_cached("SELECT tag FROM tags WHERE note_id = ?1 ORDER BY rowid")?;
        for tag in stmt.query_map([id], |r| r.get::<_, String>(0))? {
            let tag = tag?;
            if !tags.contains(&tag) {
                tags.push(tag);
            }
        }
        Ok((aliases, tags))
    }

    /// The files a wikilink with this target could resolve to: those whose
    /// name matches its last segment, and notes with a matching alias. A
    /// [`patinote_core::Resolver`] built from them resolves the target
    /// exactly as one built from every file, without loading the vault.
    pub fn link_candidates(&self, target: &str) -> Result<Vec<Target>, IndexError> {
        let key = link_key(target.trim_start_matches('/'));
        let last = key.rsplit('/').next().unwrap_or(&key).to_owned();
        // A note `a.b.md` has the name `a.b`; an attachment `a.png` is named
        // `a.png` and stored with the stem `a`.
        let last_stem = stem(&last).to_owned();
        let mut stmt = self.conn.prepare_cached(
            "SELECT id, path, kind FROM notes
             WHERE stem_lower IN (?1, ?2)
                OR id IN (SELECT note_id FROM aliases WHERE alias_lower = ?3)",
        )?;
        let rows = stmt
            .query_map(params![last, last_stem, key], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let mut out = Vec::with_capacity(rows.len());
        for (id, path, kind) in rows {
            let aliases = self
                .conn
                .prepare_cached("SELECT alias FROM aliases WHERE note_id = ?1")?
                .query_map([id], |r| r.get(0))?
                .collect::<rusqlite::Result<Vec<String>>>()?;
            out.push(Target {
                path,
                kind: match kind.as_str() {
                    "note" => FileKind::Note,
                    "canvas" => FileKind::Canvas,
                    _ => FileKind::Attachment,
                },
                aliases,
            });
        }
        Ok(out)
    }

    /// Distinct tags of every note (lowercase), by note id, in first-use order.
    pub fn all_tags(&self) -> Result<HashMap<i64, Vec<String>>, IndexError> {
        let mut out: HashMap<i64, Vec<String>> = HashMap::new();
        let mut stmt = self
            .conn
            .prepare_cached("SELECT note_id, tag FROM tags ORDER BY rowid")?;
        for row in stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))? {
            let (id, tag) = row?;
            let tags = out.entry(id).or_default();
            if !tags.contains(&tag) {
                tags.push(tag);
            }
        }
        Ok(out)
    }

    /// Full-text search with an FTS5 query; best matches first. Returns the
    /// total number of matches and at most `limit` hits (`None`: all).
    pub fn search(
        &self,
        fts_query: &str,
        limit: Option<usize>,
    ) -> Result<(usize, Vec<SearchHit>), IndexError> {
        let total: i64 = self.conn.query_row(
            "SELECT count(*) FROM notes_fts WHERE notes_fts MATCH ?1",
            [fts_query],
            |r| r.get(0),
        )?;
        let limit = limit.map_or(-1, |l| l as i64);
        let mut stmt = self.conn.prepare_cached(
            "SELECT n.id, n.path, n.title, snippet(notes_fts, 1, '[', ']', '…', 12)
             FROM notes_fts JOIN notes n ON n.id = notes_fts.rowid
             WHERE notes_fts MATCH ?1 ORDER BY rank LIMIT ?2",
        )?;
        let hits = stmt
            .query_map(params![fts_query, limit], |r| {
                Ok(SearchHit {
                    id: r.get(0)?,
                    path: r.get(1)?,
                    title: r.get(2)?,
                    snippet: r.get(3)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok((total as usize, hits))
    }

    /// The stored text of a note (as read from disk), for context lines.
    pub fn body(&self, id: i64) -> Result<Option<String>, IndexError> {
        Ok(self
            .conn
            .prepare_cached("SELECT body FROM notes_fts WHERE rowid = ?1")?
            .query_row([id], |r| r.get(0))
            .optional()?)
    }

    /// Links pointing at a note, most recently modified source first, then
    /// by position (F-09, A-01.7).
    pub fn backlinks(&self, id: i64) -> Result<Vec<BacklinkRow>, IndexError> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT s.id, s.path, s.mtime_ns, l.kind, l.subpath, l.line
             FROM links l JOIN notes s ON s.id = l.src_id
             WHERE l.dst_id = ?1
             ORDER BY s.mtime_ns DESC, s.path, l.byte_start",
        )?;
        Ok(stmt
            .query_map([id], |r| {
                Ok(BacklinkRow {
                    source_id: r.get(0)?,
                    source: r.get(1)?,
                    source_mtime_ns: r.get(2)?,
                    kind: r.get(3)?,
                    subpath: r.get(4)?,
                    line: r.get::<_, i64>(5)? as usize,
                })
            })?
            .collect::<rusqlite::Result<_>>()?)
    }

    /// Links written in a note, in file order.
    pub fn outgoing(&self, id: i64) -> Result<Vec<LinkRow>, IndexError> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT l.kind, l.raw, l.target, l.subpath, l.display, l.line, d.path
             FROM links l LEFT JOIN notes d ON d.id = l.dst_id
             WHERE l.src_id = ?1 ORDER BY l.byte_start",
        )?;
        Ok(stmt
            .query_map([id], |r| {
                Ok(LinkRow {
                    kind: r.get(0)?,
                    raw: r.get(1)?,
                    target: r.get(2)?,
                    subpath: r.get(3)?,
                    display: r.get(4)?,
                    line: r.get::<_, i64>(5)? as usize,
                    destination: r.get(6)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?)
    }

    /// Every tag with its uses and number of notes, sorted by tag.
    pub fn tag_counts(&self) -> Result<Vec<TagRow>, IndexError> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT t.tag,
                    (SELECT display FROM tags d WHERE d.tag = t.tag GROUP BY display ORDER BY count(*) DESC, display LIMIT 1),
                    count(*), count(DISTINCT t.note_id)
             FROM tags t JOIN notes n ON n.id = t.note_id
             WHERE n.excluded = 0
             GROUP BY t.tag ORDER BY t.tag",
        )?;
        Ok(stmt
            .query_map([], |r| {
                Ok(TagRow {
                    tag: r.get(0)?,
                    display: r.get(1)?,
                    uses: r.get::<_, i64>(2)? as usize,
                    notes: r.get::<_, i64>(3)? as usize,
                })
            })?
            .collect::<rusqlite::Result<_>>()?)
    }

    /// Notes with no links in or out (links from a note to itself do not count).
    pub fn orphans(&self) -> Result<Vec<NoteRow>, IndexError> {
        let mut stmt = self.conn.prepare_cached(&format!(
            "SELECT {NOTE_COLUMNS} FROM notes n
             WHERE n.kind = 'note' AND n.excluded = 0
               AND NOT EXISTS (SELECT 1 FROM links l WHERE l.src_id = n.id)
               AND NOT EXISTS (SELECT 1 FROM links l WHERE l.dst_id = n.id AND l.src_id != n.id)
             ORDER BY n.path"
        ))?;
        Ok(stmt
            .query_map([], note_row)?
            .collect::<rusqlite::Result<_>>()?)
    }

    /// Links to notes that do not exist, by target then source.
    pub fn unresolved(&self) -> Result<Vec<UnresolvedRow>, IndexError> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT l.target, s.path, l.line FROM links l JOIN notes s ON s.id = l.src_id
             WHERE l.dst_id IS NULL AND l.target != ''
             ORDER BY l.target_lower, s.path, l.byte_start",
        )?;
        Ok(stmt
            .query_map([], |r| {
                Ok(UnresolvedRow {
                    target: r.get(0)?,
                    source: r.get(1)?,
                    line: r.get::<_, i64>(2)? as usize,
                })
            })?
            .collect::<rusqlite::Result<_>>()?)
    }
}
