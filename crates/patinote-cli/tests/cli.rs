//! The `patinote` binary end to end. Every run gets its own HOME and XDG
//! folders, so tests never touch the real cache, state or configuration.

use std::fs;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::Value;

struct Env {
    dir: tempfile::TempDir,
}

struct Run {
    code: i32,
    stdout: String,
    stderr: String,
}

impl Env {
    fn new() -> Self {
        Self {
            dir: tempfile::tempdir().unwrap(),
        }
    }
    fn path(&self, rel: &str) -> PathBuf {
        self.dir.path().join(rel)
    }
    fn cmd(&self, args: &[&str]) -> Command {
        let mut c = Command::new(env!("CARGO_BIN_EXE_patinote"));
        c.args(args)
            .env_clear()
            .env("HOME", self.path("home"))
            .env("XDG_CACHE_HOME", self.path("cache"))
            .env("XDG_STATE_HOME", self.path("state"))
            .env("XDG_CONFIG_HOME", self.path("config"))
            .stdin(Stdio::null());
        c
    }
    fn run(&self, args: &[&str]) -> Run {
        self.run_with(args, &[])
    }
    fn run_with(&self, args: &[&str], env: &[(&str, &str)]) -> Run {
        let mut c = self.cmd(args);
        for (k, v) in env {
            c.env(k, v);
        }
        let out = c.output().unwrap();
        Run {
            code: out.status.code().unwrap_or(-1),
            stdout: String::from_utf8(out.stdout).unwrap(),
            stderr: String::from_utf8(out.stderr).unwrap(),
        }
    }
    /// A copy of the fixture vault, so nothing can ever write to the fixture.
    fn fixture(&self) -> PathBuf {
        let dest = self.path("vault");
        copy_dir(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/vault-mockup"),
            &dest,
        );
        dest
    }
    fn write(&self, rel: &str, text: &str) {
        let p = self.path(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, text).unwrap();
    }
}

fn copy_dir(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for e in fs::read_dir(from).unwrap() {
        let e = e.unwrap();
        if e.file_type().unwrap().is_dir() {
            copy_dir(&e.path(), &to.join(e.file_name()));
        } else {
            fs::copy(e.path(), to.join(e.file_name())).unwrap();
        }
    }
}

fn json(text: &str) -> Value {
    serde_json::from_str(text).unwrap_or_else(|e| panic!("not one JSON document ({e}):\n{text}"))
}

fn hashes(root: &Path) -> Vec<(String, Vec<u8>)> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in fs::read_dir(&d).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                stack.push(p);
            } else {
                out.push((p.to_string_lossy().into_owned(), fs::read(&p).unwrap()));
            }
        }
    }
    out.sort();
    out
}

#[test]
fn f01_2_indexing_does_not_change_the_vault() {
    let env = Env::new();
    let vault = env.fixture();
    let before = hashes(&vault);
    let r = env.run(&["--vault", vault.to_str().unwrap(), "index"]);
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert_eq!(hashes(&vault), before);
}

#[test]
fn f01_9_vault_flag_beats_env_beats_config() {
    let env = Env::new();
    for name in ["flag", "env", "config"] {
        env.write(&format!("{name}/a.md"), "x\n");
    }
    let p = |n: &str| {
        fs::canonicalize(env.path(n))
            .unwrap()
            .to_string_lossy()
            .into_owned()
    };
    env.write(
        "config/patinote/config.toml",
        &format!(
            "config_version = 1\n[vault]\ndefault = \"{}\"\n",
            p("config")
        ),
    );
    let vault_of = |args: &[&str], envs: &[(&str, &str)]| {
        let mut all = args.to_vec();
        all.extend(["vault", "info", "--json"]);
        let r = env.run_with(&all, envs);
        assert_eq!(r.code, 0, "{}", r.stderr);
        json(&r.stdout)["vault"].as_str().unwrap().to_owned()
    };
    let env_path = p("env");
    assert_eq!(
        vault_of(&["--vault", &p("flag")], &[("PATINOTE_VAULT", &env_path)]),
        p("flag")
    );
    assert_eq!(vault_of(&[], &[("PATINOTE_VAULT", &env_path)]), p("env"));
    assert_eq!(vault_of(&[], &[]), p("config"));
}

