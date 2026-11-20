//! Scans and parses the owner's real vault (PRD F-01.1, F-02.2). Runs only on
//! the owner's laptop:
//!
//! ```bash
//! PATINOTE_REAL_VAULT=~/Vault cargo test -p patinote-core --test real_vault -- --ignored --nocapture --test-threads 1
//! ```
//!
//! It prints counts only, never note names or content (CLAUDE.md, section 7).
//! Panic messages are silenced because they can quote note text.

use std::path::PathBuf;

use patinote_core::{FileKind, LinkKind, NoteContent, ScanOptions, load_note, parse, scan};

/// The vault from `PATINOTE_REAL_VAULT`, or `None` when it is not set.
fn vault_root() -> Option<PathBuf> {
    let Ok(raw) = std::env::var("PATINOTE_REAL_VAULT") else {
        eprintln!("PATINOTE_REAL_VAULT is not set; nothing to do");
        return None;
    };
    let root = match raw.strip_prefix("~/") {
        Some(rest) => PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(rest),
        None => PathBuf::from(raw),
    };
    // `cargo test` runs from the crate folder, so relative paths rarely mean
    // what was intended; fail loudly instead of reporting an empty vault.
    assert!(
        root.is_dir(),
        "PATINOTE_REAL_VAULT is not a folder (use an absolute path or ~/…)"
    );
    Some(root)
}

#[test]
#[ignore = "needs PATINOTE_REAL_VAULT; run only on the owner's laptop"]
fn f01_1_real_vault_counts() {
    let Some(root) = vault_root() else { return };
    let s = scan(&root, &ScanOptions::default()).expect("scan");
    let count = |k| s.files.iter().filter(|f| f.kind == k).count();
    eprintln!("notes:          {}", count(FileKind::Note));
    eprintln!("attachments:    {}", count(FileKind::Attachment));
    eprintln!("canvases:       {}", count(FileKind::Canvas));
    eprintln!("sync conflicts: {}", s.conflicts.len());
    eprintln!("scan issues:    {}", s.issues.len());
    eprintln!("Compare notes and attachments with Obsidian, then tick F-01.1.");
}

#[test]
#[ignore = "needs PATINOTE_REAL_VAULT; run only on the owner's laptop"]
fn f02_2_real_vault_parses_without_error() {
    let Some(root) = vault_root() else { return };
    let s = scan(&root, &ScanOptions::default()).expect("scan");

    std::panic::set_hook(Box::new(|_| {}));
    let (mut notes, mut not_utf8, mut too_large, mut unreadable, mut panics, mut warnings) =
        (0, 0, 0, 0, 0, 0);
    let mut kinds = [0usize; 4];
    let (mut tags, mut headings, mut blocks, mut properties) = (0, 0, 0, 0);
    for file in s.files.iter().filter(|f| f.kind == FileKind::Note) {
        let text = match load_note(&root.join(&file.path)) {
            Ok(NoteContent::Text(t)) => t,
            Ok(NoteContent::NotUtf8) => {
                not_utf8 += 1;
                continue;
            }
            Ok(NoteContent::TooLarge { .. }) => {
                too_large += 1;
                continue;
            }
            Err(_) => {
                unreadable += 1;
                continue;
            }
        };
        notes += 1;
        match std::panic::catch_unwind(|| parse(&text)) {
            Ok(note) => {
                for l in &note.links {
                    kinds[match l.kind {
                        LinkKind::Wiki => 0,
                        LinkKind::Embed => 1,
                        LinkKind::Markdown => 2,
                        LinkKind::Frontmatter => 3,
                    }] += 1;
                }
                tags += note.tags.len();
                headings += note.headings.len();
                blocks += note.blocks.len();
                properties += note.properties.len();
                warnings += note.warnings.len();
            }
            Err(_) => panics += 1,
        }
    }
    let _ = std::panic::take_hook();

    eprintln!("notes parsed:      {notes}");
    eprintln!("not UTF-8:         {not_utf8}");
    eprintln!("over 5 MB:         {too_large}");
    eprintln!("unreadable:        {unreadable}");
    eprintln!("panics:            {panics}");
    eprintln!("warnings:          {warnings}");
    eprintln!(
        "links wiki/embed/markdown/frontmatter: {}/{}/{}/{}",
        kinds[0], kinds[1], kinds[2], kinds[3]
    );
    eprintln!("tags {tags}, headings {headings}, blocks {blocks}, properties {properties}");
    assert_eq!(panics, 0, "the parser panicked on {panics} note(s)");
}
