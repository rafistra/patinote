//! The read commands end to end (A-01 to A-03, F-01.3, F-01.8).

mod common;

use std::collections::BTreeSet;
use std::io::Read;
use std::path::Path;
use std::process::Stdio;

use common::{Env, json};
use serde_json::Value;

/// Every read command with arguments that work on the fixture vault.
const COMMANDS: &[(&str, &[&str])] = &[
    ("note.list", &["list"]),
    ("note.read", &["read", "Desain indeks Patinote"]),
    ("note.search", &["search", "indeks"]),
    ("note.backlinks", &["backlinks", "Patinote"]),
    ("note.links", &["links", "Desain indeks Patinote"]),
    ("tag.list", &["tags"]),
    ("note.orphans", &["orphans"]),
    ("link.unresolved", &["unresolved"]),
    ("link.resolve", &["resolve", "Rapat"]),
    ("conflict.list", &["conflicts"]),
    ("vault.info", &["vault", "info"]),
    ("index.update", &["index"]),
    ("cache.clean", &["cache", "clean"]),
];

fn with_vault<'a>(vault: &'a str, args: &[&'a str]) -> Vec<&'a str> {
    let mut all = vec!["--vault", vault];
    all.extend_from_slice(args);
    all
}

fn schema_keys(name: &str) -> BTreeSet<String> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("../../docs/schema/{name}.json"));
    let schema: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    schema["properties"]
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect()
}

fn keys(v: &Value) -> BTreeSet<String> {
    v.as_object().unwrap().keys().cloned().collect()
}

#[test]
fn a01_1_every_command_runs_without_input() {
    let env = Env::new();
    let vault = env.fixture();
    for (name, args) in COMMANDS {
        let r = env.run(&with_vault(vault.to_str().unwrap(), args));
        assert_eq!(r.code, 0, "{name}: {}", r.stderr);
    }
}

#[test]
fn a01_3_name_alias_and_path_give_the_same_result() {
    let env = Env::new();
    let vault = env.fixture();
    let v = vault.to_str().unwrap();
    let results: Vec<String> = [
        "Patinote",
        "second brain",
        "Proyek/Patinote.md",
        "Proyek/Patinote",
        "[[Patinote]]",
    ]
    .iter()
    .map(|n| {
        let r = env.run(&with_vault(v, &["backlinks", n, "--json"]));
        assert_eq!(r.code, 0, "{n}: {}", r.stderr);
        r.stdout
    })
    .collect();
    assert!(results.windows(2).all(|w| w[0] == w[1]));
    assert_eq!(json(&results[0])["total"], 4);
}

#[test]
fn a01_4_ambiguous_names_exit_4_with_candidates() {
    let env = Env::new();
    let vault = env.fixture();
    let r = env.run(&with_vault(
        vault.to_str().unwrap(),
        &["read", "Rapat", "--json"],
    ));
    assert_eq!(r.code, 4);
    assert!(r.stdout.is_empty());
    let err = json(&r.stderr);
    assert_eq!(err["error"]["code"], "ambiguous_note");
    assert_eq!(
        err["error"]["candidates"],
        serde_json::json!(["Proyek/Rapat.md", "Arsip/2025/Rapat.md"])
    );
    // A path picks one.
    assert_eq!(
        env.run(&with_vault(
            vault.to_str().unwrap(),
            &["read", "Arsip/2025/Rapat"]
        ))
        .code,
        0
    );
}

#[test]
fn a01_5_first_column_is_a_valid_path() {
    let env = Env::new();
    let vault = env.fixture();
    let r = env.run(&with_vault(
        vault.to_str().unwrap(),
        &["list", "--kind", "all", "--limit", "0"],
    ));
    assert_eq!(r.code, 0);
    let paths: Vec<&str> = r
        .stdout
        .lines()
        .map(|l| l.split('\t').next().unwrap())
        .collect();
    assert_eq!(paths.len(), 13);
    for p in paths {
        assert!(vault.join(p).is_file(), "{p}");
    }
}

