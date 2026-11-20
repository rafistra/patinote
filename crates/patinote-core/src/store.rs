//! The storage interface of the index (docs/design/0001, section 1). Core
//! owns the logic; `patinote-index` implements [`Store`] with SQLite, and
//! [`MemoryStore`] keeps tests fast.

use std::collections::BTreeMap;
use std::convert::Infallible;

use crate::model::{LinkKind, ParsedNote};
use crate::resolve::Target;
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
    /// Matched by `.patinote/ignore` (Q-007).
    pub excluded: bool,
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
    /// Matched by `.patinote/ignore`: kept as a link target only (Q-007).
    pub excluded: bool,
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

/// A stored link, for resolution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkRecord {
    /// Store-specific link id.
    pub id: i64,
    /// Path of the note the link is in.
    pub source: String,
    pub kind: LinkKind,
    pub target: String,
    /// Path the link currently points to, if resolved.
    pub destination: Option<String>,
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

    /// Every file links can point to, with note aliases.
    fn targets(&self) -> Result<Vec<Target>, Self::Error>;

    /// Every stored link.
    fn links(&self) -> Result<Vec<LinkRecord>, Self::Error>;

    /// Sets where links point (`None`: a note that does not exist), atomically.
    fn set_link_destinations(
        &mut self,
        updates: &[(i64, Option<String>)],
    ) -> Result<(), Self::Error>;
}

/// An in-memory [`Store`] for tests.
#[derive(Debug, Default, Clone)]
pub struct MemoryStore {
    pub files: BTreeMap<String, IndexedFile>,
    pub meta: BTreeMap<String, String>,
    /// Link id → (source path, index in the source's links, destination).
    pub destinations: BTreeMap<i64, (String, usize, Option<String>)>,
    next_link: i64,
}

impl MemoryStore {
    /// Where the `index`-th link of `source` points, if resolved.
    pub fn destination(&self, source: &str, index: usize) -> Option<&str> {
        self.destinations
            .values()
            .find(|(s, i, _)| s == source && *i == index)
            .and_then(|(_, _, d)| d.as_deref())
    }

    fn drop_links_of(&mut self, path: &str) {
        self.destinations.retain(|_, (s, _, _)| s != path);
        // Like ON DELETE SET NULL: links into a removed file lose their destination.
        for (_, _, d) in self.destinations.values_mut() {
            if d.as_deref() == Some(path) {
                *d = None;
            }
        }
    }
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
                excluded: f.excluded,
            })
            .collect())
    }

    fn apply(&mut self, batch: &Batch) -> Result<(), Infallible> {
        for path in &batch.remove {
            self.files.remove(path);
            self.drop_links_of(path);
        }
        for (path, mtime) in &batch.touch {
            if let Some(f) = self.files.get_mut(path) {
                f.mtime_ns = *mtime;
            }
        }
        for f in &batch.upsert {
            self.destinations.retain(|_, (s, _, _)| s != &f.path);
            for i in 0..f.note.as_ref().map_or(0, |n| n.links.len()) {
                self.next_link += 1;
                self.destinations
                    .insert(self.next_link, (f.path.clone(), i, None));
            }
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

    fn targets(&self) -> Result<Vec<Target>, Infallible> {
        Ok(self
            .files
            .values()
            .map(|f| Target {
                path: f.path.clone(),
                kind: f.kind,
                aliases: f
                    .note
                    .as_ref()
                    .map(|n| n.aliases.clone())
                    .unwrap_or_default(),
            })
            .collect())
    }

    fn links(&self) -> Result<Vec<LinkRecord>, Infallible> {
        Ok(self
            .destinations
            .iter()
            .filter_map(|(&id, (source, i, dest))| {
                let link = self.files.get(source)?.note.as_ref()?.links.get(*i)?;
                Some(LinkRecord {
                    id,
                    source: source.clone(),
                    kind: link.kind,
                    target: link.target.clone(),
                    destination: dest.clone(),
                })
            })
            .collect())
    }

    fn set_link_destinations(
        &mut self,
        updates: &[(i64, Option<String>)],
    ) -> Result<(), Infallible> {
        for (id, dest) in updates {
            if let Some(entry) = self.destinations.get_mut(id) {
                entry.2 = dest.clone();
            }
        }
        Ok(())
    }
}
