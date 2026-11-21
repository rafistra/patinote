//! Incremental indexing (PRD F-03; docs/design/0001, section 3).
//!
//! 1. Scan the vault and compare each file's size and mtime with the index.
//! 2. Unchanged: skip without reading the file.
//! 3. Changed: read and hash it. Same hash (touched, or re-copied by
//!    Syncthing): only the mtime is updated.
//! 4. Different hash: parse again and replace all its rows.
//! 5. Files gone from disk are removed.
//!
//! Changes are applied in batches, each one transaction, so a process killed
//! halfway leaves the index at its last complete batch (F-03.4).

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use std::{fs, io};

use crate::model::{ParsedNote, WarningCode};
use crate::resolve::Resolver;
use crate::scan::{
    FileKind, MAX_PARSE_BYTES, ScanError, ScanIssue, ScanOptions, ScannedFile, scan,
};
use crate::store::{Batch, FileState, FileWarning, FileWarningCode, Hash, IndexedFile, Store};

/// `meta` key: canonical vault path (used by `cache clean`).
pub const META_VAULT_PATH: &str = "vault_path";
/// `meta` key: version of Patinote that last wrote the index.
pub const META_APP_VERSION: &str = "app_version";
/// `meta` key: Unix seconds of the last completed indexing run.
pub const META_INDEXED_AT: &str = "indexed_at";
/// `meta` key: `1` while files are stored whose links are not resolved
/// yet, so a run cut short is finished by the next one; `0` after.
pub const META_LINKS_PENDING: &str = "links_pending";

/// Indexing settings.
#[derive(Debug, Clone, Copy)]
pub struct IndexOptions {
    pub scan: ScanOptions,
    /// Changed files per transaction.
    pub batch_size: usize,
    /// Read and hash every file even when size and mtime match. Files whose
    /// hash is unchanged are still only touched; a rebuild from scratch is done
    /// by deleting the index (`patinote index --rebuild`).
    pub full: bool,
    /// Set by a Ctrl+C handler: indexing stops before the next batch, so the
    /// index stays at its last complete batch (A-03.5).
    pub cancel: Option<&'static AtomicBool>,
}

impl Default for IndexOptions {
    fn default() -> Self {
        Self {
            scan: ScanOptions::default(),
            batch_size: 500,
            full: false,
            cancel: None,
        }
    }
}

/// Progress, reported after the scan and after each batch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Progress {
    /// The vault was scanned; `changed` files need reading.
    Scanned { files: usize, changed: usize },
    /// `done` of `changed` files are stored.
    Stored { done: usize, changed: usize },
}

/// What an indexing run did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IndexReport {
    pub notes: usize,
    pub attachments: usize,
    pub canvases: usize,
    /// Syncthing conflict copies found (listed, not indexed).
    pub sync_conflicts: usize,
    /// New files stored.
    pub added: usize,
    /// Files whose content changed and were read again.
    pub updated: usize,
    /// Files whose mtime changed but content did not.
    pub touched: usize,
    pub removed: usize,
    /// Files skipped by the size and mtime check.
    pub unchanged: usize,
    /// Warnings stored for files written in this run.
    pub warnings: usize,
    /// Files matched by `.patinote/ignore`, kept as link targets only (Q-007).
    pub excluded: usize,
    /// Links whose destination changed in this run (step 1.4).
    pub links_updated: usize,
    /// Paths the scanner could not read, and other scan problems.
    pub issues: Vec<ScanIssue>,
    pub duration: Duration,
}

impl IndexReport {
    /// Files read and parsed again in this run (`added + updated`).
    pub fn reprocessed(&self) -> usize {
        self.added + self.updated
    }
}

/// Why indexing stopped.
#[derive(Debug, thiserror::Error)]
pub enum IndexError<E: std::error::Error + 'static> {
    #[error(transparent)]
    Scan(#[from] ScanError),
    #[error(transparent)]
    Store(E),
    /// Stopped by [`IndexOptions::cancel`]; the index keeps every batch stored before.
    #[error("indexing was interrupted")]
    Cancelled,
}

