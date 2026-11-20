//! SQLite index for Patinote (docs/design/0001, sections 2–4).
//!
//! The only crate that talks to SQLite. [`SqliteStore`] implements the
//! [`Store`] trait from `patinote-core`. The index is a cache: whenever it is
//! corrupt, from a newer version, or fails to migrate, it is deleted and built
//! again, which is always safe.

mod epoch;
pub mod query;

use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use std::{fs, io};

use patinote_core::model::{LinkKind, PropertyValue, TagSource};
use patinote_core::names::{extension, fold, link_key, stem};
use patinote_core::resolve::Target;
use patinote_core::scan::FileKind;
use patinote_core::store::{Batch, FileState, IndexedFile, LinkRecord, Store};
use rusqlite::{Connection, ErrorCode, OpenFlags, OptionalExtension, TransactionBehavior, params};
use rusqlite_migration::{M, Migrations};

/// Schema version stored in `PRAGMA user_version`.
pub const SCHEMA_VERSION: usize = 1;

/// File name of the index inside a vault's cache folder.
pub const INDEX_FILE: &str = "index.db";

pub use patinote_core::index::META_VAULT_PATH;

static MIGRATIONS: LazyLock<Migrations<'static>> =
    LazyLock::new(|| Migrations::new(vec![M::up(include_str!("../migrations/0001_init.sql"))]));

/// Errors from the index.
#[derive(Debug, thiserror::Error)]
pub enum IndexError {
    /// Another process held the write lock for longer than the busy timeout
    /// (F-03.7; the CLI exits with code 5).
    #[error("the index is locked by another process")]
    Locked,
    /// SQLite reported damaged data while the index was in use. Callers
    /// rebuild with [`SqliteStore::rebuild`]; [`update_index`] does it itself.
    #[error("the index database is damaged")]
    Corrupt,
    #[error("index database error: {0}")]
    Sqlite(rusqlite::Error),
    #[error("cannot use the index at {}: {source}", path.display())]
    Io { path: PathBuf, source: io::Error },
}

impl From<rusqlite::Error> for IndexError {
    fn from(e: rusqlite::Error) -> Self {
        match e.sqlite_error_code() {
            Some(ErrorCode::DatabaseBusy | ErrorCode::DatabaseLocked) => Self::Locked,
            Some(ErrorCode::DatabaseCorrupt | ErrorCode::NotADatabase) => Self::Corrupt,
            _ => Self::Sqlite(e),
        }
    }
}

/// Why an existing index was thrown away and created again.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RebuildReason {
    /// Not a database, a damaged schema, or damaged data found while in use
    /// (F-03.6, Q-006).
    Corrupt,
    /// Written by a newer Patinote with a higher schema version.
    NewerSchema,
    /// A migration failed.
    MigrationFailed,
}

/// Settings for opening an index.
#[derive(Debug, Clone, Copy)]
pub struct OpenOptions {
    /// How long a writer waits for another writer (F-03: 5 seconds).
    pub busy_timeout: Duration,
}

impl Default for OpenOptions {
    fn default() -> Self {
        Self {
            busy_timeout: Duration::from_secs(5),
        }
    }
}

/// Where the index of the vault at `vault_root` lives:
/// `<cache home>/patinote/<vault-id>/index.db`.
pub fn index_path_for(vault_root: &Path, cache_home: &Path) -> io::Result<PathBuf> {
    let id = patinote_core::location::vault_id(vault_root)?;
    Ok(patinote_core::location::vault_cache_dir(cache_home, &id).join(INDEX_FILE))
}

/// The SQLite implementation of [`Store`].
pub struct SqliteStore {
    conn: Connection,
    path: PathBuf,
    options: OpenOptions,
    rebuilt: Option<RebuildReason>,
    created: bool,
}

enum OpenFailure {
    Rebuild(RebuildReason),
    Fatal(IndexError),
}