#[test]
fn f01_errors_have_their_exit_codes() {
    let env = Env::new();
    env.write("file.md", "x");
    let missing = env.run(&["--vault", "/nonexistent/vault", "vault", "info", "--json"]);
    assert_eq!(missing.code, 3);
    assert!(missing.stdout.is_empty());
    assert_eq!(json(&missing.stderr)["error"]["code"], "vault_not_found");

    let file = env.run(&[
        "--vault",
        env.path("file.md").to_str().unwrap(),
        "vault",
        "info",
        "--json",
    ]);
    assert_eq!(
        (file.code, json(&file.stderr)["error"]["code"].as_str()),
        (2, Some("invalid_argument"))
    );

    let none = env.run(&["vault", "info", "--json"]);
    assert_eq!(
        (none.code, json(&none.stderr)["error"]["code"].as_str()),
        (2, Some("invalid_argument"))
    );

    let bad = env.run(&["frobnicate", "--json"]);
    assert_eq!(
        (bad.code, json(&bad.stderr)["error"]["code"].as_str()),
        (2, Some("invalid_argument"))
    );
    assert!(bad.stdout.is_empty());
}

#[test]
fn f03_5_deleting_the_cache_rebuilds_on_the_next_command() {
    let env = Env::new();
    let vault = env.fixture();
    let v = vault.to_str().unwrap();
    assert_eq!(env.run(&["--vault", v, "index"]).code, 0);
    fs::remove_dir_all(env.path("cache/patinote")).unwrap();
    let r = env.run(&["--vault", v, "vault", "info", "--json"]);
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert_eq!(json(&r.stdout)["notes"], 12);
}

#[test]
fn f03_7_reads_work_while_another_process_writes_and_writers_give_up_with_5() {
    let env = Env::new();
    let vault = env.fixture();
    let v = vault.to_str().unwrap();
    let info = json(&env.run(&["--vault", v, "vault", "info", "--json"]).stdout);
    let db = info["index"].as_str().unwrap().to_owned();

    // Another process (the app) holds the write lock.
    let holder = rusqlite::Connection::open(&db).unwrap();
    holder
        .execute_batch("BEGIN IMMEDIATE; UPDATE meta SET value = value;")
        .unwrap();

    let read = env.run(&["--vault", v, "--no-refresh", "vault", "info", "--json"]);
    assert_eq!(read.code, 0, "reading is not blocked: {}", read.stderr);

    fs::write(vault.join("Inbox/Ide cepat.md"), "diubah\n").unwrap();
    let started = Instant::now();
    let write = env.run(&["--vault", v, "index", "--json"]);
    assert_eq!(write.code, 5, "{}", write.stderr);
    assert_eq!(json(&write.stderr)["error"]["code"], "index_locked");
    assert!(
        started.elapsed() >= Duration::from_secs(5),
        "waited the full 5 seconds"
    );
    holder.execute_batch("ROLLBACK;").unwrap();
}

#[test]
fn f03_9_cache_clean_removes_indexes_of_vaults_that_are_gone() {
    let env = Env::new();
    env.write("a/n.md", "x\n");
    env.write("b/n.md", "x\n");
    for v in ["a", "b"] {
        assert_eq!(
            env.run(&["--vault", env.path(v).to_str().unwrap(), "index"])
                .code,
            0
        );
    }
    let id_b = json(
        &env.run(&[
            "--vault",
            env.path("b").to_str().unwrap(),
            "vault",
            "info",
            "--json",
        ])
        .stdout,
    )["vault_id"]
        .as_str()
        .unwrap()
        .to_owned();
    fs::remove_dir_all(env.path("b")).unwrap();
    let r = env.run(&["cache", "clean", "--json"]);
    assert_eq!(r.code, 0, "{}", r.stderr);
    let out = json(&r.stdout);
    assert_eq!(out["removed"], serde_json::json!([id_b]));
    assert_eq!(out["kept"], 1);
}

#[test]
fn a02_json_has_v_and_warnings_and_errors_go_to_stderr() {
    let env = Env::new();
    let vault = env.fixture();
    for args in [
        vec!["index", "--json"],
        vec!["vault", "info", "--json"],
        vec!["cache", "clean", "--json"],
    ] {
        let mut all = vec!["--vault", vault.to_str().unwrap()];
        all.extend(args.iter().copied());
        let r = env.run(&all);
        assert_eq!(r.code, 0, "{args:?}: {}", r.stderr);
        let doc = json(&r.stdout);
        assert_eq!(doc["v"], 1, "{args:?}");
        assert!(doc["warnings"].is_array(), "{args:?}");
    }
}