/// Brings the index in `store` up to date with the vault at `root`.
pub fn index_vault<S: Store>(
    root: &Path,
    store: &mut S,
    options: &IndexOptions,
    progress: &mut dyn FnMut(Progress),
) -> Result<IndexReport, IndexError<S::Error>> {
    let started = Instant::now();
    let scanned = scan(root, &options.scan)?;
    let mut report = IndexReport {
        sync_conflicts: scanned.conflicts.len(),
        issues: scanned.issues,
        ..IndexReport::default()
    };
    for f in &scanned.files {
        report.excluded += usize::from(f.excluded);
        match f.kind {
            FileKind::Note => report.notes += 1,
            FileKind::Attachment => report.attachments += 1,
            FileKind::Canvas => report.canvases += 1,
        }
    }

    let mut known: HashMap<String, FileState> = store
        .file_states()
        .map_err(IndexError::Store)?
        .into_iter()
        .map(|s| (s.path.clone(), s))
        .collect();
    let changed: Vec<(&ScannedFile, Option<FileState>)> = scanned
        .files
        .iter()
        .filter_map(|f| {
            let old = known.remove(&f.path);
            let same = old.as_ref().is_some_and(|o| {
                o.size == f.size && o.mtime_ns == f.mtime_ns && o.excluded == f.excluded
            });
            if same && !options.full {
                report.unchanged += 1;
                None
            } else {
                Some((f, old))
            }
        })
        .collect();
    let mut removed: Vec<String> = known.into_keys().collect();
    removed.sort();
    progress(Progress::Scanned {
        files: scanned.files.len(),
        changed: changed.len(),
    });

    let cancelled = || options.cancel.is_some_and(|c| c.load(Ordering::Relaxed));
    // Stored links start unresolved; until `resolve_links` has run, a stop
    // would leave them so with nothing left to re-index.
    let pending = store
        .meta(META_LINKS_PENDING)
        .map_err(IndexError::Store)?
        .is_some_and(|v| v == "1");
    if !changed.is_empty() || !removed.is_empty() {
        store
            .set_meta(META_LINKS_PENDING, "1")
            .map_err(IndexError::Store)?;
    }
    let mut batch = Batch::default();
    let mut done = 0;
    for (file, old) in &changed {
        if cancelled() {
            return Err(IndexError::Cancelled);
        }
        // A file whose exclusion changed is stored again even if its bytes did not.
        let old_for_hash = old.as_ref().filter(|o| o.excluded == file.excluded);
        match read(root, file, old_for_hash) {
            Read::SameContent => {
                batch.touch.push((file.path.clone(), file.mtime_ns));
                report.touched += 1;
            }
            Read::File(indexed) => {
                if old.is_some() {
                    report.updated += 1;
                } else {
                    report.added += 1;
                }
                report.warnings += indexed.warnings.len();
                batch.upsert.push(*indexed);
            }
        }
        done += 1;
        if batch.upsert.len() + batch.touch.len() >= options.batch_size.max(1) {
            store
                .apply(&std::mem::take(&mut batch))
                .map_err(IndexError::Store)?;
            progress(Progress::Stored {
                done,
                changed: changed.len(),
            });
        }
    }
    if cancelled() {
        return Err(IndexError::Cancelled);
    }
    report.removed = removed.len();
    batch.remove = removed;
    store.apply(&batch).map_err(IndexError::Store)?;
    progress(Progress::Stored {
        done,
        changed: changed.len(),
    });

    // Adding, changing or removing any file can move where links point (a new
    // shorter path, a removed note), so resolution runs after every change.
    if report.reprocessed() + report.removed > 0 || pending {
        report.links_updated = resolve_links(store).map_err(IndexError::Store)?;
    }
    if pending || !changed.is_empty() || report.removed > 0 {
        store
            .set_meta(META_LINKS_PENDING, "0")
            .map_err(IndexError::Store)?;
    }

    if let Ok(canonical) = fs::canonicalize(root) {
        store
            .set_meta(META_VAULT_PATH, &canonical.to_string_lossy())
            .map_err(IndexError::Store)?;
    }
    store
        .set_meta(META_APP_VERSION, env!("CARGO_PKG_VERSION"))
        .map_err(IndexError::Store)?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    store
        .set_meta(META_INDEXED_AT, &now.to_string())
        .map_err(IndexError::Store)?;

    report.duration = started.elapsed();
    Ok(report)
}

