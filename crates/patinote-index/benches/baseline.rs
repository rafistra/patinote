//! Baseline benchmarks: the cost of touching every file in a synthetic vault
//! without parsing or indexing anything. They are lower bounds for the
//! targets in docs/PRD.md (Performa): a full reindex of 10,000 notes in under
//! 10 s (F-03.1) and a quick check of an unchanged vault in under 200 ms
//! (F-03.8). Index benchmarks are added with the index in steps 1.3 and 1.9.
//!
//! Run with `cargo bench -p patinote-index`.

use std::fs;
use std::hint::black_box;
use std::path::{Path, PathBuf};

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};

/// Generates a vault of `notes` notes into a temporary folder.
fn vault(notes: usize) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("create temp dir");
    let root = dir.path().join("vault");
    let config = vaultgen::Config {
        notes,
        seed: 1,
        long_note_lines: 0,
    };
    vaultgen::generate(&config, &root).expect("generate vault");
    (dir, root)
}

/// Lists every non-hidden file under `root`, the way the scanner will skip
/// `.obsidian/` and other hidden folders.
fn files(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir).expect("read dir") {
            let entry = entry.expect("dir entry");
            if entry.file_name().to_string_lossy().starts_with('.') {
                continue;
            }
            let path = entry.path();
            if entry.file_type().expect("file type").is_dir() {
                stack.push(path);
            } else {
                out.push(path);
            }
        }
    }
    out
}

fn baseline(c: &mut Criterion) {
    let mut group = c.benchmark_group("baseline");
    group.sample_size(10);
    for notes in [1_000, 10_000] {
        let (_dir, root) = vault(notes);
        let count = files(&root).len() as u64;
        group.throughput(Throughput::Elements(count));

        // Walk and stat every file: the floor for the quick change check (F-03.8).
        group.bench_with_input(BenchmarkId::new("walk_stat", notes), &root, |b, root| {
            b.iter(|| {
                let mut total = 0u64;
                for path in files(root) {
                    total += fs::metadata(&path).expect("stat").len();
                }
                black_box(total)
            });
        });

        // Walk and read every file: the floor for a full reindex (F-03.1).
        group.bench_with_input(BenchmarkId::new("walk_read", notes), &root, |b, root| {
            b.iter(|| {
                let mut total = 0usize;
                for path in files(root) {
                    total += fs::read(&path).expect("read").len();
                }
                black_box(total)
            });
        });
    }
    group.finish();
}

criterion_group!(benches, baseline);
criterion_main!(benches);