#[test]
fn a02_3_stdout_stays_json_while_progress_goes_to_stderr() {
    let env = Env::new();
    let vault = env.path("big");
    vaultgen::generate(
        &vaultgen::Config {
            notes: 600,
            seed: 2,
            long_note_lines: 0,
        },
        &vault,
    )
    .unwrap();
    let r = env.run_with(
        &["--vault", vault.to_str().unwrap(), "index", "--json"],
        &[("PATINOTE_PROGRESS_AFTER_MS", "0")],
    );
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert!(
        r.stderr.contains("indexing:"),
        "progress is shown: {}",
        r.stderr
    );
    assert_eq!(json(&r.stdout)["notes"], 600);
}

#[test]
fn a03_4_warnings_exit_0_and_strict_exits_1() {
    let env = Env::new();
    env.write("v/ok.md", "x\n");
    env.write("v/rusak.md", "---\njudul: [tidak ditutup\n---\nisi\n");
    let v = env.path("v");
    let r = env.run(&["--vault", v.to_str().unwrap(), "index", "--json"]);
    assert_eq!(r.code, 0, "{}", r.stderr);
    let w = &json(&r.stdout)["warnings"];
    assert_eq!(w.as_array().unwrap().len(), 1);
    assert_eq!(
        (w[0]["code"].as_str(), w[0]["path"].as_str()),
        (Some("frontmatter_invalid"), Some("rusak.md"))
    );
    let strict = env.run(&["--vault", v.to_str().unwrap(), "--strict", "index"]);
    assert_eq!(strict.code, 1);
    assert!(
        strict.stderr.contains("warning: rusak.md"),
        "{}",
        strict.stderr
    );
}

#[test]
fn a03_5_ctrl_c_during_index_exits_130_and_keeps_the_index_intact() {
    let env = Env::new();
    let vault = env.path("big");
    vaultgen::generate(
        &vaultgen::Config {
            notes: 4000,
            seed: 4,
            long_note_lines: 0,
        },
        &vault,
    )
    .unwrap();
    let mut child = env
        .cmd(&["--vault", vault.to_str().unwrap(), "index"])
        .env("PATINOTE_PROGRESS_AFTER_MS", "0")
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    // Wait until indexing has stored a batch, then press Ctrl+C.
    let mut lines = BufReader::new(child.stderr.take().unwrap()).lines();
    for line in lines.by_ref() {
        if line.unwrap().starts_with("indexing: 500/") {
            break;
        }
    }
    Command::new("kill")
        .args(["-INT", &child.id().to_string()])
        .status()
        .unwrap();
    let status = child.wait().unwrap();
    assert_eq!(status.code(), Some(130), "{status:?}");

    let info = env.run(&[
        "--vault",
        vault.to_str().unwrap(),
        "--no-refresh",
        "vault",
        "info",
        "--json",
    ]);
    let db = json(&info.stdout)["index"].as_str().unwrap().to_owned();
    let store = patinote_index_check(&db);
    assert!(store, "the index passes integrity_check after Ctrl+C");
    // The next run finishes the job.
    assert_eq!(
        env.run(&["--vault", vault.to_str().unwrap(), "index"]).code,
        0
    );
}

fn patinote_index_check(db: &str) -> bool {
    let conn = rusqlite::Connection::open(db).unwrap();
    conn.query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0))
        .unwrap()
        == "ok"
}

#[test]
fn a03_3_closed_pipe_ends_quietly() {
    let env = Env::new();
    let mut child = env
        .cmd(&["completions", "bash"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    // Read a little, then close the pipe like `| head -1`.
    let mut first = [0u8; 16];
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
fn a01_2_every_command_has_help_with_an_example() {
    let env = Env::new();
    for args in [
        vec!["--help"],
        vec!["index", "--help"],
        vec!["vault", "info", "--help"],
        vec!["cache", "clean", "--help"],
        vec!["completions", "--help"],
    ] {
        let r = env.run(&args);
        assert_eq!(r.code, 0, "{args:?}");
        assert!(r.stdout.contains("Examples:"), "{args:?}:\n{}", r.stdout);
    }
}

#[test]
fn snapshots_of_json_output() {
    let env = Env::new();
    let vault = env.fixture();
    let v = vault.to_str().unwrap();
    let mut index = json(&env.run(&["--vault", v, "index", "--json"]).stdout);
    index["duration_ms"] = Value::from(0);
    insta::assert_json_snapshot!("index", index);
    let mut info = json(&env.run(&["--vault", v, "vault", "info", "--json"]).stdout);
    for key in ["vault", "vault_id", "index"] {
        info[key] = Value::from(format!("[{key}]"));
    }
    insta::assert_json_snapshot!("vault_info", info);
}
