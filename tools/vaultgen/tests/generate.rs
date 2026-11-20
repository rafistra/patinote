//! Integration tests for `vaultgen::generate`.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use vaultgen::{Config, Summary, generate};

fn config(notes: usize, seed: u64) -> Config {
    Config {
        notes,
        seed,
        long_note_lines: 0,
    }
}

/// Reads every file under `root` as relative path → bytes.
fn tree(root: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut out = BTreeMap::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                stack.push(path);
            } else {
                let rel = path
                    .strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/");
                out.insert(rel, fs::read(&path).unwrap());
            }
        }
    }
    out
}

fn gen_into(dir: &Path, cfg: &Config) -> (Summary, BTreeMap<String, Vec<u8>>) {
    let summary = generate(cfg, dir).unwrap();
    (summary, tree(dir))
}

fn is_note(path: &str) -> bool {
    path.ends_with(".md") && !path.contains(".sync-conflict-")
}

/// Counts link openers outside fenced code and inline code, the way a parser
/// would see them. Returns (wiki-style including embeds and frontmatter, markdown).
fn count_links(text: &str) -> (usize, usize) {
    let mut wiki = 0;
    let mut markdown = 0;
    let mut in_fence = false;
    for line in text.lines() {
        if line.starts_with("```") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            continue;
        }
        let visible: String = line.split('`').step_by(2).collect();
        wiki += visible.matches("[[").count();
        markdown += visible.matches("](").count();
    }
    (wiki, markdown)
}

#[test]
fn same_seed_gives_identical_vault() {
    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    let (sa, ta) = gen_into(a.path(), &config(600, 42));
    let (sb, tb) = gen_into(b.path(), &config(600, 42));
    assert_eq!(sa, sb);
    assert_eq!(ta, tb);
}

#[test]
fn different_seed_gives_different_vault() {
    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    let (_, ta) = gen_into(a.path(), &config(200, 1));
    let (_, tb) = gen_into(b.path(), &config(200, 2));
    assert_ne!(ta, tb);
}

#[test]
fn file_counts_match_summary() {
    let dir = tempfile::tempdir().unwrap();
    let (s, t) = gen_into(dir.path(), &config(1000, 1));
    assert_eq!(s.notes, 1000);
    assert_eq!(t.keys().filter(|p| is_note(p)).count(), 1000);
    assert_eq!(s.attachments, 20);
    assert_eq!(t.keys().filter(|p| p.ends_with(".png")).count(), 20);
    assert_eq!(s.sync_conflicts, 2);
    assert_eq!(
        t.keys().filter(|p| p.contains(".sync-conflict-")).count(),
        2
    );
    assert!(s.ambiguous_names > 0 && s.aliases > 0 && s.links_unresolved > 0);
    assert!(s.crlf_notes > 0 && s.bom_notes > 0);
    assert!(t.contains_key(".obsidian/app.json"));
}

#[test]
fn link_counts_match_files() {
    let dir = tempfile::tempdir().unwrap();
    let (s, t) = gen_into(dir.path(), &config(1000, 7));
    let mut wiki = 0;
    let mut markdown = 0;
    let mut unresolved = 0;
    for (path, bytes) in &t {
        if !is_note(path) {
            continue;
        }
        let text = String::from_utf8(bytes.clone()).unwrap();
        let (w, m) = count_links(&text);
        wiki += w;
        markdown += m;
        unresolved += text.matches("[[Belum ditulis ").count();
    }
    assert_eq!(wiki, s.links_wiki + s.links_embed + s.links_frontmatter);
    assert_eq!(markdown, s.links_markdown);
    assert_eq!(unresolved, s.links_unresolved);
    assert!(!t.keys().any(|p| p.contains("Belum ditulis")));
}

#[test]
fn ambiguous_names_live_in_different_folders() {
    let dir = tempfile::tempdir().unwrap();
    let (s, t) = gen_into(dir.path(), &config(400, 3));
    let mut by_stem: BTreeMap<String, Vec<&String>> = BTreeMap::new();
    for path in t.keys().filter(|p| is_note(p)) {
        let stem = path
            .rsplit('/')
            .next()
            .unwrap()
            .trim_end_matches(".md")
            .to_lowercase();
        by_stem.entry(stem).or_default().push(path);
    }
    let shared = by_stem.values().filter(|v| v.len() > 1).count();
    assert_eq!(shared, s.ambiguous_names);
    assert_eq!(by_stem.values().map(Vec::len).max(), Some(2));
}

#[test]
fn long_note_has_requested_lines() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = Config {
        notes: 50,
        seed: 1,
        long_note_lines: 5000,
    };
    let (s, t) = gen_into(dir.path(), &cfg);
    let text = String::from_utf8(t["Catatan panjang.md"].clone()).unwrap();
    let lines = text.matches('\n').count();
    assert!(lines >= 5000);
    assert_eq!(s.long_note_lines, lines);
}

#[test]
fn refuses_non_empty_folder() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("keep.md"), "mine").unwrap();
    let err = generate(&config(10, 1), dir.path()).unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::AlreadyExists);
    assert_eq!(fs::read(dir.path().join("keep.md")).unwrap(), b"mine");
}

#[test]
fn creates_missing_folder_and_handles_tiny_vaults() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("a/b");
    let s = generate(&config(1, 1), &out).unwrap();
    assert_eq!(s.notes, 1);
    assert_eq!(s.attachments, 0);
    let empty = dir.path().join("empty");
    assert_eq!(generate(&config(0, 1), &empty).unwrap().notes, 0);
}
