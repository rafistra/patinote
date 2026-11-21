//! Quick switcher matching on a 10,000-note vault from `vaultgen` (F-07.1:
//! under 16 ms per key press). Each iteration is one key press: the whole
//! vault is matched and ranked for the query typed so far.
//!
//! Run with `cargo bench -p patinote-app --bench switcher`.

use std::collections::HashMap;
use std::hint::black_box;

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use patinote_app::switcher::{Candidate, Finder};

fn candidates(notes: usize) -> Vec<Candidate> {
    let dir = tempfile::tempdir().expect("create temp dir");
    let root = dir.path().join("vault");
    let config = vaultgen::Config {
        notes,
        seed: 1,
        long_note_lines: 0,
    };
    vaultgen::generate(&config, &root).expect("generate vault");
    let mut out = Vec::new();
    let mut stack = vec![root.clone()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).expect("read dir") {
            let p = e.expect("entry").path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().is_some_and(|x| x == "md") {
                let rel = p.strip_prefix(&root).expect("inside the vault");
                out.push(Candidate::new(&rel.to_string_lossy(), Vec::new()));
            }
        }
    }
    out
}

fn bench(c: &mut Criterion) {
    let all = candidates(10_000);
    let recent: HashMap<String, usize> = all
        .iter()
        .take(50)
        .enumerate()
        .map(|(i, c)| (c.path.clone(), i))
        .collect();
    let mut group = c.benchmark_group("switcher_10k");
    let mut finder = Finder::new();
    for query in ["d", "ds", "dsn", "dsn i", "dsn ind", "kabut dermaga", ""] {
        group.bench_with_input(
            BenchmarkId::from_parameter(format!("{query:?}")),
            query,
            |b, q| b.iter(|| black_box(finder.find(&all, q, &recent))),
        );
    }
    group.finish();
}

criterion_group!(benches, bench);
criterion_main!(benches);
