//! Vault scanner (PRD F-01; exclusion rules in docs/design/0001, section 7).
//!
//! Scanning only lists files and reads their metadata. It never writes to the
//! vault and never reads file contents; [`load_note`] does that per note.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use serde::Serialize;
use unicode_normalization::UnicodeNormalization;

/// Notes larger than this are listed but not parsed or searched (F-01).
pub const MAX_PARSE_BYTES: u64 = 5 * 1024 * 1024;

/// Per-vault ignore file, `.gitignore` syntax, relative to the vault root.
pub const IGNORE_FILE: &str = ".patinote/ignore";

/// Marker in Syncthing conflict copies: `name.sync-conflict-YYYYMMDD-HHMMSS-ID.ext`.
const SYNC_CONFLICT: &str = ".sync-conflict-";

/// What kind of file this is (`notes.kind` in the index).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum FileKind {
    /// `.md`, parsed.
    Note,
    /// `.canvas`, listed only.
    Canvas,
    /// Anything else, listed so links to it resolve.
    Attachment,
}

/// One file found in the vault.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScannedFile {
    /// Relative to the vault root, `/` separators, Unicode NFC.
    pub path: String,
    pub kind: FileKind,
    pub size: u64,
    /// Modification time in nanoseconds since the Unix epoch; 0 if unknown.
    pub mtime_ns: i64,
    /// Matched by `.patinote/ignore`: still a link target, but left out of
    /// search, the quick switcher and note lists (0001 section 7, Q-007).
    pub excluded: bool,
}

/// A Syncthing conflict copy. It is not a note; it goes to the conflict list (F-11).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncConflict {
    /// The copy, relative to the vault root.
    pub path: String,
    /// The note it conflicts with, relative to the vault root.
    pub original: String,
    pub size: u64,
    pub mtime_ns: i64,
}

/// Something the scanner skipped or could not read. Scanning continues.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanIssue {
    /// Relative to the vault root when possible.
    pub path: String,
    pub kind: ScanIssueKind,
}

/// Why a path was skipped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ScanIssueKind {
    /// Permission denied or another I/O error.
    Unreadable,
    /// A symlink leads back to one of its parents (only when following symlinks).
    SymlinkLoop,
    /// The file name is not valid UTF-8.
    InvalidName,
    /// `.patinote/ignore` has a line that could not be parsed.
    InvalidIgnoreFile,
}

/// Scanner settings.
#[derive(Debug, Clone, Copy, Default)]
pub struct ScanOptions {
    /// Follow symbolic links. Off by default (F-01).
    pub follow_symlinks: bool,
}

/// Everything found in one scan, sorted by path.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Scan {
    pub files: Vec<ScannedFile>,
    pub conflicts: Vec<SyncConflict>,
    pub issues: Vec<ScanIssue>,
}

/// Why a vault could not be scanned at all.
#[derive(Debug, thiserror::Error)]
pub enum ScanError {
    #[error("vault folder not found: {}", .0.display())]
    NotFound(PathBuf),
    #[error("vault path is not a folder: {}", .0.display())]
    NotADirectory(PathBuf),
    #[error("cannot open vault {}: {source}", path.display())]
    Io { path: PathBuf, source: io::Error },
}

