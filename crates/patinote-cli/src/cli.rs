//! Command-line arguments. Every command's `--help` has an example (A-01.2).

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};

/// Patinote: a fast, local second brain for Markdown vaults.
#[derive(Debug, Parser)]
#[command(
    name = "patinote",
    version,
    after_help = "Examples:\n  patinote index\n  patinote --vault ~/Vault vault info --json\n\nExit codes: 0 ok, 1 error, 2 bad arguments, 3 not found, 4 conflict or ambiguous, 5 index locked, 130 interrupted."
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
