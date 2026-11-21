//! PRD target: renaming a note with its link updates takes under 1 s on a
//! 10,000-note vault. Measures what the rename dialog runs when the user
//! presses Enter (`commit_moves`: plan from the index, then one write-path
//! transaction), on the note with the most backlinks. Each run renames it
//! back and forth; the index is brought up to date between runs, as the app
//! does in the background. Exits with code 1 when the target is missed.
//!
//! Run with `cargo bench -p patinote-app --bench rename`.

use std::time::{Duration, Instant};

use patinote_app::app::commit_moves;
use patinote_core::relink::Moved;
use patinote_core::write::Journal;
use patinote_core::{IndexOptions, index_vault};
use patinote_index::{OpenOptions, SqliteStore};

const TARGET: Duration = Duration::from_secs(1);
const RUNS: usize = 7;

fn main() {
    let dir = tempfile::tempdir().expect("temp dir");
    let root = dir.path().join("vault-10k");
    let config = vaultgen::Config {
        notes: 10_000,
        seed: 1,
        long_note_lines: 0,
    };
    vaultgen::generate(&config, &root).expect("generate vault");
    let mut store =
        SqliteStore::open(&dir.path().join("index.db"), &OpenOptions::default()).expect("open");
    let index = |store: &mut SqliteStore| {
        index_vault(&root, store, &IndexOptions::default(), &mut |_| {}).expect("index");
    };
    index(&mut store);
    let journal = Journal::new(dir.path().join("journal"));

    let busiest = store
        .all_notes()
        .expect("notes")
        .into_iter()
        .filter(|n| n.kind == "note")
        .max_by_key(|n| store.backlinks(n.id).map_or(0, |b| b.len()))
        .expect("a note");
    let links = store.backlinks(busiest.id).expect("backlinks").len();
    let (folder, _) = busiest.path.rsplit_once('/').unwrap_or(("", &busiest.path));
    let renamed = if folder.is_empty() {
        "Renamed by bench.md".to_owned()
    } else {
        format!("{folder}/Renamed by bench.md")
    };

    let mut times = Vec::new();
    let mut changed = 0;
    for run in 0..RUNS {
        let (from, to) = if run % 2 == 0 {
            (&busiest.path, &renamed)
        } else {
            (&renamed, &busiest.path)
        };
        let moves = [Moved {
            from: from.clone(),
            to: to.clone(),
        }];
        let started = Instant::now();
        let files = commit_moves(&store, &root, &journal, &moves, None).expect("rename");
        times.push(started.elapsed());
        changed = files.len();
        index(&mut store);
    }
    times.sort();
    let (median, max) = (times[RUNS / 2], times[RUNS - 1]);
    let ok = median < TARGET;
    println!("rename: busiest note has {links} backlinks; {changed} files written per run");
    println!(
        "{} rename with link updates, 10k notes: median {median:.1?}, max {max:.1?} (target {TARGET:?})",
        if ok { "ok  " } else { "MISS" }
    );
    if !ok {
        std::process::exit(1);
    }
}