/// Points every stored link at its destination (docs/design/0001,
/// section 4). Returns how many links changed.
pub fn resolve_links<S: Store>(store: &mut S) -> Result<usize, S::Error> {
    let resolver = Resolver::new(store.targets()?);
    let updates: Vec<(i64, Option<String>)> = store
        .links()?
        .into_iter()
        .filter_map(|l| {
            let dest = resolver
                .resolve(&l.source, l.kind, &l.target)
                .map(|r| r.path.to_owned());
            (dest != l.destination).then_some((l.id, dest))
        })
        .collect();
    if !updates.is_empty() {
        store.set_link_destinations(&updates)?;
    }
    Ok(updates.len())
}

enum Read {
    /// The bytes hash to what the index already has.
    SameContent,
    File(Box<IndexedFile>),
}

fn hash(bytes: &[u8]) -> Hash {
    *blake3::hash(bytes).as_bytes()
}

/// Reads one changed file into what the index stores.
fn read(root: &Path, file: &ScannedFile, old: Option<&FileState>) -> Read {
    let mut indexed = IndexedFile {
        path: file.path.clone(),
        kind: file.kind,
        size: file.size,
        mtime_ns: file.mtime_ns,
        hash: None,
        excluded: file.excluded,
        text: None,
        note: None,
        warnings: Vec::new(),
    };
    let warn = |code| FileWarning { code, line: None };

    if file.size > MAX_PARSE_BYTES {
        // Not read at all: too large to parse or search (F-01), not hashed.
        if file.kind == FileKind::Note {
            indexed.warnings.push(warn(FileWarningCode::TooLarge));
        }
        return Read::File(Box::new(indexed));
    }
    let bytes = match fs::read(root.join(&file.path)) {
        Ok(b) => b,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Read::File(Box::new(indexed)),
        Err(_) => {
            indexed.warnings.push(warn(FileWarningCode::Unreadable));
            return Read::File(Box::new(indexed));
        }
    };
    let h = hash(&bytes);
    if old.and_then(|o| o.hash) == Some(h) {
        return Read::SameContent;
    }
    indexed.hash = Some(h);
    if file.kind != FileKind::Note {
        return Read::File(Box::new(indexed));
    }
    match String::from_utf8(bytes) {
        Ok(text) => {
            let note = crate::parser::parse(&text);
            indexed
                .warnings
                .extend(note.warnings.iter().map(|w| match w.code {
                    WarningCode::FrontmatterInvalid => FileWarning {
                        code: FileWarningCode::FrontmatterInvalid,
                        line: Some(1),
                    },
                }));
            if file.excluded {
                // A link target only: its title and aliases resolve links; it has
                // no text to search and no links or tags of its own (Q-007).
                indexed.note = Some(ParsedNote {
                    title: note.title,
                    aliases: note.aliases,
                    frontmatter_json: note.frontmatter_json,
                    ..ParsedNote::default()
                });
            } else {
                indexed.note = Some(note);
                indexed.text = Some(text);
            }
        }
        Err(_) => indexed.warnings.push(warn(FileWarningCode::NotUtf8)),
    }
    Read::File(Box::new(indexed))
}