#[test]
fn a02_1_json_matches_the_schema_for_every_command_and_error() {
    let env = Env::new();
    let vault = env.fixture();
    for (name, args) in COMMANDS {
        let mut all = with_vault(vault.to_str().unwrap(), args);
        all.push("--json");
        let r = env.run(&all);
        assert_eq!(r.code, 0, "{name}: {}", r.stderr);
        assert_eq!(keys(&json(&r.stdout)), schema_keys(name), "{name}");
    }
    for args in [
        vec!["read", "Rapat"],
        vec!["read", "Nothing here"],
        vec!["read", "../x"],
    ] {
        let mut all = with_vault(vault.to_str().unwrap(), &args);
        all.push("--json");
        let r = env.run(&all);
        assert_ne!(r.code, 0);
        let err = json(&r.stderr);
        assert_eq!(keys(&err), schema_keys("error"));
        assert_eq!(
            keys(&err["error"]),
            ["candidates", "code", "message"].map(String::from).into()
        );
    }
}

/// Removes values that change between runs: times and absolute paths.
fn redact(v: &mut Value) {
    match v {
        Value::Object(map) => {
            for (k, val) in map.iter_mut() {
                match k.as_str() {
                    "modified" | "duration_ms" => *val = Value::from("[redacted]"),
                    "vault" | "vault_id" | "index" if val.is_string() => {
                        *val = Value::from("[redacted]")
                    }
                    _ => redact(val),
                }
            }
        }
        Value::Array(items) => items.iter_mut().for_each(redact),
        _ => {}
    }
}

#[test]
fn a02_2_snapshots_guard_every_output() {
    let env = Env::new();
    let vault = env.fixture();
    for (name, args) in COMMANDS
        .iter()
        .filter(|(n, _)| !["index.update", "vault.info", "cache.clean"].contains(n))
    {
        let mut all = with_vault(vault.to_str().unwrap(), args);
        all.push("--json");
        let mut doc = json(&env.run(&all).stdout);
        redact(&mut doc);
        insta::assert_json_snapshot!(*name, doc);
    }
}

#[test]
fn a02_4_search_with_more_than_50_results_is_truncated() {
    let env = Env::new();
    let vault = env.path("big");
    vaultgen::generate(
        &vaultgen::Config {
            notes: 120,
            seed: 8,
            long_note_lines: 0,
        },
        &vault,
    )
    .unwrap();
    let r = env.run(&[
        "--vault",
        vault.to_str().unwrap(),
        "search",
        "Ringkasan",
        "--json",
    ]);
    let doc = json(&r.stdout);
    assert_eq!(doc["results"].as_array().unwrap().len(), 50);
    assert_eq!(doc["truncated"], true);
    assert_eq!(doc["total"], 120, "every note has a `## Ringkasan` heading");
    let all = json(
        &env.run(&[
            "--vault",
            vault.to_str().unwrap(),
            "search",
            "Ringkasan",
            "--limit",
            "0",
            "--json",
        ])
        .stdout,
    );
    assert_eq!(all["results"].as_array().unwrap().len(), 120);
}

