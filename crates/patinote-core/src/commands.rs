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

/// Every command, in `--help` order.
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
        name: "note.list",
        cli: "list",
        summary: "List notes, filtered by tag, folder or kind",
    },
    CommandInfo {
        name: "note.read",
        cli: "read",
        summary: "Read a note, with its frontmatter, tags, headings and links",
    },
    CommandInfo {
        name: "note.search",
        cli: "search",
        summary: "Search note text for words and phrases",
    },
    CommandInfo {
        name: "note.backlinks",
        cli: "backlinks",
        summary: "Links to a note, with the sentence around each",
    },
    CommandInfo {
        name: "note.links",
        cli: "links",
        summary: "Links in a note, including to notes that do not exist",
    },
    CommandInfo {
        name: "tag.list",
        cli: "tags",
        summary: "Tags with how often they are used, nested tags included",
    },
    CommandInfo {
        name: "note.orphans",
        cli: "orphans",
        summary: "Notes with no links in or out",
    },
    CommandInfo {
        name: "link.unresolved",
        cli: "unresolved",
        summary: "Links to notes that do not exist, by target name",
    },
    CommandInfo {
        name: "link.resolve",
        cli: "resolve",
        summary: "Which note a name or link points to",
    },
    CommandInfo {
        name: "conflict.list",
        cli: "conflicts",
        summary: "Syncthing conflict copies",
    },
    CommandInfo {
        name: "note.open",
        cli: "open",
        summary: "Open a note in the desktop app",
    },
    CommandInfo {
        name: "agents.init",
        cli: "agents init",
        summary: "Write AGENTS.md with instructions for AI agents",
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
    /// The file to create already exists (`agents init`, A-05.4).
    AlreadyExists,
}

impl ErrorCode {
    /// The process exit code (A-03).
    pub fn exit_code(self) -> i32 {
        match self {
            Self::IoError | Self::NotUtf8 => exit::ERROR,
            Self::InvalidArgument => exit::INVALID_ARGUMENT,
            Self::VaultNotFound | Self::NoteNotFound => exit::NOT_FOUND,
            Self::AmbiguousNote | Self::AlreadyExists => exit::CONFLICT,
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

/// A note as every command shows it (A-02).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct NoteSummary {
    /// Relative to the vault root, `/` separators; usable as `<note>` in any command.
    pub path: String,
    pub title: String,
    /// `note`, `attachment` or `canvas`.
    pub kind: String,
    pub aliases: Vec<String>,
    /// Lowercase, without `#`.
    pub tags: Vec<String>,
    /// Modification time, ISO 8601 in UTC.
    pub modified: String,
    /// Size in bytes.
    pub size: u64,
}

/// A note named in another result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct NoteRef {
    pub path: String,
    pub title: String,
}

/// `note.list` (`patinote list`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct ListOutput {
    pub v: u32,
    /// Matches before `--limit`.
    pub total: usize,
    pub truncated: bool,
    pub notes: Vec<NoteSummary>,
    pub warnings: Vec<OutputWarning>,
}

/// Lines returned by `note.read`, 1-based and inclusive.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct LineRange {
    pub start: usize,
    pub end: usize,
    /// Lines in the whole note.
    pub total: usize,
}

/// A heading in `note.read`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct HeadingOut {
    pub level: u8,
    pub text: String,
    pub line: usize,
}

/// A link in `note.read` and `note.links`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct LinkOut {
    /// As written, without subpath or display text.
    pub target: String,
    /// Where it points, or `null` for a note that does not exist.
    pub path: Option<String>,
    /// `wiki`, `embed`, `markdown` or `frontmatter`.
    pub kind: String,
    pub subpath: Option<String>,
    pub display: Option<String>,
    pub line: usize,
    /// The exact text in the note.
    pub raw: String,
}

/// `note.read` (`patinote read`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct ReadOutput {
    pub v: u32,
    pub note: NoteSummary,
    /// The requested lines, joined with `\n`.
    pub content: String,
    pub lines: LineRange,
    /// Whether `content` was cut at the size limit (MCP reads 100 KB at a time).
    pub truncated: bool,
    /// The frontmatter as JSON, or `null`.
    pub frontmatter: Option<serde_json::Value>,
    pub headings: Vec<HeadingOut>,
    pub links: Vec<LinkOut>,
    pub warnings: Vec<OutputWarning>,
}

/// One `note.search` result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct SearchResult {
    pub path: String,
    pub title: String,
    /// First line with a match.
    pub line: usize,
    /// Text around the match, matches in `[` `]`.
    pub snippet: String,
}

