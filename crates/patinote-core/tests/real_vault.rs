//! Parses the owner's real vault (PRD F-02.2). Runs only on the owner's laptop:
//!
//! ```bash
//! PATINOTE_REAL_VAULT=~/Vault cargo test -p patinote-core --test real_vault -- --ignored --nocapture
//! ```
//!
//! It prints counts only, never note names or content (CLAUDE.md, section 7).
//! Panic messages are silenced because they can quote note text.

use std::path::{Path, PathBuf};

use patinote_core::{LinkKind, parse};

fn markdown_files(root: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        if path.is_dir() {
            markdown_files(&path, out);
        } else if name.ends_with(".md") && !name.contains(".sync-conflict-") {
            out.push(path);
        }
    }
}

#[test]
#[ignore = "needs PATINOTE_REAL_VAULT; run only on the owner's laptop"]
fn f02_2_real_vault_parses_without_error() {
    let Ok(raw) = std::env::var("PATINOTE_REAL_VAULT") else {
        eprintln!("PATINOTE_REAL_VAULT is not set; nothing to do");
        return;
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
    let mut files = Vec::new();
    markdown_files(&root, &mut files);

    std::panic::set_hook(Box::new(|_| {}));
    let (mut notes, mut not_utf8, mut panics, mut warnings) = (0, 0, 0, 0);
    let mut kinds = [0usize; 4];
    let (mut tags, mut headings, mut blocks, mut properties) = (0, 0, 0, 0);
    for path in &files {
        let Ok(bytes) = std::fs::read(path) else {
            continue;
        };
        let Ok(text) = String::from_utf8(bytes) else {
            not_utf8 += 1;
            continue;
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
    eprintln!("panics:            {panics}");
    eprintln!("warnings:          {warnings}");
    eprintln!(
        "links wiki/embed/markdown/frontmatter: {}/{}/{}/{}",
        kinds[0], kinds[1], kinds[2], kinds[3]
    );
    eprintln!("tags {tags}, headings {headings}, blocks {blocks}, properties {properties}");
    assert_eq!(panics, 0, "the parser panicked on {panics} note(s)");
}
