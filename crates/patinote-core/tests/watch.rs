//! File watcher (PRD F-04). Timing assertions use the PRD limits (1 s, and
//! 30 s scaled down for polling) so they hold on slow CI runners too.

use std::fs;
use std::path::Path;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use patinote_core::watch::{check_watch_limit, watched_dirs};
use patinote_core::{
    Change, IndexOptions, MemoryStore, PollReason, WatchMode, WatchOptions, index_vault, watch,
};

fn write(root: &Path, rel: &str, text: &str) {
    let p = root.join(rel);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(p, text).unwrap();
}

/// Starts a watcher whose changes arrive on a channel.
fn start(
    root: &Path,
    options: WatchOptions,
) -> (patinote_core::VaultWatcher, mpsc::Receiver<Change>) {
    let (tx, rx) = mpsc::channel();
    let w = watch(root, options, move |c| {
        let _ = tx.send(c);
    })
    .unwrap();
    (w, rx)
}

fn next(rx: &mpsc::Receiver<Change>, within: Duration) -> Change {
    rx.recv_timeout(within)
        .expect("a change within the time limit")
}

/// Changed files only: folders that changed are reported too, which is fine.
fn files(c: &Change) -> Vec<&str> {
    c.paths
        .iter()
        .map(String::as_str)
        .filter(|p| p.ends_with(".md") || p.ends_with("ignore"))
        .collect()
}

fn index(root: &Path, store: &mut MemoryStore) -> patinote_core::IndexReport {
    index_vault(root, store, &IndexOptions::default(), &mut |_| {}).unwrap()
}

#[test]
fn f04_1_saved_note_is_reported_and_indexed_within_a_second() {
    let dir = tempfile::tempdir().unwrap();
    let r = dir.path();
    write(r, "Proyek/a.md", "satu\n");
    let mut store = MemoryStore::default();
    index(r, &mut store);
    let (_w, rx) = start(r, WatchOptions::default());

    // In place, like `:w` with Neovim's default `backupcopy`.
    let t = Instant::now();
    fs::write(r.join("Proyek/a.md"), "dua [[B]]\n").unwrap();
    let c = next(&rx, Duration::from_secs(1));
    assert_eq!(files(&c), ["Proyek/a.md"]);
    let rep = index(r, &mut store);
    assert_eq!(rep.updated, 1);
    assert!(
        t.elapsed() < Duration::from_secs(1),
        "reported and indexed in {:?}",
        t.elapsed()
    );

    // Atomic save: a temporary file renamed over the note.
    fs::write(r.join("Proyek/.a.md.tmp"), "tiga\n").unwrap();
    fs::rename(r.join("Proyek/.a.md.tmp"), r.join("Proyek/a.md")).unwrap();
    let c = next(&rx, Duration::from_secs(1));
    assert_eq!(
        files(&c),
        ["Proyek/a.md"],
        "the hidden temp file is not reported"
    );
    assert_eq!(index(r, &mut store).updated, 1);
}

#[test]
fn f04_2_new_note_is_reported() {
    let dir = tempfile::tempdir().unwrap();
    let (_w, rx) = start(dir.path(), WatchOptions::default());
    fs::write(dir.path().join("baru.md"), "isi\n").unwrap();
    assert_eq!(files(&next(&rx, Duration::from_secs(1))), ["baru.md"]);
}

#[test]
fn f04_3_deleted_note_leaves_the_index_and_its_links_dangle() {
    let dir = tempfile::tempdir().unwrap();
    let r = dir.path();
    write(r, "a.md", "[[b]]\n");
    write(r, "b.md", "isi\n");
    let mut store = MemoryStore::default();
    index(r, &mut store);
    assert_eq!(store.destination("a.md", 0), Some("b.md"));
    let (_w, rx) = start(r, WatchOptions::default());

    fs::remove_file(r.join("b.md")).unwrap();
    assert_eq!(files(&next(&rx, Duration::from_secs(1))), ["b.md"]);
    let rep = index(r, &mut store);
    assert_eq!(rep.removed, 1);
    assert!(!store.files.contains_key("b.md"));
    assert_eq!(
        store.destination("a.md", 0),
        None,
        "now a link to a note that does not exist"
    );
}

#[test]
fn f04_4_moved_folder_of_50_notes_is_one_change() {
    let dir = tempfile::tempdir().unwrap();
    let r = dir.path();
    for i in 0..50 {
        write(r, &format!("Lama/n{i}.md"), &format!("catatan {i}\n"));
    }
    let mut store = MemoryStore::default();
    index(r, &mut store);
    let (_w, rx) = start(r, WatchOptions::default());

    fs::rename(r.join("Lama"), r.join("Baru")).unwrap();
    let c = next(&rx, Duration::from_secs(1));
    assert!(
        c.paths.contains(&"Lama".to_owned()) && c.paths.contains(&"Baru".to_owned()),
        "{:?}",
        c.paths
    );
    assert!(
        rx.recv_timeout(Duration::from_millis(300)).is_err(),
        "one change only"
    );
    let rep = index(r, &mut store);
    assert_eq!((rep.added, rep.removed), (50, 50));
    assert!(store.files.keys().all(|p| p.starts_with("Baru/")));

    // The moved folder is still watched.
    fs::write(r.join("Baru/n0.md"), "diubah\n").unwrap();
    assert_eq!(files(&next(&rx, Duration::from_secs(1))), ["Baru/n0.md"]);
}

