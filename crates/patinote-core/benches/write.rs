//! Creating a note in a folder of 10,000 notes through the write path. The
//! name lookup for files stored in NFD (Q-019) must not read the folder for
//! every new note.
//!
//! Run with `cargo bench -p patinote-core --bench write`.

use std::time::{Duration, Instant};

use criterion::{Criterion, criterion_group, criterion_main};
use patinote_core::write::{Actor, Expect, Journal, Op, Transaction, commit};

fn create_note(c: &mut Criterion) {
    let dir = tempfile::tempdir().expect("temp dir");
    let root = dir.path().join("vault");
    let folder = root.join("Catatan");
    std::fs::create_dir_all(&folder).expect("folder");
    for i in 0..10_000 {
        std::fs::write(folder.join(format!("Catatan {i}.md")), "isi\n").expect("note");
    }
    let mut group = c.benchmark_group("write");
    group.sample_size(30);
    for (name, stem) in [
        ("create_ascii_in_10k", "Baru"),
        ("create_accented_in_10k", "Kafé"),
    ] {
        group.bench_function(name, |b| {
            // Each create starts from the same state: the note and its
            // journal are removed outside the timing.
            b.iter_custom(|iters| {
                let mut total = Duration::ZERO;
                for _ in 0..iters {
                    let journal_dir = tempfile::tempdir().expect("journal dir");
                    let journal = Journal::new(journal_dir.path().join("journal"));
                    let path = format!("Catatan/{stem}.md");
                    let tx = Transaction {
                        actor: Actor::User,
                        ops: vec![Op::Write {
                            path: path.clone(),
                            content: b"x\n".to_vec(),
                            expect: Expect::Absent,
                        }],
                    };
                    let start = Instant::now();
                    commit(&root, &journal, &tx).expect("commit");
                    total += start.elapsed();
                    std::fs::remove_file(root.join(&path)).expect("remove");
                }
                total
            });
        });
    }
    group.finish();
}

/// A folder move as the app makes it: one move per note, 10,000 in one
/// transaction (`changed` then lists 20,000 paths).
fn move_folder(c: &mut Criterion) {
    let dir = tempfile::tempdir().expect("temp dir");
    let root = dir.path().join("vault");
    std::fs::create_dir_all(root.join("Satu")).expect("folder");
    let names: Vec<String> = (0..10_000).map(|i| format!("Catatan {i}.md")).collect();
    for n in &names {
        std::fs::write(root.join("Satu").join(n), "isi\n").expect("note");
    }
    let mut group = c.benchmark_group("write");
    group.sample_size(10);
    group.bench_function("move_folder_10k", |b| {
        b.iter_custom(|iters| {
            let mut total = Duration::ZERO;
            for _ in 0..iters {
                let journal_dir = tempfile::tempdir().expect("journal dir");
                let journal = Journal::new(journal_dir.path().join("journal"));
                let tx = Transaction {
                    actor: Actor::User,
                    ops: names
                        .iter()
                        .map(|n| Op::Move {
                            from: format!("Satu/{n}"),
                            to: format!("Dua/{n}"),
                            content: None,
                            expect: Expect::Hash(patinote_core::write::content_hash(b"isi\n")),
                        })
                        .collect(),
                };
                let start = Instant::now();
                commit(&root, &journal, &tx).expect("commit");
                total += start.elapsed();
                std::fs::remove_dir(root.join("Satu")).ok();
                std::fs::rename(root.join("Dua"), root.join("Satu")).expect("back");
            }
            total
        });
    });
    group.finish();
}

criterion_group!(benches, create_note, move_folder);
criterion_main!(benches);