/// Lists the files of the vault at `root`.
pub fn scan(root: &Path, options: &ScanOptions) -> Result<Scan, ScanError> {
    match fs::metadata(root) {
        Ok(m) if m.is_dir() => {}
        Ok(_) => return Err(ScanError::NotADirectory(root.to_path_buf())),
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            return Err(ScanError::NotFound(root.to_path_buf()));
        }
        Err(source) => {
            return Err(ScanError::Io {
                path: root.to_path_buf(),
                source,
            });
        }
    }

    let mut out = Scan::default();
    let mut builder = ignore::WalkBuilder::new(root);
    builder
        .standard_filters(false)
        .hidden(true)
        .parents(false)
        .follow_links(options.follow_symlinks);
    // `.patinote/ignore` does not hide files from the walk: matches are listed
    // with `excluded` so they stay link targets (Q-007).
    let ignore_file = root.join(IGNORE_FILE);
    let user_ignore = if ignore_file.is_file() {
        let mut gi = ignore::gitignore::GitignoreBuilder::new(root);
        // On a partial error the valid lines still apply.
        if gi.add(&ignore_file).is_some() {
            out.issues.push(ScanIssue {
                path: IGNORE_FILE.to_owned(),
                kind: ScanIssueKind::InvalidIgnoreFile,
            });
        }
        gi.build().ok()
    } else {
        None
    };

    // Walking in parallel: stat calls dominate the quick check (F-03.8, A-01.6).
    // The lists are sorted below, so the result does not depend on thread order.
    let shared = std::sync::Mutex::new(out);
    builder.build_parallel().run(|| {
        Box::new(|result| {
            // The stat call happens here, outside the lock.
            let mut one = Scan::default();
            visit(result, root, user_ignore.as_ref(), &mut one);
            // A poisoned lock means another walker thread panicked; stop.
            let Ok(mut out) = shared.lock() else {
                return ignore::WalkState::Quit;
            };
            out.files.append(&mut one.files);
            out.conflicts.append(&mut one.conflicts);
            out.issues.append(&mut one.issues);
            ignore::WalkState::Continue
        })
    });
    let mut out = shared
        .into_inner()
        .unwrap_or_else(std::sync::PoisonError::into_inner);

    out.files.sort_by(|a, b| a.path.cmp(&b.path));
    out.conflicts.sort_by(|a, b| a.path.cmp(&b.path));
    out.issues.sort_by(|a, b| {
        a.path
            .cmp(&b.path)
            .then_with(|| format!("{:?}", a.kind).cmp(&format!("{:?}", b.kind)))
    });
    Ok(out)
}

/// Adds one walk entry to the scan.
fn visit(
    result: Result<ignore::DirEntry, ignore::Error>,
    root: &Path,
    user_ignore: Option<&ignore::gitignore::Gitignore>,
    out: &mut Scan,
) {
    let entry = match result {
        Ok(entry) => entry,
        Err(err) => {
            out.issues.push(issue_from(&err, root));
            return;
        }
    };
    let Some(file_type) = entry.file_type() else {
        return;
    };
    if !file_type.is_file() {
        // Directories, and symlinks when they are not followed.
        return;
    }
    let Some(rel) = relative(entry.path(), root) else {
        out.issues.push(ScanIssue {
            path: entry.path().to_string_lossy().into_owned(),
            kind: ScanIssueKind::InvalidName,
        });
        return;
    };
    let name = rel.rsplit('/').next().unwrap_or(&rel);
    if name.ends_with('~') {
        // Editor backup files (F-04 ignores them too).
        return;
    }
    let (size, mtime_ns) = match entry.metadata() {
        Ok(m) => (m.len(), mtime_ns(&m)),
        Err(_) => {
            out.issues.push(ScanIssue {
                path: rel,
                kind: ScanIssueKind::Unreadable,
            });
            return;
        }
    };
    if let Some(original) = conflict_original(&rel) {
        out.conflicts.push(SyncConflict {
            path: rel,
            original,
            size,
            mtime_ns,
        });
        return;
    }
    let kind = kind_of(name);
    let excluded = user_ignore.is_some_and(|gi| {
        gi.matched_path_or_any_parents(entry.path(), false)
            .is_ignore()
    });
    out.files.push(ScannedFile {
        path: rel,
        kind,
        size,
        mtime_ns,
        excluded,
    });
}

/// Relative path with `/` separators in NFC, or `None` if the name is not UTF-8.
pub(crate) fn relative(path: &Path, root: &Path) -> Option<String> {
    let rel = path.strip_prefix(root).ok()?;
    let parts: Option<Vec<&str>> = rel.components().map(|c| c.as_os_str().to_str()).collect();
    Some(parts?.join("/").nfc().collect())
}

