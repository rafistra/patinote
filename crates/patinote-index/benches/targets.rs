//! Checks the PRD performance targets that Phase 1 can measure, on
//! `vaultgen` vaults, and exits with code 1 when one is missed:
//!
//! | Target | PRD |
//! | --- | --- |
//! | full index from nothing, 10,000 notes, < 10 s | Performa, F-03.1 |
//! | quick check of an unchanged vault, 10,000 notes, < 200 ms | F-03.8 |
//! | backlinks with context lines, 10,000 notes, < 16 ms | Performa |
//! | full-text search, 50,000 notes, < 50 ms | Performa |
//!
//! Run with `cargo bench -p patinote-index --bench targets`. Each number is
//! the median of several runs; the slowest run is printed as well.

use std::collections::HashMap;
use std::path::Path;
use std::time::{Duration, Instant};

use patinote_core::{IndexOptions, index_vault};
use patinote_index::{OpenOptions, SqliteStore};

struct Row {
    name: String,
    target: Duration,
    median: Duration,
    max: Duration,
}

fn measure(runs: usize, mut f: impl FnMut()) -> (Duration, Duration) {
    let mut times: Vec<Duration> = (0..runs)
        .map(|_| {
            let t = Instant::now();
            f();
            t.elapsed()
        })
        .collect();
    times.sort();
    (times[runs / 2], times[runs - 1])
}

fn vault(dir: &Path, notes: usize) -> std::path::PathBuf {
    let root = dir.join(format!("vault-{notes}"));
    let config = vaultgen::Config {
        notes,
        seed: 1,
        long_note_lines: 0,
    };
    vaultgen::generate(&config, &root).expect("generate vault");
    root
}

fn indexed(root: &Path, db: &Path) -> SqliteStore {
    let mut s = SqliteStore::open(db, &OpenOptions::default()).expect("open");
    index_vault(root, &mut s, &IndexOptions::default(), &mut |_| {}).expect("index");
    s
}

fn main() {
    let dir = tempfile::tempdir().expect("temp dir");
    let opts = IndexOptions::default();
    let mut rows = Vec::new();

    let root = vault(dir.path(), 10_000);
    let mut n = 0;
    let (median, max) = measure(3, || {
        n += 1;
        let db = dir.path().join(format!("full-{n}.db"));
        drop(indexed(&root, &db));
    });
    rows.push(Row {
        name: "full index, 10k notes".into(),
        target: Duration::from_secs(10),
        median,
        max,
    });

    let db = dir.path().join("index-10k.db");
    let mut store = indexed(&root, &db);
    let (median, max) = measure(11, || {
        index_vault(&root, &mut store, &opts, &mut |_| {}).expect("index");
    });
    rows.push(Row {
        name: "quick check, 10k notes".into(),
        target: Duration::from_millis(200),
        median,
        max,
    });

    // The note with the most backlinks, with one context line per link, as
    // the backlinks panel and `patinote backlinks` show them.
    let notes = store.all_notes().expect("notes");
    let busiest = notes
        .iter()
        .max_by_key(|n| store.backlinks(n.id).expect("backlinks").len())
        .expect("a note")
        .id;
    let (median, max) = measure(21, || {
        let links = store.backlinks(busiest).expect("backlinks");
        let mut bodies: HashMap<i64, String> = HashMap::new();
        let mut context = 0;
        for l in &links {
            let body = bodies
                .entry(l.source_id)
                .or_insert_with(|| store.body(l.source_id).expect("body").unwrap_or_default());
            context += body
                .lines()
                .nth(l.line.saturating_sub(1))
                .map_or(0, str::len);
        }
        std::hint::black_box((links.len(), context));
    });
    println!(
        "backlinks: busiest note has {} links",
        store.backlinks(busiest).expect("backlinks").len()
    );
    rows.push(Row {
        name: "backlinks with context, 10k notes".into(),
        target: Duration::from_millis(16),
        median,
        max,
    });
    drop(store);

    let root = vault(dir.path(), 50_000);
    let store = indexed(&root, &dir.path().join("index-50k.db"));
    for query in [r#""kabut""#, r#""dermaga" "pantai""#, r#""angin pilar""#] {
        let (median, max) = measure(21, || {
            let (_, hits) = store.search(query, Some(50)).expect("search");
            // Like `patinote search`: the body of each hit gives the line.
            for h in &hits {
                std::hint::black_box(store.body(h.id).expect("body"));
            }
        });
        let (total, _) = store.search(query, None).expect("search");
        println!("search {query}: {total} matches");
        rows.push(Row {
            name: format!("search `{query}` (50 hits), 50k notes"),
            target: Duration::from_millis(50),
            median,
            max,
        });
    }

    report(&rows);
}

fn report(rows: &[Row]) {
    println!("\n| Measure | Target | Median | Slowest | |");
    println!("| --- | --- | --- | --- | --- |");
    let mut failed = false;
    for r in rows {
        let ok = r.median < r.target;
        failed |= !ok;
        println!(
            "| {} | < {:?} | {:.1?} | {:.1?} | {} |",
            r.name,
            r.target,
            r.median,
            r.max,
            if ok { "ok" } else { "MISSED" }
        );
    }
    if failed {
        std::process::exit(1);
    }
}
