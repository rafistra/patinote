//! Index targets on a 10,000-note `vaultgen` vault (PRD F-03):
//! - `full_10k`: full index from nothing (F-03.1, under 10 s);
//! - `quick_check_10k`: run on an unchanged vault (F-03.8, under 200 ms);
//! - `open_10k`: open an existing index, including `integrity_check`.
//!
//! Run with `cargo bench -p patinote-index --bench index`.

use std::hint::black_box;

use criterion::{BatchSize, Criterion, criterion_group, criterion_main};
use patinote_core::{IndexOptions, index_vault};
use patinote_index::{OpenOptions, SqliteStore};

fn bench(c: &mut Criterion) {
    let dir = tempfile::tempdir().expect("temp dir");
    let root = dir.path().join("vault");
    let config = vaultgen::Config {
        notes: 10_000,
        seed: 1,
        long_note_lines: 0,
    };
    vaultgen::generate(&config, &root).expect("generate vault");
    let opts = IndexOptions::default();

    let mut group = c.benchmark_group("index");
    group.sample_size(10);
    group.bench_function("full_10k", |b| {
        b.iter_batched(
            || tempfile::tempdir().expect("db dir"),
            |db| {
                let mut s = SqliteStore::open(&db.path().join("index.db"), &OpenOptions::default())
                    .expect("open");
                black_box(index_vault(&root, &mut s, &opts, &mut |_| {}).expect("index"));
            },
            BatchSize::PerIteration,
        );
    });

    let db = dir.path().join("index.db");
    {
        let mut s = SqliteStore::open(&db, &OpenOptions::default()).expect("open");
        index_vault(&root, &mut s, &opts, &mut |_| {}).expect("index");
    }
    group.bench_function("quick_check_10k", |b| {
        let mut s = SqliteStore::open(&db, &OpenOptions::default()).expect("open");
        b.iter(|| black_box(index_vault(&root, &mut s, &opts, &mut |_| {}).expect("index")));
    });
    group.bench_function("open_10k", |b| {
        b.iter(|| black_box(SqliteStore::open(&db, &OpenOptions::default()).expect("open")));
    });
    group.finish();
}

criterion_group!(benches, bench);
criterion_main!(benches);
