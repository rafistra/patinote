//! An open note: its text in a Vim buffer, how it is written on disk, and
//! where saving stands (F-05).

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use patinote_core::store::Hash;
use patinote_core::write::{
    Actor, Expect, Journal, Op, Transaction, WriteError, commit, content_hash,
};

use super::file::{self, Format, ReadOnly};
use super::highlight::{LineKind, line_kinds};
use crate::vim;

/// How long after the last edit autosave runs (F-05).
pub const AUTOSAVE_AFTER: Duration = Duration::from_secs(1);
/// How often a failed save is tried again.
pub const RETRY_AFTER: Duration = Duration::from_secs(5);

/// Where saving stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Save {
    Idle,
    /// A save of this buffer revision is running.
    Running(u64),
    /// The last save failed; the text is kept and saving is tried again.
    Failed {
        reason: String,
        at: Instant,
    },
    /// The file changed outside Patinote since it was read; autosave waits
    /// for the user (F-11, step 2.7).
    Changed,
}

/// What happened on disk to an open note with unsaved text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum External {
    /// Changed by another app: the bytes on disk, their text and hash.
    Changed {
        bytes: Vec<u8>,
        text: String,
        hash: patinote_core::store::Hash,
    },
    /// Deleted (or moved away) by another app.
    Deleted,
}

/// An open note.
pub struct Document {
    pub path: String,
    pub buffer: vim::Buffer,
    pub format: Format,
    pub read_only: Option<ReadOnly>,
    /// Hash of the bytes on disk, as last read or written; `None` for a
    /// new note that is not on disk yet (it is written once it has text).
    pub disk_hash: Option<Hash>,
    /// The buffer revision that is on disk.
    pub saved_rev: u64,
    pub last_edit: Option<Instant>,
    pub save: Save,
    /// The Vim engine of this note (mode, pending keys, undo, last search).
    pub vim: vim::Vim,
    /// Changed or deleted on disk while it had unsaved text (F-11).
    pub external: Option<External>,
    /// The disk changed while a save ran: read it again once the save lands.
    pub recheck: bool,
    kinds: Vec<LineKind>,
    kinds_rev: u64,
}

impl Document {
    /// A document from a note's bytes on disk.
    pub fn new(path: String, bytes: &[u8], vim: vim::Vim) -> Self {
        let opened = file::open(bytes);
        let buffer = vim::Buffer::from_text(&opened.text);
        let rev = buffer.revision();
        let kinds = line_kinds(buffer.lines());
        Self {
            path,
            buffer,
            format: opened.format,
            read_only: opened.read_only,
            disk_hash: Some(opened.hash),
            saved_rev: rev,
            last_edit: None,
            save: Save::Idle,
            vim,
            external: None,
            recheck: false,
            kinds,
            kinds_rev: rev,
        }
    }

    /// A new, empty note that is not on disk yet (F-07.5, F-08).
    pub fn new_note(path: String, vim: vim::Vim) -> Self {
        let mut doc = Self::new(path, b"", vim);
        doc.disk_hash = None;
        doc
    }

    /// Unsaved changes.
    pub fn dirty(&self) -> bool {
        self.buffer.revision() != self.saved_rev
    }

    /// The kind of each line, recomputed after the text changes.
    pub fn kinds(&mut self) -> &[LineKind] {
        if self.kinds_rev != self.buffer.revision() {
            self.kinds = line_kinds(self.buffer.lines());
            self.kinds_rev = self.buffer.revision();
        }
        &self.kinds
    }

    /// The kinds as last computed (call [`Self::kinds`] after edits).
    pub fn cached_kinds(&self) -> &[LineKind] {
        &self.kinds
    }

    /// Records an edit made now.
    pub fn edited(&mut self, now: Instant) {
        self.last_edit = Some(now);
    }