#[test]
fn f04_5_checkout_of_100_files_is_one_change_and_one_transaction() {
    let dir = tempfile::tempdir().unwrap();
    let r = dir.path();
    for i in 0..100 {
        write(r, &format!("d{}/n{i}.md", i % 5), "v1\n");
    }
    let mut store = MemoryStore::default();
    index(r, &mut store);
    let (_w, rx) = start(r, WatchOptions::default());

    // Like `git checkout`: many files rewritten in a burst.
    for i in 0..100 {
        fs::write(r.join(format!("d{}/n{i}.md", i % 5)), "v2 [[x]]\n").unwrap();
    }
    let c = next(&rx, Duration::from_secs(1));
    assert_eq!(files(&c).len(), 100);
    assert!(
        rx.recv_timeout(Duration::from_millis(300)).is_err(),
        "one change only"
    );
    let mut stored = 0;
    let rep = index_vault(r, &mut store, &IndexOptions::default(), &mut |p| {
        if let patinote_core::Progress::Stored { .. } = p {
            stored += 1;
        }
    })
    .unwrap();
    assert_eq!(rep.updated, 100);
    assert_eq!(stored, 1, "all 100 files in one transaction");
}

#[test]
fn f04_6_own_save_already_indexed_is_not_parsed_again() {
    let dir = tempfile::tempdir().unwrap();
    let r = dir.path();
    write(r, "a.md", "satu\n");
    let mut store = MemoryStore::default();
    index(r, &mut store);
    let (_w, rx) = start(r, WatchOptions::default());

    // Patinote saves and updates the index right away (the write path, step 1.6)...
    fs::write(r.join("a.md"), "dua\n").unwrap();
    assert_eq!(index(r, &mut store).updated, 1);
    // ...so the watcher's event finds nothing new to parse.
    assert_eq!(files(&next(&rx, Duration::from_secs(1))), ["a.md"]);
    let rep = index(r, &mut store);
    assert_eq!(rep.reprocessed(), 0);
}

#[test]
fn f04_7_polling_detects_changes_and_reports_why() {
    let dir = tempfile::tempdir().unwrap();
    let opts = WatchOptions {
        force_polling: true,
        poll_interval: Duration::from_millis(300),
        ..WatchOptions::default()
    };
    let (w, rx) = start(dir.path(), opts);
    assert_eq!(w.mode(), WatchMode::Polling(PollReason::Requested));
    fs::write(dir.path().join("a.md"), "x\n").unwrap();
    let c = next(&rx, Duration::from_secs(2));
    assert!(c.rescan && c.paths.is_empty());
    let mut store = MemoryStore::default();
    assert_eq!(index(dir.path(), &mut store).added, 1);

    // The warning shown when the vault needs too many watches.
    let warning = check_watch_limit(50_000, 65_536).unwrap();
    assert!(
        warning
            .sysctl_command()
            .starts_with("sudo sysctl fs.inotify.max_user_watches=")
    );
}

#[test]
fn hidden_folders_are_not_watched_but_patinote_ignore_is() {
    let dir = tempfile::tempdir().unwrap();
    let r = dir.path();
    write(r, ".git/objects/ab/cd", "x");
    write(r, ".obsidian/app.json", "{}");
    write(r, ".patinote/ignore", "Arsip/\n");
    write(r, "Proyek/a.md", "x");
    let dirs = watched_dirs(r);
    assert!(dirs.contains(&r.join("Proyek")) && dirs.contains(&r.join(".patinote")));
    assert!(
        !dirs
            .iter()
            .any(|d| d.starts_with(r.join(".git")) || d.starts_with(r.join(".obsidian")))
    );

    let (_w, rx) = start(r, WatchOptions::default());
    write(r, ".git/HEAD", "ref");
    write(r, ".obsidian/app.json", "{\"x\":1}");
    assert!(
        rx.recv_timeout(Duration::from_millis(400)).is_err(),
        "hidden folders are quiet"
    );
    write(r, ".patinote/ignore", "Arsip/\nDraf/\n");
    assert_eq!(
        files(&next(&rx, Duration::from_secs(1))),
        [".patinote/ignore"]
    );
}

#[test]
fn new_folders_are_watched() {
    let dir = tempfile::tempdir().unwrap();
    let r = dir.path();
    let (_w, rx) = start(r, WatchOptions::default());
    fs::create_dir(r.join("Baru")).unwrap();
    let c = next(&rx, Duration::from_secs(1));
    assert!(c.paths.contains(&"Baru".to_owned()), "{:?}", c.paths);
    // After the folder event, files inside it are reported too.
    fs::write(r.join("Baru/x.md"), "x\n").unwrap();
    assert_eq!(files(&next(&rx, Duration::from_secs(1))), ["Baru/x.md"]);
}

#[test]
fn dropping_the_watcher_stops_it() {
    let dir = tempfile::tempdir().unwrap();
    let (w, rx) = start(dir.path(), WatchOptions::default());
    drop(w);
    fs::write(dir.path().join("a.md"), "x\n").unwrap();
    assert!(rx.recv_timeout(Duration::from_millis(400)).is_err());
}
