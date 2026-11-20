//! Checks that every case in the parser golden corpus is well formed
//! (docs/design/0004-korpus-uji-parser.md). Comparing parser output against
//! the corpus is added with the parser in step 1.1 (F-02.1).

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

fn corpus_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden")
}

fn cases() -> Vec<(String, Vec<u8>, Value)> {
    let dir = corpus_dir();
    let mut stems = BTreeSet::new();
    for entry in fs::read_dir(&dir).unwrap() {
        let name = entry.unwrap().file_name().into_string().unwrap();
        let (stem, ext) = name.rsplit_once('.').unwrap();
        assert!(ext == "md" || ext == "json", "unexpected file {name}");
        stems.insert(stem.to_owned());
    }
    stems
        .into_iter()
        .map(|stem| {
            let md = fs::read(dir.join(format!("{stem}.md")))
                .unwrap_or_else(|_| panic!("{stem}: missing .md"));
            let json = fs::read_to_string(dir.join(format!("{stem}.json")))
                .unwrap_or_else(|_| panic!("{stem}: missing .json"));
            let value = serde_json::from_str(&json).unwrap_or_else(|e| panic!("{stem}: {e}"));
            (stem, md, value)
        })
        .collect()
}

fn keys(v: &Value) -> Vec<&str> {
    v.as_object().unwrap().keys().map(String::as_str).collect()
}

fn line_of(md: &[u8], offset: usize) -> u64 {
    md[..offset].iter().filter(|&&b| b == b'\n').count() as u64 + 1
}

fn line_text(md: &[u8], line: u64) -> String {
    let text = String::from_utf8_lossy(md);
    text.split('\n').nth(line as usize - 1).unwrap().to_owned()
}

fn opt_str(v: &Value) -> bool {
    v.is_null() || v.is_string()
}

#[test]
fn corpus_has_about_forty_cases() {
    let n = cases().len();
    assert!((35..=60).contains(&n), "{n} cases");
}

#[test]
fn every_case_is_well_formed() {
    for (stem, md, v) in cases() {
        assert_eq!(
            keys(&v),
            [
                "aliases",
                "blocks",
                "headings",
                "links",
                "properties",
                "tags",
                "title",
                "v",
                "warnings"
            ],
            "{stem}: top-level fields"
        );
        assert_eq!(v["v"], 1, "{stem}");
        assert!(opt_str(&v["title"]), "{stem}: title");
        assert!(
            v["aliases"]
                .as_array()
                .unwrap()
                .iter()
                .all(Value::is_string),
            "{stem}"
        );

        for l in v["links"].as_array().unwrap() {
            assert_eq!(
                keys(l),
                [
                    "byte_end",
                    "byte_start",
                    "display",
                    "kind",
                    "line",
                    "raw",
                    "subpath",
                    "target"
                ],
                "{stem}: link fields"
            );
            let kind = l["kind"].as_str().unwrap();
            assert!(
                ["wiki", "embed", "markdown", "frontmatter"].contains(&kind),
                "{stem}: kind {kind}"
            );
            assert!(l["target"].is_string() && opt_str(&l["subpath"]) && opt_str(&l["display"]));
            let start = l["byte_start"].as_u64().unwrap() as usize;
            let end = l["byte_end"].as_u64().unwrap() as usize;
            assert_eq!(
                &md[start..end],
                l["raw"].as_str().unwrap().as_bytes(),
                "{stem}: raw does not match bytes {start}..{end}"
            );
            assert_eq!(
                l["line"].as_u64().unwrap(),
                line_of(&md, start),
                "{stem}: link line"
            );
        }

        for t in v["tags"].as_array().unwrap() {
            assert_eq!(
                keys(t),
                ["display", "line", "source", "tag"],
                "{stem}: tag fields"
            );
            let display = t["display"].as_str().unwrap();
            assert_eq!(t["tag"].as_str().unwrap(), display.to_lowercase(), "{stem}");
            match t["source"].as_str().unwrap() {
                "inline" => {
                    let line = t["line"].as_u64().unwrap();
                    assert!(
                        line_text(&md, line).contains(&format!("#{display}")),
                        "{stem}: #{display} not on line {line}"
                    );
                }
                "frontmatter" => assert!(t["line"].is_null(), "{stem}"),
                other => panic!("{stem}: source {other}"),
            }
        }

        for h in v["headings"].as_array().unwrap() {
            assert_eq!(
                keys(h),
                ["byte_start", "level", "line", "text"],
                "{stem}: heading fields"
            );
            assert!((1..=6).contains(&h["level"].as_u64().unwrap()), "{stem}");
            let start = h["byte_start"].as_u64().unwrap() as usize;
            assert!(start == 0 || md[start - 1] == b'\n' || md[..start] == *b"\xef\xbb\xbf");
            let line = h["line"].as_u64().unwrap();
            assert_eq!(line, line_of(&md, start), "{stem}: heading line");
            assert!(
                line_text(&md, line).contains(h["text"].as_str().unwrap()),
                "{stem}"
            );
        }

        for b in v["blocks"].as_array().unwrap() {
            assert_eq!(keys(b), ["block_id", "line"], "{stem}: block fields");
            let id = b["block_id"].as_str().unwrap();
            assert!(
                id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'),
                "{stem}"
            );
            let line = line_text(&md, b["line"].as_u64().unwrap());
            assert!(
                line.trim_end().ends_with(&format!("^{id}")),
                "{stem}: block {id}"
            );
        }

        for p in v["properties"].as_array().unwrap() {
            assert_eq!(
                keys(p),
                ["idx", "key", "type", "value"],
                "{stem}: property fields"
            );
            let ok = match p["type"].as_str().unwrap() {
                "text" | "date" | "datetime" => p["value"].is_string(),
                "number" => p["value"].is_number(),
                "bool" => p["value"].is_boolean(),
                "null" => p["value"].is_null(),
                other => panic!("{stem}: type {other}"),
            };
            assert!(ok, "{stem}: value does not match type");
        }

        for w in v["warnings"].as_array().unwrap() {
            assert_eq!(keys(w), ["code"], "{stem}: warning fields");
        }
    }
}
