//! The storage interface of the index (docs/design/0001, section 1). Core
//! owns the logic; `patinote-index` implements [`Store`] with SQLite, and
//! [`MemoryStore`] keeps tests fast.

use std::collections::BTreeMap;
use std::convert::Infallible;

use crate::model::ParsedNote;
use crate::scan::FileKind;

/// A blake3 hash of a file's bytes.
pub type Hash = [u8; 32];

/// What the index remembers about a file, for change detection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileState {
    pub path: String,
    pub size: u64,
    pub mtime_ns: i64,
    /// `None` for files that are not hashed (large attachments, unreadable files).
    pub hash: Option<Hash>,
}

/// A problem with one file that did not stop indexing (Q-005).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileWarning {
    pub code: FileWarningCode,
    /// 1-based line, when the problem has one.
    pub line: Option<usize>,
}

/// Codes stored in `warnings.code`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileWarningCode {
    NotUtf8,
    TooLarge,
    Unreadable,
    FrontmatterInvalid,
}

impl FileWarningCode {
    /// The value stored in the index.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NotUtf8 => "not_utf8",
            Self::TooLarge => "too_large",
            Self::Unreadable => "unreadable",
            Self::FrontmatterInvalid => "frontmatter_invalid",
        }
    }

    /// Parses a stored value.
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "not_utf8" => Self::NotUtf8,
            "too_large" => Self::TooLarge,
            "unreadable" => Self::Unreadable,
            "frontmatter_invalid" => Self::FrontmatterInvalid,
            _ => return None,
        })
    }
}

/// One file to add or replace in the index, with everything derived from it.
#[derive(Debug, Clone, PartialEq)]
pub struct IndexedFile {
    /// Relative to the vault root, `/` separators, NFC.
    pub path: String,
    pub kind: FileKind,
    pub size: u64,
    pub mtime_ns: i64,
    pub hash: Option<Hash>,
    /// Full text for search, for notes that were read; `None` otherwise.
    pub text: Option<String>,
    /// Parse result, for notes that were parsed.
    pub note: Option<ParsedNote>,
    pub warnings: Vec<FileWarning>,
}

/// Changes applied together in one transaction.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Batch {
    /// Files to add, or to replace with all their derived rows.
    pub upsert: Vec<IndexedFile>,
    /// Files whose content is unchanged but whose mtime moved: (path, mtime_ns).
    pub touch: Vec<(String, i64)>,
    /// Files that no longer exist.
    pub remove: Vec<String>,
}

impl Batch {
    /// Whether the batch changes nothing.
    pub fn is_empty(&self) -> bool {
        self.upsert.is_empty() && self.touch.is_empty() && self.remove.is_empty()
    }
}

/// Storage for the index.
pub trait Store {
    type Error: std::error::Error + Send + Sync + 'static;

    /// The state of every indexed file, for change detection.
    fn file_states(&self) -> Result<Vec<FileState>, Self::Error>;

    /// Applies a batch atomically: either all of it is stored or none.
    fn apply(&mut self, batch: &Batch) -> Result<(), Self::Error>;

    /// Reads a metadata value (`meta` table).
    fn meta(&self, key: &str) -> Result<Option<String>, Self::Error>;

    /// Writes a metadata value.
    fn set_meta(&mut self, key: &str, value: &str) -> Result<(), Self::Error>;
}

/// An in-memory [`Store`] for tests.
#[derive(Debug, Default, Clone)]
pub struct MemoryStore {
    pub files: BTreeMap<String, IndexedFile>,
    pub meta: BTreeMap<String, String>,
}

impl Store for MemoryStore {
    type Error = Infallible;

    fn file_states(&self) -> Result<Vec<FileState>, Infallible> {
        Ok(self
            .files
            .values()
            .map(|f| FileState {
                path: f.path.clone(),
                size: f.size,
                mtime_ns: f.mtime_ns,
                hash: f.hash,
            })
            .collect())
    }

    fn apply(&mut self, batch: &Batch) -> Result<(), Infallible> {
        for path in &batch.remove {
            self.files.remove(path);
        }
        for (path, mtime) in &batch.touch {
            if let Some(f) = self.files.get_mut(path) {
                f.mtime_ns = *mtime;
            }
        }
        for f in &batch.upsert {
            self.files.insert(f.path.clone(), f.clone());
        }
        Ok(())
    }

    fn meta(&self, key: &str) -> Result<Option<String>, Infallible> {
        Ok(self.meta.get(key).cloned())
    }

    fn set_meta(&mut self, key: &str, value: &str) -> Result<(), Infallible> {
        self.meta.insert(key.to_owned(), value.to_owned());
        Ok(())
    }
}