    /// Whether autosave should run now: unsaved, idle for a second (or
    /// `force`, for switching notes, losing focus and quitting), and not
    /// already saving. A failed save is retried every few seconds.
    pub fn should_save(&self, now: Instant, force: bool) -> bool {
        if !self.dirty() || self.read_only.is_some() {
            return false;
        }
        // A new note stays off the disk until it has text (F-08.6).
        if self.disk_hash.is_none() && self.buffer.text().trim().is_empty() {
            return false;
        }
        match &self.save {
            Save::Running(_) | Save::Changed => false,
            Save::Failed { at, .. } => force || now.duration_since(*at) >= RETRY_AFTER,
            Save::Idle => {
                force
                    || self
                        .last_edit
                        .is_none_or(|t| now.duration_since(t) >= AUTOSAVE_AFTER)
            }
        }
    }

    /// Starts a save: marks it running and returns the job to run off the
    /// UI thread.
    pub fn start_save(&mut self, root: &Path, journal: &Journal) -> SaveJob {
        let rev = self.buffer.revision();
        self.save = Save::Running(rev);
        SaveJob {
            root: root.to_path_buf(),
            journal: journal.clone(),
            path: self.path.clone(),
            bytes: file::encode(&self.buffer.text(), self.format),
            expect: self.disk_hash,
            rev,
        }
    }

    /// Applies the result of a save.
    pub fn finish_save(&mut self, done: &SaveDone, now: Instant) {
        match &done.result {
            Ok(hash) => {
                self.disk_hash = Some(*hash);
                self.saved_rev = done.rev;
                self.save = Save::Idle;
            }
            Err(SaveFailure::Changed) => self.save = Save::Changed,
            Err(SaveFailure::Io(reason)) => {
                self.save = Save::Failed {
                    reason: reason.clone(),
                    at: now,
                }
            }
        }
    }
}

/// A save to run on a background thread.
#[derive(Debug, Clone)]
pub struct SaveJob {
    root: PathBuf,
    journal: Journal,
    pub path: String,
    bytes: Vec<u8>,
    /// What is on disk now; `None` when the file must not exist yet.
    expect: Option<Hash>,
    rev: u64,
}

/// Why a save did not happen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SaveFailure {
    /// The file changed outside since it was read (F-11).
    Changed,
    /// Writing failed; the reason, for the banner.
    Io(String),
}

/// The result of a [`SaveJob`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaveDone {
    pub path: String,
    pub rev: u64,
    /// The new hash on disk.
    pub result: Result<Hash, SaveFailure>,
}

impl SaveJob {
    /// Writes the note through the core write path (atomic, journaled,
    /// checked against the hash it was read with). Bytes equal to what is
    /// on disk are not written again (F-05.4).
    pub fn run(self) -> SaveDone {
        let hash = content_hash(&self.bytes);
        let result = if self.expect == Some(hash) {
            Ok(hash)
        } else {
            let tx = Transaction {
                actor: Actor::User,
                ops: vec![Op::Write {
                    path: self.path.clone(),
                    content: self.bytes,
                    expect: self.expect.map_or(Expect::Absent, Expect::Hash),
                }],
            };
            match commit(&self.root, &self.journal, &tx) {
                Ok(_) => Ok(hash),
                Err(WriteError::Conflict { .. }) => Err(SaveFailure::Changed),
                Err(e) => Err(SaveFailure::Io(io_reason(&e))),
            }
        };
        SaveDone {
            path: self.path,
            rev: self.rev,
            result,
        }
    }
}

/// A short reason for the banner: "the disk is full", "permission denied".
fn io_reason(e: &WriteError) -> String {
    if let WriteError::InvalidPath { reason, .. } = e {
        return (*reason).to_owned();
    }
    let source = match e {
        WriteError::Io { source, .. }
        | WriteError::RolledBack { source, .. }
        | WriteError::RollbackFailed { source, .. } => Some(source),
        _ => None,
    };
    match source.map(std::io::Error::kind) {
        Some(std::io::ErrorKind::StorageFull) => "the disk is full".into(),
        Some(std::io::ErrorKind::PermissionDenied) => "permission denied".into(),
        Some(std::io::ErrorKind::ReadOnlyFilesystem) => "the disk is read-only".into(),
        _ => e.to_string(),
    }
}
