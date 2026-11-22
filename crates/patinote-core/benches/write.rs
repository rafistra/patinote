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

criterion_group!(benches, create_note);
criterion_main!(benches);