/// `note.search` (`patinote search`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct SearchOutput {
    pub v: u32,
    pub query: String,
    pub total: usize,
    pub truncated: bool,
    pub results: Vec<SearchResult>,
    pub warnings: Vec<OutputWarning>,
}

/// One link in `note.backlinks`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct Backlink {
    /// The note the link is in.
    pub path: String,
    pub line: usize,
    /// `wiki`, `embed`, `markdown` or `frontmatter`.
    pub kind: String,
    /// The heading or `^block` linked to, if any.
    pub subpath: Option<String>,
    /// The line around the link.
    pub context: String,
}

/// `note.backlinks` (`patinote backlinks`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct BacklinksOutput {
    pub v: u32,
    pub note: NoteRef,
    pub total: usize,
    pub backlinks: Vec<Backlink>,
    pub warnings: Vec<OutputWarning>,
}

/// `note.links` (`patinote links`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct LinksOutput {
    pub v: u32,
    pub note: NoteRef,
    pub total: usize,
    pub links: Vec<LinkOut>,
    pub warnings: Vec<OutputWarning>,
}

/// One tag in `tag.list`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct TagOut {
    /// Lowercase, without `#`; nested tags keep their `/`.
    pub tag: String,
    /// The most common spelling.
    pub display: String,
    pub uses: usize,
    pub notes: usize,
}

/// `tag.list` (`patinote tags`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct TagsOutput {
    pub v: u32,
    pub total: usize,
    pub tags: Vec<TagOut>,
    pub warnings: Vec<OutputWarning>,
}

/// `note.orphans` (`patinote orphans`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct OrphansOutput {
    pub v: u32,
    pub total: usize,
    pub notes: Vec<NoteRef>,
    pub warnings: Vec<OutputWarning>,
}

/// Where a link is written.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct SourceLine {
    pub path: String,
    pub line: usize,
}

/// One missing note in `link.unresolved`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct UnresolvedTarget {
    /// The name as first written.
    pub target: String,
    pub count: usize,
    pub sources: Vec<SourceLine>,
}

/// `link.unresolved` (`patinote unresolved`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct UnresolvedOutput {
    pub v: u32,
    /// Distinct missing notes.
    pub total: usize,
    pub targets: Vec<UnresolvedTarget>,
    pub warnings: Vec<OutputWarning>,
}

/// `link.resolve` (`patinote resolve`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct ResolveOutput {
    pub v: u32,
    /// The name or link as given.
    pub input: String,
    /// The chosen note, or `null` when nothing matches.
    pub path: Option<String>,
    pub ambiguous: bool,
    /// Every match, the chosen one first.
    pub candidates: Vec<String>,
    pub warnings: Vec<OutputWarning>,
}

/// `note.open` (`patinote open`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct OpenOutput {
    pub v: u32,
    /// The note that was opened.
    pub note: NoteRef,
    /// `true` when the app was not running and has been started; `false`
    /// when a running app received the request.
    pub started: bool,
    pub warnings: Vec<OutputWarning>,
}

/// One Syncthing conflict copy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct ConflictOut {
    /// The copy.
    pub path: String,
    /// The note it conflicts with.
    pub original: String,
    pub modified: String,
    pub size: u64,
}

/// `conflict.list` (`patinote conflicts`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct ConflictsOutput {
    pub v: u32,
    pub total: usize,
    pub conflicts: Vec<ConflictOut>,
    pub warnings: Vec<OutputWarning>,
}

/// `agents.init` (`patinote agents init`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct AgentsInitOutput {
    pub v: u32,
    /// Files created or updated.
    pub written: Vec<String>,
    /// Files that already had the right content.
    pub unchanged: Vec<String>,
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
        ("note.list.json", schemars::schema_for!(ListOutput)),
        ("note.read.json", schemars::schema_for!(ReadOutput)),
        ("note.search.json", schemars::schema_for!(SearchOutput)),
        (
            "note.backlinks.json",
            schemars::schema_for!(BacklinksOutput),
        ),
        ("note.links.json", schemars::schema_for!(LinksOutput)),
        ("tag.list.json", schemars::schema_for!(TagsOutput)),
        ("note.orphans.json", schemars::schema_for!(OrphansOutput)),
        (
            "link.unresolved.json",
            schemars::schema_for!(UnresolvedOutput),
        ),
        ("link.resolve.json", schemars::schema_for!(ResolveOutput)),
        ("conflict.list.json", schemars::schema_for!(ConflictsOutput)),
        ("note.open.json", schemars::schema_for!(OpenOutput)),
        ("agents.init.json", schemars::schema_for!(AgentsInitOutput)),
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
