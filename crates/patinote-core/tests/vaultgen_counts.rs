//! Cross-check: parsing a `vaultgen` vault gives the counts its summary reports.
//! Both were written independently, so agreement checks both.

use std::path::{Path, PathBuf};

use patinote_core::{LinkKind, TagSource, parse};

fn notes(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            if name.starts_with('.') {
                continue;
            }
            if path.is_dir() {
                stack.push(path);
            } else if name.ends_with(".md") && !name.contains(".sync-conflict-") {
                out.push(path);
            }
        }
    }
    out
}

#[test]
fn parse_counts_match_vaultgen_summary() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("vault");
    let config = vaultgen::Config {
        notes: 1000,
        seed: 11,
        long_note_lines: 300,
    };
    let s = vaultgen::generate(&config, &root).unwrap();

    let (mut wiki, mut embed, mut markdown, mut front) = (0, 0, 0, 0);
    let (mut inline_tags, mut fm_tags, mut headings, mut blocks, mut aliases, mut warnings) =
        (0, 0, 0, 0, 0, 0);
    let files = notes(&root);
    assert_eq!(files.len(), s.notes);
    for path in files {
        let note = parse(&std::fs::read_to_string(&path).unwrap());
        for l in &note.links {
            match l.kind {
                LinkKind::Wiki => wiki += 1,
                LinkKind::Embed => embed += 1,
                LinkKind::Markdown => markdown += 1,
                LinkKind::Frontmatter => front += 1,
            }
        }
        inline_tags += note
            .tags
            .iter()
            .filter(|t| t.source == TagSource::Inline)
            .count();
        fm_tags += note
            .tags
            .iter()
            .filter(|t| t.source == TagSource::Frontmatter)
            .count();
        headings += note.headings.len();
        blocks += note.blocks.len();
        aliases += note.aliases.len();
        warnings += note.warnings.len();
    }
    assert_eq!(
        (wiki, embed, markdown, front),
        (
            s.links_wiki,
            s.links_embed,
            s.links_markdown,
            s.links_frontmatter
        ),
        "links by kind"
    );
    assert_eq!(
        (inline_tags, fm_tags),
        (s.inline_tags, s.frontmatter_tags),
        "tags"
    );
    assert_eq!(
        (headings, blocks, aliases),
        (s.headings, s.blocks, s.aliases),
        "headings, blocks, aliases"
    );
    assert_eq!(warnings, 0);
}
