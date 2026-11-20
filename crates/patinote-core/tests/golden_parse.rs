//! Parser against the golden corpus (PRD F-02, docs/design/0004).

use std::fs;
use std::path::{Path, PathBuf};

use patinote_core::{LinkKind, WarningCode, parse};
use serde_json::{Value, json};

fn corpus_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden")
}

fn read_case(stem: &str) -> (String, Value) {
    let dir = corpus_dir();
    let md = fs::read_to_string(dir.join(format!("{stem}.md"))).unwrap();
    let json = fs::read_to_string(dir.join(format!("{stem}.json"))).unwrap();
    (md, serde_json::from_str(&json).unwrap())
}

fn actual(md: &str) -> Value {
    let mut v = serde_json::to_value(parse(md)).unwrap();
    v.as_object_mut().unwrap().insert("v".into(), json!(1));
    v
}

fn stems() -> Vec<String> {
    let mut stems: Vec<String> = fs::read_dir(corpus_dir())
        .unwrap()
        .filter_map(|e| e.unwrap().file_name().into_string().ok())
        .filter_map(|n| n.strip_suffix(".md").map(str::to_owned))
        .collect();
    stems.sort();
    stems
}

#[test]
fn f02_1_golden_corpus() {
    let mut failed = Vec::new();
    for stem in stems() {
        let (md, expected) = read_case(&stem);
        let got = actual(&md);
        if got != expected {
            eprintln!(
                "--- {stem}\nexpected: {}\n     got: {}",
                serde_json::to_string(&expected).unwrap(),
                serde_json::to_string(&got).unwrap()
            );
            failed.push(stem);
        }
    }
    assert!(
        failed.is_empty(),
        "{} case(s) differ: {failed:?}",
        failed.len()
    );
}

#[test]
fn f02_4_links_in_code_math_comments_are_ignored() {
    for stem in [
        "44-code-fenced",
        "45-code-indented",
        "46-inline-code-and-math",
        "47-comments",
    ] {
        let (md, _) = read_case(stem);
        let note = parse(&md);
        let targets: Vec<&str> = note.links.iter().map(|l| l.target.as_str()).collect();
        assert_eq!(targets, ["Luar"], "{stem}");
        assert!(note.tags.is_empty(), "{stem}: tags in excluded areas");
    }
}

#[test]
fn f02_5_broken_frontmatter_indexed_with_warning() {
    let (md, _) = read_case("41-frontmatter-invalid");
    let note = parse(&md);
    assert_eq!(note.warnings.len(), 1);
    assert_eq!(note.warnings[0].code, WarningCode::FrontmatterInvalid);
    assert!(note.properties.is_empty());
    assert_eq!(note.links.len(), 1);
    assert_eq!(note.links[0].kind, LinkKind::Wiki);
    assert_eq!(note.tags.len(), 1);
}

#[test]
fn parse_never_panics_on_odd_input() {
    for text in [
        "", "\u{feff}", "---", "---\n---", "[[", "]]", "#", "%%", "^", "![", "[](", "\r\n\r\n",
        "# \n=\n",
    ] {
        let _ = parse(text);
    }
}

#[test]
fn parse_never_panics_on_multibyte_insertions() {
    // Slicing a string in the middle of a character panics; insert multi-byte
    // characters at every position of every corpus case to find such slices.
    for stem in stems() {
        let (md, _) = read_case(&stem);
        let positions: Vec<usize> = md
            .char_indices()
            .map(|(i, _)| i)
            .chain([md.len()])
            .collect();
        for &at in &positions {
            for ins in ["é", "日", "🦀"] {
                let mut s = md.clone();
                s.insert_str(at, ins);
                let _ = parse(&s);
            }
        }
    }
}
