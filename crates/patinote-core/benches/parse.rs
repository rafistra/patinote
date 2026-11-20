//! Parser throughput (PRD F-02.6): parsing every note of a 10,000-note
//! `vaultgen` vault must fit in the 10 s full-reindex budget (F-03.1).
//!
//! Run with `cargo bench -p patinote-core`.

use std::hint::black_box;
use std::path::{Path, PathBuf};

use criterion::{Criterion, Throughput, criterion_group, criterion_main};

fn notes(root: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).expect("read dir") {
            let path: PathBuf = entry.expect("entry").path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "md") {
                out.push(std::fs::read_to_string(&path).expect("read note"));
            }
        }
    }
    out
}

fn parse_vault(c: &mut Criterion) {
    let dir = tempfile::tempdir().expect("temp dir");
    let root = dir.path().join("vault");
    let config = vaultgen::Config {
        notes: 10_000,
        seed: 1,
        long_note_lines: 0,
    };
    vaultgen::generate(&config, &root).expect("generate vault");
    let notes = notes(&root);
    let bytes: usize = notes.iter().map(String::len).sum();

    let mut group = c.benchmark_group("parse");
    group.sample_size(10);
    group.throughput(Throughput::Bytes(bytes as u64));
    group.bench_function("vault_10k", |b| {
        b.iter(|| {
            for note in &notes {
                black_box(patinote_core::parse(note));
            }
        });
    });
    group.finish();
}

criterion_group!(benches, parse_vault);
criterion_main!(benches);