impl SqliteStore {
    /// Opens (or creates) the index at `path`, rebuilding it when it cannot be
    /// trusted.
    pub fn open(path: &Path, options: &OpenOptions) -> Result<Self, IndexError> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir).map_err(|source| IndexError::Io {
                path: dir.to_path_buf(),
                source,
            })?;
        }
        let created = !path.exists();
        match try_open(path, options) {
            Ok(conn) => Ok(Self {
                conn,
                path: path.to_path_buf(),
                options: *options,
                rebuilt: None,
                created,
            }),
            Err(OpenFailure::Fatal(e)) => Err(e),
            Err(OpenFailure::Rebuild(reason)) => {
                remove_database(path)?;
                match try_open(path, options) {
                    Ok(conn) => Ok(Self {
                        conn,
                        path: path.to_path_buf(),
                        options: *options,
                        rebuilt: Some(reason),
                        created: true,
                    }),
                    Err(OpenFailure::Fatal(e)) => Err(e),
                    Err(OpenFailure::Rebuild(_)) => Err(IndexError::Io {
                        path: path.to_path_buf(),
                        source: io::Error::other("the new index could not be created"),
                    }),
                }
            }
        }
    }

    /// Why the index was rebuilt when it was opened, if it was.
    pub fn rebuilt(&self) -> Option<RebuildReason> {
        self.rebuilt
    }

    /// Whether the index file did not exist (or was replaced) when opened.
    pub fn created(&self) -> bool {
        self.created
    }

    /// The database file.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The underlying connection, for queries added in later steps.
    pub fn connection(&self) -> &Connection {
        &self.conn
    }

    /// Deletes the database and creates an empty one in its place, after
    /// damaged data was found while in use. The index must then be filled again.
    pub fn rebuild(&mut self) -> Result<(), IndexError> {
        // Close the damaged file before deleting it.
        self.conn = Connection::open_in_memory()?;
        remove_database(&self.path)?;
        self.conn = match try_open(&self.path, &self.options) {
            Ok(conn) => conn,
            Err(OpenFailure::Fatal(e)) => return Err(e),
            Err(OpenFailure::Rebuild(_)) => {
                return Err(IndexError::Io {
                    path: self.path.clone(),
                    source: io::Error::other("the new index could not be created"),
                });
            }
        };
        self.rebuilt = Some(RebuildReason::Corrupt);
        self.created = true;
        Ok(())
    }

    /// Runs the full `PRAGMA integrity_check`. It reads the whole database
    /// (about 0.9 s for 10,000 notes), so the app runs it in the background now
    /// and then instead of on every open (Q-006). Returns whether the index is
    /// intact.
    pub fn integrity_check(&self) -> Result<bool, IndexError> {
        match self
            .conn
            .query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0))
        {
            Ok(s) => Ok(s == "ok"),
            Err(e) => match IndexError::from(e) {
                IndexError::Corrupt => Ok(false),
                other => Err(other),
            },
        }
    }
}

/// File counts stored in the index.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Counts {
    pub notes: usize,
    pub attachments: usize,
    pub canvases: usize,
    pub excluded: usize,
    pub links: usize,
    pub unresolved_links: usize,
}

/// A stored per-file warning.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredWarning {
    pub path: String,
    pub code: String,
    pub line: Option<usize>,
}

impl SqliteStore {
    /// How many files and links the index holds.
    pub fn counts(&self) -> Result<Counts, IndexError> {
        let one = |sql: &str| -> Result<usize, IndexError> {
            Ok(self.conn.query_row(sql, [], |r| r.get::<_, i64>(0))? as usize)
        };
        Ok(Counts {
            notes: one("SELECT count(*) FROM notes WHERE kind = 'note'")?,
            attachments: one("SELECT count(*) FROM notes WHERE kind = 'attachment'")?,
            canvases: one("SELECT count(*) FROM notes WHERE kind = 'canvas'")?,
            excluded: one("SELECT count(*) FROM notes WHERE excluded = 1")?,
            links: one("SELECT count(*) FROM links")?,
            unresolved_links: one("SELECT count(*) FROM links WHERE dst_id IS NULL")?,
        })
    }

