//! Scanner speed. Listing a 10,000-note vault is the base cost of the quick
//! change check (PRD F-03.8, under 200 ms).
//!
//! Run with `cargo bench -p patinote-core --bench scan`.

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use patinote_core::{ScanOptions, scan};

fn scan_vault(c: &mut Criterion) {
    let dir = tempfile::tempdir().expect("temp dir");
    let root = dir.path().join("vault");
    let config = vaultgen::Config {
        notes: 10_000,
        seed: 1,
        long_note_lines: 0,
    };
    vaultgen::generate(&config, &root).expect("generate vault");

    let mut group = c.benchmark_group("scan");
    group.sample_size(20);
    group.bench_function("vault_10k", |b| {
        b.iter(|| black_box(scan(&root, &ScanOptions::default()).expect("scan")));
    });
    group.finish();
}

criterion_group!(benches, scan_vault);
criterion_main!(benches);