fn mtime_ns(m: &fs::Metadata) -> i64 {
    m.modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .and_then(|d| i64::try_from(d.as_nanos()).ok())
        .unwrap_or(0)
}

fn kind_of(name: &str) -> FileKind {
    match name
        .rsplit_once('.')
        .map(|(_, ext)| ext.to_ascii_lowercase())
    {
        Some(ext) if ext == "md" => FileKind::Note,
        Some(ext) if ext == "canvas" => FileKind::Canvas,
        _ => FileKind::Attachment,
    }
}

/// For `dir/Note.sync-conflict-20261119-162104-ABCDEFG.md`, returns `dir/Note.md`.
fn conflict_original(rel: &str) -> Option<String> {
    let (dir, name) = match rel.rsplit_once('/') {
        Some((d, n)) => (Some(d), n),
        None => (None, rel),
    };
    let at = name.find(SYNC_CONFLICT)?;
    let rest = &name[at + SYNC_CONFLICT.len()..];
    let ext = rest.find('.').map_or("", |i| &rest[i..]);
    let original = format!("{}{ext}", &name[..at]);
    Some(match dir {
        Some(d) => format!("{d}/{original}"),
        None => original,
    })
}

/// Turns a walk error into an issue with a vault-relative path when possible.
fn issue_from(err: &ignore::Error, root: &Path) -> ScanIssue {
    fn find(err: &ignore::Error) -> (Option<&Path>, bool) {
        match err {
            ignore::Error::WithPath { path, err } => (Some(path), find(err).1),
            ignore::Error::WithDepth { err, .. } | ignore::Error::WithLineNumber { err, .. } => {
                find(err)
            }
            ignore::Error::Loop { child, .. } => (Some(child), true),
            ignore::Error::Partial(errs) => errs.first().map_or((None, false), find),
            _ => (None, false),
        }
    }
    let (path, is_loop) = find(err);
    let path = path
        .and_then(|p| relative(p, root).or_else(|| Some(p.to_string_lossy().into_owned())))
        .unwrap_or_default();
    ScanIssue {
        path,
        kind: if is_loop {
            ScanIssueKind::SymlinkLoop
        } else {
            ScanIssueKind::Unreadable
        },
    }
}

/// The contents of a note, ready to parse or explaining why not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NoteContent {
    /// Valid UTF-8, BOM kept so byte offsets match the file.
    Text(String),
    /// Larger than [`MAX_PARSE_BYTES`]; listed but not parsed (F-01, F-05).
    TooLarge { size: u64 },
    /// Not UTF-8 (for example Latin-1); listed with a warning, not parsed.
    NotUtf8,
}

/// Reads a note for parsing.
pub fn load_note(path: &Path) -> io::Result<NoteContent> {
    let size = fs::metadata(path)?.len();
    if size > MAX_PARSE_BYTES {
        return Ok(NoteContent::TooLarge { size });
    }
    match String::from_utf8(fs::read(path)?) {
        Ok(text) => Ok(NoteContent::Text(text)),
        Err(_) => Ok(NoteContent::NotUtf8),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conflict_names() {
        assert_eq!(
            conflict_original("Rapat/Rapat 19 Nov.sync-conflict-20261119-162104-ABCDEFG.md")
                .as_deref(),
            Some("Rapat/Rapat 19 Nov.md")
        );
        assert_eq!(
            conflict_original("a.sync-conflict-20261119-162104-X").as_deref(),
            Some("a")
        );
        assert_eq!(conflict_original("Notes/plain.md"), None);
    }

    #[test]
    fn kinds() {
        assert_eq!(kind_of("a.md"), FileKind::Note);
        assert_eq!(kind_of("a.MD"), FileKind::Note);
        assert_eq!(kind_of("b.canvas"), FileKind::Canvas);
        assert_eq!(kind_of("c.png"), FileKind::Attachment);
        assert_eq!(kind_of("README"), FileKind::Attachment);
    }
}