    /// Every stored per-file warning, by path.
    pub fn warnings(&self) -> Result<Vec<StoredWarning>, IndexError> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT n.path, w.code, w.line FROM warnings w JOIN notes n ON n.id = w.note_id ORDER BY n.path, w.line",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(StoredWarning {
                path: r.get(0)?,
                code: r.get(1)?,
                line: r.get::<_, Option<i64>>(2)?.map(|l| l as usize),
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }
}

/// Brings the index up to date, rebuilding it once when SQLite reports
/// damaged data along the way (F-03.6, Q-006). Check [`SqliteStore::rebuilt`]
/// to know whether it happened.
pub fn update_index(
    store: &mut SqliteStore,
    vault_root: &Path,
    options: &patinote_core::IndexOptions,
    progress: &mut dyn FnMut(patinote_core::Progress),
) -> Result<patinote_core::IndexReport, patinote_core::index::IndexError<IndexError>> {
    match patinote_core::index_vault(vault_root, store, options, progress) {
        Err(patinote_core::index::IndexError::Store(IndexError::Corrupt)) => {
            store
                .rebuild()
                .map_err(patinote_core::index::IndexError::Store)?;
            patinote_core::index_vault(vault_root, store, options, progress)
        }
        other => other,
    }
}

fn try_open(path: &Path, options: &OpenOptions) -> Result<Connection, OpenFailure> {
    // Damage found at any step of opening means rebuild; other errors are fatal.
    let fatal = |e: rusqlite::Error| match IndexError::from(e) {
        IndexError::Corrupt => OpenFailure::Rebuild(RebuildReason::Corrupt),
        other => OpenFailure::Fatal(other),
    };
    let mut conn = Connection::open(path).map_err(fatal)?;
    conn.busy_timeout(options.busy_timeout).map_err(fatal)?;

    // The first real read tells whether this is a database at all.
    let mode: String = conn
        .query_row("PRAGMA journal_mode = WAL", [], |r| r.get(0))
        .map_err(fatal)?;
    debug_assert_eq!(mode.to_lowercase(), "wal");
    // Memory budget (PRD): a small page cache and no memory-mapped I/O.
    conn.execute_batch(
        "PRAGMA synchronous = NORMAL;
         PRAGMA foreign_keys = ON;
         PRAGMA cache_size = -2048;
         PRAGMA mmap_size = 0;",
    )
    .map_err(fatal)?;

    // Cheap checks only (Q-006): reading the schema catches a damaged first
    // page. Damage elsewhere surfaces as SQLITE_CORRUPT when a query reaches
    // it, and `update_index` rebuilds then; a full integrity_check takes
    // about 0.9 s on 10,000 notes, too slow for every open.
    conn.query_row("SELECT count(*) FROM sqlite_master", [], |r| {
        r.get::<_, i64>(0)
    })
    .map_err(fatal)?;

    let version: i64 = conn
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .map_err(fatal)?;
    if version > SCHEMA_VERSION as i64 {
        return Err(OpenFailure::Rebuild(RebuildReason::NewerSchema));
    }
    if MIGRATIONS.to_latest(&mut conn).is_err() {
        return Err(OpenFailure::Rebuild(RebuildReason::MigrationFailed));
    }
    Ok(conn)
}

fn remove_database(path: &Path) -> Result<(), IndexError> {
    for suffix in ["", "-wal", "-shm"] {
        let mut p = path.as_os_str().to_owned();
        p.push(suffix);
        match fs::remove_file(&p) {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(source) => {
                return Err(IndexError::Io {
                    path: PathBuf::from(p),
                    source,
                });
            }
        }
    }
    Ok(())
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

fn kind_str(kind: FileKind) -> &'static str {
    match kind {
        FileKind::Note => "note",
        FileKind::Canvas => "canvas",
        FileKind::Attachment => "attachment",
    }
}

fn link_kind_str(kind: LinkKind) -> &'static str {
    match kind {
        LinkKind::Wiki => "wiki",
        LinkKind::Embed => "embed",
        LinkKind::Markdown => "markdown",
        LinkKind::Frontmatter => "frontmatter",
    }
}

