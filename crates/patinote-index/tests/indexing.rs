//! The indexer with SqliteStore (F-03.4, F-03.6, F-01.2).

use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use patinote_core::store::Store;
use patinote_core::{IndexOptions, Progress, index_vault};
use patinote_index::{OpenOptions, RebuildReason, SqliteStore, index_path_for, update_index};

fn vault(dir: &Path, notes: usize) -> PathBuf {
    let root = dir.join("vault");
    vaultgen::generate(
        &vaultgen::Config {
            notes,
            seed: 3,
            long_note_lines: 0,
        },
        &root,
    )
    .unwrap();
    root
}

fn count(store: &SqliteStore, sql: &str) -> i64 {
    store.connection().query_row(sql, [], |r| r.get(0)).unwrap()
}

fn snapshot(root: &Path) -> Vec<(PathBuf, Vec<u8>, std::time::SystemTime)> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in fs::read_dir(&d).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                stack.push(p);
            } else {
                let m = fs::metadata(&p).unwrap();
                out.push((p.clone(), fs::read(&p).unwrap(), m.modified().unwrap()));
            }
        }
    }
    out.sort();
    out
}

#[test]
fn indexing_fills_every_table_and_leaves_the_vault_untouched() {
    let dir = tempfile::tempdir().unwrap();
    let root = vault(dir.path(), 300);
    let before = snapshot(&root);
    let mut store =
        SqliteStore::open(&dir.path().join("index.db"), &OpenOptions::default()).unwrap();
    let r = index_vault(&root, &mut store, &IndexOptions::default(), &mut |_| {}).unwrap();
    assert_eq!(
        snapshot(&root),
        before,
        "indexing must not change a byte of the vault (F-01.2)"
    );

    assert_eq!(
        count(&store, "SELECT count(*) FROM notes"),
        (r.notes + r.attachments) as i64
    );
    assert_eq!(
        count(&store, "SELECT count(*) FROM notes_fts"),
        r.notes as i64
    );
    assert!(count(&store, "SELECT count(*) FROM links") > 1000);
    assert!(count(&store, "SELECT count(*) FROM tags") > 100);
    assert!(count(&store, "SELECT count(*) FROM headings") > 900);
    assert_eq!(count(&store, "SELECT count(*) FROM blocks"), 300);
    assert!(count(&store, "SELECT count(*) FROM properties") > 500);
    assert_eq!(count(&store, "SELECT count(*) FROM warnings"), 0);
    assert!(store.meta("vault_path").unwrap().is_some());
}

#[test]
fn f03_6_corrupt_index_is_rebuilt_with_full_content() {
    let dir = tempfile::tempdir().unwrap();
    let root = vault(dir.path(), 100);
    let db = dir.path().join("index.db");
    {
        let mut s = SqliteStore::open(&db, &OpenOptions::default()).unwrap();
        index_vault(&root, &mut s, &IndexOptions::default(), &mut |_| {}).unwrap();
        s.connection()
            .execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
            .unwrap();
    }
    let mut bytes = fs::read(&db).unwrap();
    bytes[4096..].fill(0x5A);
    fs::write(&db, bytes).unwrap();

    let mut s = SqliteStore::open(&db, &OpenOptions::default()).unwrap();
    let r = update_index(&mut s, &root, &IndexOptions::default(), &mut |_| {}).unwrap();
    assert_eq!(s.rebuilt(), Some(RebuildReason::Corrupt));
    assert_eq!(r.added, r.notes + r.attachments);
    assert_eq!(count(&s, "SELECT count(*) FROM blocks"), 100);
    assert!(s.integrity_check().unwrap());
}

#[test]
fn update_index_without_damage_does_not_rebuild() {
    let dir = tempfile::tempdir().unwrap();
    let root = vault(dir.path(), 50);
    let mut s = SqliteStore::open(&dir.path().join("index.db"), &OpenOptions::default()).unwrap();
    update_index(&mut s, &root, &IndexOptions::default(), &mut |_| {}).unwrap();
    let r = update_index(&mut s, &root, &IndexOptions::default(), &mut |_| {}).unwrap();
    assert_eq!(s.rebuilt(), None);
    assert_eq!(r.unchanged, r.notes + r.attachments);
}

#[test]
fn index_path_is_per_vault() {
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("a");
    let b = dir.path().join("b");
    fs::create_dir_all(&a).unwrap();
    fs::create_dir_all(&b).unwrap();
    let cache = dir.path().join("cache");
    let pa = index_path_for(&a, &cache).unwrap();
    assert!(pa.starts_with(cache.join("patinote")) && pa.ends_with("index.db"));
    assert_ne!(pa, index_path_for(&b, &cache).unwrap());
    assert!(
        !pa.starts_with(&a),
        "the index never lives inside the vault"
    );
}

/// Child process for F-03.4: indexes slowly and reports each stored batch.
#[test]
#[ignore = "helper process for f03_4_kill_during_index_keeps_db_intact"]
fn index_child_process() {
    let Ok(dir) = std::env::var("PATINOTE_TEST_KILL_DIR") else {
        return;
    };
    let dir = PathBuf::from(dir);
    let mut s = SqliteStore::open(&dir.join("index.db"), &OpenOptions::default()).unwrap();
    let opts = IndexOptions {
        batch_size: 25,
        ..IndexOptions::default()
    };
    index_vault(&dir.join("vault"), &mut s, &opts, &mut |p| {
        if let Progress::Stored { done, .. } = p {
            println!("stored {done}");
            std::thread::sleep(Duration::from_millis(20));
        }
    })
    .unwrap();
    println!("finished");
}

#[test]
fn f03_4_kill_during_index_keeps_db_intact() {
    let dir = tempfile::tempdir().unwrap();
    let root = vault(dir.path(), 1500);
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "index_child_process",
            "--ignored",
            "--nocapture",
            "--test-threads",
            "1",
        ])
        .env("PATINOTE_TEST_KILL_DIR", dir.path())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    // Kill it after a few batches are committed, in the middle of the run.
    let mut lines = BufReader::new(child.stdout.take().unwrap()).lines();
    let mut batches = 0;
    for line in lines.by_ref() {
        let line = line.unwrap();
        assert_ne!(
            line, "finished",
            "the child finished before it could be killed"
        );
        if line.starts_with("stored") {
            batches += 1;
            if batches == 5 {
                break;
            }
        }
    }
    child.kill().unwrap(); // SIGKILL on Unix
    child.wait().unwrap();

    let mut s = SqliteStore::open(&dir.path().join("index.db"), &OpenOptions::default()).unwrap();
    assert_eq!(
        s.rebuilt(),
        None,
        "the index survived the kill and passed integrity_check"
    );
    let partial = count(&s, "SELECT count(*) FROM notes");
    assert!(
        (5 * 25..1530).contains(&partial),
        "{partial} files were stored before the kill"
    );

    let r = index_vault(&root, &mut s, &IndexOptions::default(), &mut |_| {}).unwrap();
    assert_eq!(
        count(&s, "SELECT count(*) FROM notes"),
        (r.notes + r.attachments) as i64
    );
    assert_eq!(
        r.unchanged as i64, partial,
        "the next run continues where the killed one stopped"
    );
}
