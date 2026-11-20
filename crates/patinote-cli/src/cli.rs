//! Command-line arguments. Every command's `--help` has an example (A-01.2).

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};

use crate::commands::{DEFAULT_LIMIT, KindFilter, SortBy};

/// Patinote: a fast, local second brain for Markdown vaults.
#[derive(Debug, Parser)]
#[command(
    name = "patinote",
    version,
    after_help = "Examples:\n  patinote index\n  patinote --vault ~/Vault backlinks Patinote --json\n\nExit codes: 0 ok, 1 error, 2 bad arguments, 3 not found, 4 conflict or ambiguous, 5 index locked, 130 interrupted."
)]
pub struct Cli {
    #[command(flatten)]
    pub global: Global,
    #[command(subcommand)]
    pub command: Command,
}

/// Options for every command.
#[derive(Debug, Args)]
pub struct Global {
    /// Vault folder. Default: $PATINOTE_VAULT, then `[vault] default` in ~/.config/patinote/config.toml
    #[arg(long, global = true, value_name = "PATH")]
    pub vault: Option<PathBuf>,
    /// Print one JSON document on stdout; errors go to stderr as JSON
    #[arg(long, global = true)]
    pub json: bool,
    /// Skip the quick index refresh before the command
    #[arg(long, global = true)]
    pub no_refresh: bool,
    /// Print no progress or warnings on stderr
    #[arg(long, short, global = true)]
    pub quiet: bool,
    /// Exit with code 1 when there are warnings
    #[arg(long, global = true)]
    pub strict: bool,
    /// Color in text output
    #[arg(long, global = true, value_enum, default_value_t = ColorChoice::Auto)]
    pub color: ColorChoice,
}

/// When to use color.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ColorChoice {
    Auto,
    Always,
    Never,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Update the index, or rebuild it from scratch
    #[command(after_help = "Examples:\n  patinote index\n  patinote index --rebuild --json")]
    Index {
        /// Delete the index and build it again
        #[arg(long)]
        rebuild: bool,
    },
    /// Information about the vault
    #[command(subcommand)]
    Vault(VaultCommand),
    /// List notes, filtered by tag, folder or kind
    #[command(
        after_help = "Examples:\n  patinote list --tag proyek --sort modified\n  patinote list --folder Harian --limit 0 | cut -f1"
    )]
    List {
        /// Only notes with this tag or a tag nested under it
        #[arg(long)]
        tag: Option<String>,
        /// Only notes in this folder (and its subfolders)
        #[arg(long)]
        folder: Option<String>,
        #[arg(long, value_enum, default_value_t = KindFilter::Note)]
        kind: KindFilter,
        #[arg(long, value_enum, default_value_t = SortBy::Name)]
        sort: SortBy,
        /// Maximum results; 0 for all
        #[arg(long, default_value_t = DEFAULT_LIMIT)]
        limit: usize,
    },
    /// Read a note; with --json also its frontmatter, tags, headings and links
    #[command(
        after_help = "Examples:\n  patinote read \"Desain indeks Patinote\"\n  patinote read Proyek/Patinote.md --lines 1:20 --json"
    )]
    Read {
        /// Name, alias or path
        note: String,
        /// Lines to print, 1-based and inclusive, like 10:40 or 5:
        #[arg(long, value_name = "FROM:TO")]
        lines: Option<String>,
    },
    /// Search note text for words and "phrases"
    #[command(
        after_help = "Examples:\n  patinote search indeks\n  patinote search '\"skema SQLite\" fts5' --limit 10 --json"
    )]
    Search {
        /// Words and quoted phrases; every one must appear
        query: String,
        /// Maximum results; 0 for all
        #[arg(long, default_value_t = DEFAULT_LIMIT)]
        limit: usize,
    },
    /// Links to a note, with the line around each
    #[command(
        after_help = "Examples:\n  patinote backlinks Patinote\n  patinote backlinks \"Second Brain\" --json"
    )]
    Backlinks {
        /// Name, alias or path
        note: String,
    },
    /// Links in a note, including to notes that do not exist
    #[command(
        after_help = "Examples:\n  patinote links \"Desain indeks Patinote\"\n  patinote links Harian/2026-11-19.md --json"
    )]
    Links {
        /// Name, alias or path
        note: String,
    },
    /// Tags with how often they are used, nested tags included
    #[command(after_help = "Examples:\n  patinote tags\n  patinote tags --prefix proyek --json")]
    Tags {
        /// Only this tag and the tags nested under it
        #[arg(long)]
        prefix: Option<String>,
    },
    /// Notes with no links in or out
    #[command(after_help = "Examples:\n  patinote orphans\n  patinote orphans --json")]
    Orphans,
    /// Links to notes that do not exist, grouped by name
    #[command(after_help = "Examples:\n  patinote unresolved\n  patinote unresolved --json")]
    Unresolved,
    /// Which note a name or link points to
    #[command(
        after_help = "Examples:\n  patinote resolve Rapat\n  patinote resolve '[[Proyek/Rapat#Agenda]]' --json"
    )]
    Resolve {
        /// A name, alias, path or [[link]]
        name: String,
    },
    /// Syncthing conflict copies
    #[command(after_help = "Examples:\n  patinote conflicts\n  patinote conflicts --json")]
    Conflicts,
    /// Manage index caches
    #[command(subcommand)]
    Cache(CacheCommand),
    /// Print a shell completion script
    #[command(
        after_help = "Examples:\n  patinote completions bash > ~/.local/share/bash-completion/completions/patinote\n  patinote completions zsh > ~/.zfunc/_patinote"
    )]
    Completions {
        /// bash, zsh, fish, elvish or powershell
        shell: clap_complete::Shell,
    },
}

#[derive(Debug, Subcommand)]
pub enum VaultCommand {
    /// Vault and index location, file counts and warnings
    #[command(
        after_help = "Examples:\n  patinote vault info\n  patinote --vault ~/Vault vault info --json"
    )]
    Info,
}

#[derive(Debug, Subcommand)]
pub enum CacheCommand {
    /// Remove indexes of vaults that no longer exist
    #[command(after_help = "Examples:\n  patinote cache clean\n  patinote cache clean --json")]
    Clean,
}