fn delete_note_rows(tx: &rusqlite::Transaction<'_>, id: i64) -> rusqlite::Result<()> {
    // Child tables cascade on a note delete, but an update keeps the row (and
    // the id other notes link to), so children are replaced explicitly.
    for table in [
        "aliases",
        "tags",
        "headings",
        "blocks",
        "properties",
        "warnings",
    ] {
        tx.prepare_cached(&format!("DELETE FROM {table} WHERE note_id = ?1"))?
            .execute([id])?;
    }
    tx.prepare_cached("DELETE FROM links WHERE src_id = ?1")?
        .execute([id])?;
    tx.prepare_cached("DELETE FROM notes_fts WHERE rowid = ?1")?
        .execute([id])?;
    Ok(())
}

fn upsert(tx: &rusqlite::Transaction<'_>, f: &IndexedFile, now: i64) -> rusqlite::Result<()> {
    let note = f.note.as_ref();
    let stem = stem(&f.path);
    let title = match (f.kind, note.and_then(|n| n.title.as_deref())) {
        (_, Some(t)) => Some(t.to_owned()),
        (FileKind::Note, None) => Some(stem.to_owned()),
        _ => None,
    };
    let id: i64 = tx
        .prepare_cached(
            "INSERT INTO notes (path, path_lower, stem, stem_lower, ext, kind, title, size, mtime_ns, hash, frontmatter, indexed_at, excluded)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
             ON CONFLICT (path) DO UPDATE SET
                 path_lower = excluded.path_lower, stem = excluded.stem, stem_lower = excluded.stem_lower,
                 ext = excluded.ext, kind = excluded.kind, title = excluded.title, size = excluded.size,
                 mtime_ns = excluded.mtime_ns, hash = excluded.hash, frontmatter = excluded.frontmatter,
                 indexed_at = excluded.indexed_at, excluded = excluded.excluded
             RETURNING id",
        )?
        .query_row(
            params![
                f.path,
                fold(&f.path),
                stem,
                fold(stem),
                extension(&f.path),
                kind_str(f.kind),
                title,
                f.size as i64,
                f.mtime_ns,
                f.hash.map(|h| h.to_vec()),
                note.and_then(|n| n.frontmatter_json.as_deref()),
                now,
                f.excluded,
            ],
            |r| r.get(0),
        )?;
    delete_note_rows(tx, id)?;

    for w in &f.warnings {
        tx.prepare_cached("INSERT INTO warnings (note_id, code, line) VALUES (?1, ?2, ?3)")?
            .execute(params![id, w.code.as_str(), w.line.map(|l| l as i64)])?;
    }
    if let Some(text) = &f.text {
        tx.prepare_cached("INSERT INTO notes_fts (rowid, title, body) VALUES (?1, ?2, ?3)")?
            .execute(params![id, title.as_deref().unwrap_or(stem), text])?;
    }
    let Some(note) = note else { return Ok(()) };

    for alias in &note.aliases {
        tx.prepare_cached(
            "INSERT OR IGNORE INTO aliases (note_id, alias, alias_lower) VALUES (?1, ?2, ?3)",
        )?
        .execute(params![id, alias, fold(alias)])?;
    }
    for l in &note.links {
        tx.prepare_cached(
            "INSERT INTO links (src_id, dst_id, kind, raw, target, target_lower, subpath, display, line, byte_start, byte_end)
             VALUES (?1, NULL, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        )?
        .execute(params![
            id,
            link_kind_str(l.kind),
            l.raw,
            l.target,
            link_key(&l.target),
            l.subpath,
            l.display,
            l.line as i64,
            l.byte_start as i64,
            l.byte_end as i64
        ])?;
    }
    for t in &note.tags {
        let source = match t.source {
            TagSource::Inline => "inline",
            TagSource::Frontmatter => "frontmatter",
        };
        tx.prepare_cached(
            "INSERT INTO tags (note_id, tag, display, source, line) VALUES (?1, ?2, ?3, ?4, ?5)",
        )?
        .execute(params![
            id,
            t.tag,
            t.display,
            source,
            t.line.map(|l| l as i64)
        ])?;
    }
    for h in &note.headings {
        tx.prepare_cached(
            "INSERT INTO headings (note_id, level, text, slug, line, byte_start) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        )?
        .execute(params![id, h.level, h.text, fold(h.text.trim()), h.line as i64, h.byte_start as i64])?;
    }
    for b in &note.blocks {
        tx.prepare_cached(
            "INSERT OR IGNORE INTO blocks (note_id, block_id, line) VALUES (?1, ?2, ?3)",
        )?
        .execute(params![id, b.block_id, b.line as i64])?;
    }
    for p in &note.properties {
        let (text, num): (Option<String>, Option<f64>) = match &p.value {
            PropertyValue::Text(s) => (Some(s.clone()), None),
            PropertyValue::Integer(i) => (None, Some(*i as f64)),
            PropertyValue::Float(x) => (None, Some(*x)),
            PropertyValue::Bool(b) => (Some(b.to_string()), None),
            PropertyValue::Date(s) => (Some(s.clone()), epoch::date(s).map(|e| e as f64)),
            PropertyValue::DateTime(s) => (Some(s.clone()), epoch::datetime(s).map(|e| e as f64)),
            PropertyValue::Null => (None, None),
        };
        tx.prepare_cached(
            "INSERT OR IGNORE INTO properties (note_id, key, key_lower, idx, type, value_text, value_num)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        )?
        .execute(params![id, p.key, fold(&p.key), p.idx as i64, p.value.type_name(), text, num])?;
    }
    Ok(())
}

impl Store for SqliteStore {
    type Error = IndexError;

    fn file_states(&self) -> Result<Vec<FileState>, IndexError> {
        let mut stmt = self
            .conn
            .prepare_cached("SELECT path, size, mtime_ns, hash, excluded FROM notes")?;
        let rows = stmt.query_map([], |r| {
            let hash: Option<Vec<u8>> = r.get(3)?;
            Ok(FileState {
                path: r.get(0)?,
                size: r.get::<_, i64>(1)? as u64,
                mtime_ns: r.get(2)?,
                hash: hash.and_then(|h| h.try_into().ok()),
                excluded: r.get(4)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    fn apply(&mut self, batch: &Batch) -> Result<(), IndexError> {
        if batch.is_empty() {
            return Ok(());
        }
        // IMMEDIATE takes the write lock up front, so a busy index fails here,
        // after the busy timeout, before anything is written.
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        for path in &batch.remove {
            let id: Option<i64> = tx
                .prepare_cached("SELECT id FROM notes WHERE path = ?1")?
                .query_row([path], |r| r.get(0))
                .optional()?;
            if let Some(id) = id {
                tx.prepare_cached("DELETE FROM notes_fts WHERE rowid = ?1")?
                    .execute([id])?;
                tx.prepare_cached("DELETE FROM notes WHERE id = ?1")?
                    .execute([id])?;
            }
        }
        for (path, mtime) in &batch.touch {
            tx.prepare_cached("UPDATE notes SET mtime_ns = ?2 WHERE path = ?1")?
                .execute(params![path, mtime])?;
        }
        let now = now_secs();
        for f in &batch.upsert {
            upsert(&tx, f, now)?;
        }
        tx.commit()?;
        Ok(())
    }

    fn meta(&self, key: &str) -> Result<Option<String>, IndexError> {
        Ok(self
            .conn
            .prepare_cached("SELECT value FROM meta WHERE key = ?1")?
            .query_row([key], |r| r.get(0))
            .optional()?)
    }

    fn set_meta(&mut self, key: &str, value: &str) -> Result<(), IndexError> {
        self.conn
            .prepare_cached("INSERT INTO meta (key, value) VALUES (?1, ?2) ON CONFLICT (key) DO UPDATE SET value = excluded.value")?
            .execute([key, value])?;
        Ok(())
    }

    fn targets(&self) -> Result<Vec<Target>, IndexError> {
        let mut aliases: std::collections::HashMap<i64, Vec<String>> =
            std::collections::HashMap::new();
        let mut stmt = self
            .conn
            .prepare_cached("SELECT note_id, alias FROM aliases")?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?;
        for row in rows {
            let (id, alias) = row?;
            aliases.entry(id).or_default().push(alias);
        }
        let mut stmt = self
            .conn
            .prepare_cached("SELECT id, path, kind FROM notes")?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (id, path, kind) = row?;
            out.push(Target {
                path,
                kind: match kind.as_str() {
                    "note" => FileKind::Note,
                    "canvas" => FileKind::Canvas,
                    _ => FileKind::Attachment,
                },
                aliases: aliases.remove(&id).unwrap_or_default(),
            });
        }
        Ok(out)
    }

    fn links(&self) -> Result<Vec<LinkRecord>, IndexError> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT l.id, s.path, l.kind, l.target, d.path
             FROM links l JOIN notes s ON s.id = l.src_id LEFT JOIN notes d ON d.id = l.dst_id",
        )?;
        let rows = stmt.query_map([], |r| {
            let kind: String = r.get(2)?;
            Ok(LinkRecord {
                id: r.get(0)?,
                source: r.get(1)?,
                kind: match kind.as_str() {
                    "embed" => LinkKind::Embed,
                    "markdown" => LinkKind::Markdown,
                    "frontmatter" => LinkKind::Frontmatter,
                    _ => LinkKind::Wiki,
                },
                target: r.get(3)?,
                destination: r.get(4)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    fn set_link_destinations(
        &mut self,
        updates: &[(i64, Option<String>)],
    ) -> Result<(), IndexError> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        for (id, dest) in updates {
            tx.prepare_cached(
                "UPDATE links SET dst_id = (SELECT id FROM notes WHERE path = ?2) WHERE id = ?1",
            )?
            .execute(params![id, dest])?;
        }
        tx.commit()?;
        Ok(())
    }
}

/// What [`cache_clean`] did.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct CleanReport {
    /// Vault IDs whose index was removed because the vault folder is gone.
    pub removed: Vec<String>,
    /// Indexes kept because their vault still exists.
    pub kept: usize,
    /// Vault IDs kept because their vault path could not be read; nothing is
    /// deleted without proof that the vault is gone.
    pub unknown: Vec<String>,
}

/// Removes the indexes of vaults whose folder no longer exists (F-03.9).
/// Only folders named like a vault ID (16 hex characters) are considered.
pub fn cache_clean(cache_home: &Path) -> Result<CleanReport, IndexError> {
    let root = cache_home.join("patinote");
    let mut report = CleanReport::default();
    let entries = match fs::read_dir(&root) {
        Ok(entries) => entries,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(report),
        Err(source) => return Err(IndexError::Io { path: root, source }),
    };
    let mut dirs: Vec<(String, PathBuf)> = entries
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .filter_map(|e| {
            let name = e.file_name().into_string().ok()?;
            (name.len() == 16 && name.chars().all(|c| c.is_ascii_hexdigit()))
                .then(|| (name, e.path()))
        })
        .collect();
    dirs.sort();
    for (id, dir) in dirs {
        let vault_path =
            Connection::open_with_flags(dir.join(INDEX_FILE), OpenFlags::SQLITE_OPEN_READ_ONLY)
                .ok()
                .and_then(|c| {
                    c.query_row(
                        "SELECT value FROM meta WHERE key = ?1",
                        [META_VAULT_PATH],
                        |r| r.get::<_, String>(0),
                    )
                    .ok()
                });
        match vault_path {
            Some(p) if !Path::new(&p).exists() => {
                fs::remove_dir_all(&dir).map_err(|source| IndexError::Io {
                    path: dir.clone(),
                    source,
                })?;
                report.removed.push(id);
            }
            Some(_) => report.kept += 1,
            None => report.unknown.push(id),
        }
    }
    Ok(report)
}
