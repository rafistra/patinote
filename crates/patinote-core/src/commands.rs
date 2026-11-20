//! The command registry and the JSON contract shared by the CLI and the MCP
//! server (PRD A-01 to A-03, A-08; CLAUDE.md section 5).
//!
//! Every command has a stable dotted name. Its output is a Rust type with a
//! JSON Schema (`schemars`) written to `docs/schema/`. Every output has `"v"`
//! (the contract version of that command) and `"warnings"`. Adding a field
//! keeps `v`; changing or removing one raises it.

use schemars::JsonSchema;
use serde::Serialize;

/// One command in the registry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommandInfo {
    /// Stable dotted name, shared by the CLI, MCP, keymaps and the palette.
    pub name: &'static str,
    /// How it is typed on the command line, after `patinote`.
    pub cli: &'static str,
    /// One line for `--help` and MCP tool descriptions.
    pub summary: &'static str,
}

/// Every command, in `--help` order. Step 1.7b adds the read commands.
pub const COMMANDS: &[CommandInfo] = &[
    CommandInfo {
        name: "index.update",
        cli: "index",
        summary: "Update the index, or rebuild it from scratch",
    },
    CommandInfo {
        name: "vault.info",
        cli: "vault info",
        summary: "Vault and index location, counts, warnings",
    },
    CommandInfo {
        name: "cache.clean",
        cli: "cache clean",
        summary: "Remove indexes of vaults that no longer exist",
    },
];

/// Looks a command up by its dotted name.
pub fn command(name: &str) -> Option<&'static CommandInfo> {
    COMMANDS.iter().find(|c| c.name == name)
}

/// Error codes (A-02) and the exit code each one maps to (A-03).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    VaultNotFound,
    NoteNotFound,
    AmbiguousNote,
    InvalidArgument,
    IndexLocked,
    IoError,
    NotUtf8,
}

impl ErrorCode {
    /// The process exit code (A-03).
    pub fn exit_code(self) -> i32 {
        match self {
            Self::IoError | Self::NotUtf8 => exit::ERROR,
            Self::InvalidArgument => exit::INVALID_ARGUMENT,
            Self::VaultNotFound | Self::NoteNotFound => exit::NOT_FOUND,
            Self::AmbiguousNote => exit::CONFLICT,
            Self::IndexLocked => exit::LOCKED,
        }
    }
}

/// Exit codes (A-03). New codes need the owner's approval.
pub mod exit {
    /// Success, including empty results.
    pub const OK: i32 = 0;
    /// A general error, or warnings with `--strict`.
    pub const ERROR: i32 = 1;
    /// Bad arguments, or no vault given.
    pub const INVALID_ARGUMENT: i32 = 2;
    /// The vault or note does not exist.
    pub const NOT_FOUND: i32 = 3;
    /// A conflict, or an ambiguous name.
    pub const CONFLICT: i32 = 4;
    /// The index stayed locked for too long.
    pub const LOCKED: i32 = 5;
    /// Stopped with Ctrl+C.
    pub const INTERRUPTED: i32 = 130;
}

/// A problem that did not stop the command. Shown on stderr in text mode,
/// in `"warnings"` with `--json`; with `--strict` it makes the exit code 1.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct OutputWarning {
    /// `not_utf8`, `too_large`, `unreadable`, `frontmatter_invalid`,
    /// `symlink_loop`, `invalid_name`, `invalid_ignore_file`, `config`, `index_rebuilt`.
    pub code: String,
    /// Vault-relative path, when the warning is about one.
    pub path: Option<String>,
    /// 1-based line, when the warning has one.
    pub line: Option<usize>,
    pub message: String,
}

/// `index.update` (`patinote index`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct IndexOutput {
    pub v: u32,
    pub notes: usize,
    pub attachments: usize,
    pub canvases: usize,
    /// Files matched by `.patinote/ignore`.
    pub excluded: usize,
    pub sync_conflicts: usize,
    /// Files read and parsed in this run (`added + updated`).
    pub reprocessed: usize,
    pub added: usize,
    pub updated: usize,
    /// Files whose mtime moved but content did not.
    pub touched: usize,
    pub removed: usize,
    pub unchanged: usize,
    /// Links whose destination changed.
    pub links_updated: usize,
    /// Why the index was rebuilt, if it was: `rebuild_requested`, `corrupt`,
    /// `newer_schema`, `migration_failed`.
    pub rebuilt: Option<String>,
    pub duration_ms: u64,
    pub warnings: Vec<OutputWarning>,
}

/// `vault.info` (`patinote vault info`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct VaultInfoOutput {
    pub v: u32,
    /// Absolute, canonical vault path.
    pub vault: String,
    pub vault_id: String,
    /// Absolute path of the index database.
    pub index: String,
    pub notes: usize,
    pub attachments: usize,
    pub canvases: usize,
    pub excluded: usize,
    pub links: usize,
    /// Links to notes that do not exist.
    pub unresolved_links: usize,
    pub sync_conflicts: usize,
    pub warnings: Vec<OutputWarning>,
}

/// `cache.clean` (`patinote cache clean`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct CacheCleanOutput {
    pub v: u32,
    /// Vault IDs whose index was removed.
    pub removed: Vec<String>,
    /// Indexes kept because their vault exists.
    pub kept: usize,
    /// Vault IDs kept because their vault path could not be read.
    pub unknown: Vec<String>,
    pub warnings: Vec<OutputWarning>,
}

/// An error, written to stderr with `--json`; stdout stays empty (A-02).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct ErrorOutput {
    pub v: u32,
    pub error: ErrorBody,
}

/// The `"error"` object.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct ErrorBody {
    pub code: ErrorCode,
    pub message: String,
    /// The matching notes, for `ambiguous_note`.
    pub candidates: Option<Vec<String>>,
}

/// Schemas of every output, by file name in `docs/schema/`.
pub fn schemas() -> Vec<(&'static str, schemars::Schema)> {
    vec![
        ("index.update.json", schemars::schema_for!(IndexOutput)),
        ("vault.info.json", schemars::schema_for!(VaultInfoOutput)),
        ("cache.clean.json", schemars::schema_for!(CacheCleanOutput)),
        ("error.json", schemars::schema_for!(ErrorOutput)),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_names_are_unique_and_dotted() {
        let mut names: Vec<&str> = COMMANDS.iter().map(|c| c.name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), COMMANDS.len());
        assert!(COMMANDS.iter().all(|c| c.name.contains('.')));
        assert_eq!(command("vault.info").map(|c| c.cli), Some("vault info"));
    }

    #[test]
    fn exit_codes_follow_a03() {
        assert_eq!(ErrorCode::AmbiguousNote.exit_code(), 4);
        assert_eq!(ErrorCode::IndexLocked.exit_code(), 5);
        assert_eq!(ErrorCode::VaultNotFound.exit_code(), 3);
        assert_eq!(ErrorCode::InvalidArgument.exit_code(), 2);
        assert_eq!(ErrorCode::NotUtf8.exit_code(), 1);
    }
}