#[test]
fn a02_5_every_path_in_output_works_as_a_note_argument() {
    let env = Env::new();
    let vault = env.fixture();
    let v = vault.to_str().unwrap();
    let list = json(
        &env.run(&with_vault(v, &["list", "--limit", "0", "--json"]))
            .stdout,
    );
    let mut paths: Vec<String> = list["notes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n["path"].as_str().unwrap().to_owned())
        .collect();
    let bl = json(
        &env.run(&with_vault(v, &["backlinks", "Patinote", "--json"]))
            .stdout,
    );
    paths.extend(
        bl["backlinks"]
            .as_array()
            .unwrap()
            .iter()
            .map(|b| b["path"].as_str().unwrap().to_owned()),
    );
    for p in paths {
        let r = env.run(&with_vault(v, &["read", &p, "--json"]));
        assert_eq!(r.code, 0, "{p}: {}", r.stderr);
        assert_eq!(json(&r.stdout)["note"]["path"], p.as_str());
    }
}

#[test]
fn a03_1_each_exit_code_row() {
    let env = Env::new();
    let vault = env.fixture();
    std::fs::write(vault.join("Latin.md"), b"caf\xe9\n").unwrap();
    let v = vault.to_str().unwrap();
    let code = |args: &[&str]| {
        let mut all = with_vault(v, args);
        all.push("--json");
        let r = env.run(&all);
        let error = if r.code == 0 {
            None
        } else {
            Some(
                json(&r.stderr)["error"]["code"]
                    .as_str()
                    .unwrap()
                    .to_owned(),
            )
        };
        (r.code, error)
    };
    assert_eq!(code(&["list"]), (0, None));
    assert_eq!(code(&["read", "Latin"]), (1, Some("not_utf8".into())));
    assert_eq!(
        code(&["read", "Rapat", "--lines", "9:2"]),
        (2, Some("invalid_argument".into()))
    );
    assert_eq!(
        code(&["read", "Tidak ada catatan ini"]),
        (3, Some("note_not_found".into()))
    );
    assert_eq!(
        code(&["backlinks", "Rapat"]),
        (4, Some("ambiguous_note".into()))
    );
    // 5 (index_locked) and 130 (Ctrl+C) are tested in cli.rs.
}

#[test]
fn a03_2_no_backlinks_is_exit_0_and_an_empty_list() {
    let env = Env::new();
    let vault = env.fixture();
    let r = env.run(&with_vault(
        vault.to_str().unwrap(),
        &["backlinks", "Ide cepat", "--json"],
    ));
    assert_eq!(r.code, 0);
    assert_eq!(json(&r.stdout)["backlinks"], serde_json::json!([]));
}

#[test]
fn a03_3_list_into_head_ends_without_broken_pipe() {
    let env = Env::new();
    let vault = env.path("big");
    vaultgen::generate(
        &vaultgen::Config {
            notes: 3000,
            seed: 1,
            long_note_lines: 0,
        },
        &vault,
    )
    .unwrap();
    assert_eq!(
        env.run(&["--vault", vault.to_str().unwrap(), "index"]).code,
        0
    );
    // The output is larger than the pipe buffer, so the writer is still
    // writing when the reader goes away, like `patinote list | head -1`.
    let mut child = env
        .cmd(&["--vault", vault.to_str().unwrap(), "list", "--limit", "0"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut first = [0u8; 64];
    child
        .stdout
        .as_mut()
        .unwrap()
        .read_exact(&mut first)
        .unwrap();
    drop(child.stdout.take());
    let mut err = String::new();
    child
        .stderr
        .take()
        .unwrap()
        .read_to_string(&mut err)
        .unwrap();
    let status = child.wait().unwrap();
    assert_eq!(status.code(), Some(0));
    assert!(!err.to_lowercase().contains("broken pipe"), "{err}");
}

#[test]
fn f01_3_hidden_folders_are_not_listed() {
    let env = Env::new();
    let vault = env.fixture();
    for rel in [
        ".git/HEAD",
        ".trash/old.md",
        ".hidden/x.md",
        "Proyek/.draft.md",
    ] {
        env.write(&format!("vault/{rel}"), "x\n");
    }
    let r = env.run(&with_vault(
        vault.to_str().unwrap(),
        &["list", "--kind", "all", "--limit", "0"],
    ));
    assert!(
        r.stdout
            .lines()
            .all(|l| !l.split('/').any(|p| p.starts_with('.'))),
        "{}",
        r.stdout
    );
    assert_eq!(r.stdout.lines().count(), 13);
}

#[test]
fn f01_8_a_6_mb_note_is_listed_but_not_found_by_search() {
    let env = Env::new();
    let vault = env.fixture();
    env.write("vault/Besar.md", &"zzunikkata ".repeat(600_000));
    let v = vault.to_str().unwrap();
    let list = env.run(&with_vault(v, &["list", "--limit", "0"]));
    assert!(list.stdout.lines().any(|l| l.starts_with("Besar.md\t")));
    let search = json(
        &env.run(&with_vault(v, &["search", "zzunikkata", "--json"]))
            .stdout,
    );
    assert_eq!(search["total"], 0);
}

/// A note named in NFD on disk is read by its name (Q-019).
#[test]
fn a_note_named_in_nfd_is_read() {
    let env = Env::new();
    let vault = env.fixture();
    std::fs::write(vault.join("Cafe\u{301}.md"), "kopi\n").unwrap();
    let v = vault.to_str().unwrap();
    let r = env.run(&["--vault", v, "read", "Café", "--json"]);
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert_eq!(json(&r.stdout)["content"], "kopi");
}
