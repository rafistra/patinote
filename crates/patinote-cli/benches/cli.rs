//! A-01.6: on a 10,000-note vault, `backlinks`, `links`, `resolve` and
//! `read` finish in under 100 ms, including the quick index check. Measured
//! as the wall-clock time of the real binary, process start included.
//! Exits with code 1 when a command misses the target.
//!
//! Run with `cargo bench -p patinote-cli --bench cli`.

use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const TARGET: Duration = Duration::from_millis(100);

fn run(home: &Path, vault: &Path, args: &[&str]) -> Duration {
    let t = Instant::now();
    let status = Command::new(env!("CARGO_BIN_EXE_patinote"))
        .arg("--vault")
        .arg(vault)
        .args(args)
        .env_clear()
        .env("HOME", home)
        .env("XDG_CACHE_HOME", home.join("cache"))
        .env("XDG_STATE_HOME", home.join("state"))
        .env("XDG_CONFIG_HOME", home.join("config"))
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .expect("run patinote");
    let elapsed = t.elapsed();
    assert!(status.success(), "patinote {args:?} failed");
    elapsed
}

fn main() {
    let dir = tempfile::tempdir().expect("temp dir");
    let vault = dir.path().join("vault");
    let config = vaultgen::Config {
        notes: 10_000,
        seed: 1,
        long_note_lines: 0,
    };
    vaultgen::generate(&config, &vault).expect("generate vault");
    run(dir.path(), &vault, &["index"]);

    // A note that exists in every generated vault, and its first link.
    let note = std::fs::read_dir(vault.join("Inbox"))
        .expect("Inbox")
        .filter_map(Result::ok)
        .map(|e| e.path())
        .find(|p| p.extension().is_some_and(|x| x == "md"))
        .expect("a note");
    let rel = note
        .strip_prefix(&vault)
        .expect("inside")
        .to_string_lossy()
        .into_owned();
    let text = std::fs::read_to_string(&note).expect("read");
    let link = text
        .split("[[")
        .nth(1)
        .and_then(|s| s.split("]]").next())
        .map_or_else(|| rel.clone(), |s| format!("[[{s}]]"));

    let commands: [(&str, Vec<&str>); 4] = [
        ("backlinks", vec!["backlinks", &rel, "--json"]),
        ("links", vec!["links", &rel, "--json"]),
        ("resolve", vec!["resolve", &link, "--json"]),
        ("read", vec!["read", &rel, "--json"]),
    ];
    println!("| Command (10k notes) | Target | Median | Slowest | |");
    println!("| --- | --- | --- | --- | --- |");
    let mut failed = false;
    for (name, args) in &commands {
        let mut times: Vec<Duration> = (0..21).map(|_| run(dir.path(), &vault, args)).collect();
        times.sort();
        let (median, max) = (times[times.len() / 2], times[times.len() - 1]);
        let ok = median < TARGET;
        failed |= !ok;
        println!(
            "| `patinote {name}` | < {TARGET:?} | {median:.1?} | {max:.1?} | {} |",
            if ok { "ok" } else { "MISSED" }
        );
    }
    if failed {
        std::process::exit(1);
    }
}
