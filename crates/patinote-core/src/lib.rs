//! Core of Patinote: note model, Markdown parser, link resolver, vault
//! scanner, file watcher, write path and command registry.
//!
//! This crate must not depend on SQLite, Iced or any UI crate. Storage is
//! reached through the `Store` trait, implemented by `patinote-index`.

pub mod index;
pub mod location;
pub mod model;
pub mod names;
pub mod parser;
pub mod resolve;
pub mod scan;
pub mod store;
pub mod watch;
pub mod write;

pub use index::{IndexOptions, IndexReport, Progress, index_vault};
pub use model::{
    Block, Heading, Link, LinkKind, ParsedNote, Property, PropertyValue, Tag, TagSource, Warning,
    WarningCode,
};
pub use parser::parse;
pub use resolve::{Resolved, Resolver, Target};
pub use scan::{
    FileKind, MAX_PARSE_BYTES, NoteContent, Scan, ScanError, ScanIssue, ScanIssueKind, ScanOptions,
    ScannedFile, SyncConflict, load_note, scan,
};
pub use store::{
    Batch, FileState, FileWarning, FileWarningCode, Hash, IndexedFile, LinkRecord, MemoryStore,
    Store,
};
pub use watch::{Change, PollReason, VaultWatcher, WatchMode, WatchOptions, watch};
