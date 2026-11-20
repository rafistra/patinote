//! `vaultgen`: writes a deterministic synthetic vault and prints a JSON
//! summary of what it wrote to stdout.

use std::path::PathBuf;

use anyhow::Context as _;
use clap::Parser;

/// Generate a synthetic Patinote test vault.
///
/// Example: cargo run -p vaultgen -- --notes 10000 --out target/vaults/10k
#[derive(Parser)]
#[command(version)]
struct Args {
    /// Number of notes to write.
    #[arg(long)]
    notes: usize,
    /// Folder to write into. Must not exist or be empty.
    #[arg(long)]
    out: PathBuf,
    /// Seed. The same seed and options always give the same vault.
    #[arg(long, default_value_t = 1)]
    seed: u64,
    /// Pad the first note to at least this many lines (for typing latency tests).
    #[arg(long, default_value_t = 0)]
    long_note_lines: usize,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let config = vaultgen::Config {
        notes: args.notes,
        seed: args.seed,
        long_note_lines: args.long_note_lines,
    };
    let summary = vaultgen::generate(&config, &args.out)
        .with_context(|| format!("cannot generate a vault in {}", args.out.display()))?;
    println!("{}", summary.to_json());
    eprintln!(
        "vaultgen: wrote {} notes and {} attachments to {}",
        summary.notes,
        summary.attachments,
        args.out.display()
    );
    Ok(())
}
