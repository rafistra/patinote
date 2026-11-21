//! Incremental indexing with MemoryStore (F-03.2, F-03.3, F-01.4–F-01.6).

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::time::{Duration, SystemTime};

use patinote_core::store::FileWarningCode;
use patinote_core::{IndexOptions, IndexReport, MemoryStore, Progress, index_vault};

fn write(root: &Path, rel: &str, data: &[u8]) {
    let p = root.join(rel);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(p, data).unwrap();
}

fn index(root: &Path, store: &mut MemoryStore) -> IndexReport {
    index_vault(root, store, &IndexOptions::default(), &mut |_| {}).unwrap()
}

/// Moves a file's mtime so the size+mtime check sees a change.
fn bump_mtime(path: &Path) {
    let f = fs::File::options().write(true).open(path).unwrap();
    f.set_modified(SystemTime::now() + Duration::from_secs(60))
        .unwrap();
}

fn vault(notes: usize) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let cfg = vaultgen::Config {
        notes,
        seed: 5,
        long_note_lines: 0,
    };
    vaultgen::generate(&cfg, &dir.path().join("v")).unwrap();
    dir
}

#[test]
fn first_run_indexes_everything() {
    let dir = vault(200);
    let root = dir.path().join("v");
    let mut store = MemoryStore::default();
    let r = index(&root, &mut store);
    assert_eq!((r.notes, r.attachments), (200, 4));
    assert_eq!(r.added, 204);
    assert_eq!(r.sync_conflicts, 0);
    assert_eq!(store.files.len(), 204);
    assert!(store.meta.contains_key("vault_path"));
    let again = index(&root, &mut store);
    assert_eq!(
        (again.unchanged, again.reprocessed(), again.touched),
        (204, 0, 0)
    );
}

#[test]
fn f03_2_changing_one_note_reprocesses_one_file() {
    let dir = vault(200);
    let root = dir.path().join("v");
    let mut store = MemoryStore::default();
    index(&root, &mut store);
    let path = root
        .join("Inbox")
        .read_dir()
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let mut text = fs::read_to_string(&path).unwrap();
    text.push_str("\nBaris baru dengan [[Tujuan Baru]].\n");
    fs::write(&path, text).unwrap();
    bump_mtime(&path);

    let r = index(&root, &mut store);
    assert_eq!((r.updated, r.added, r.touched, r.removed), (1, 0, 0, 0));
    assert_eq!(r.unchanged, 203);
    let rel = path
        .strip_prefix(&root)
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let note = store.files[&rel].note.as_ref().unwrap();
    assert!(note.links.iter().any(|l| l.target == "Tujuan Baru"));
}

#[test]
fn f03_3_touched_file_is_not_parsed_again() {
    let dir = tempfile::tempdir().unwrap();
    let r0 = dir.path();
    write(r0, "a.md", b"# A\n[[B]]\n");
    let mut store = MemoryStore::default();
    index(r0, &mut store);
    bump_mtime(&r0.join("a.md"));
    let r = index(r0, &mut store);
    assert_eq!((r.touched, r.reprocessed()), (1, 0));
    let new_mtime = fs::metadata(r0.join("a.md")).unwrap().modified().unwrap();
    let ns = new_mtime
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_nanos() as i64;
    assert_eq!(store.files["a.md"].mtime_ns, ns);
}

#[test]
fn added_and_removed_files() {
    let dir = tempfile::tempdir().unwrap();
    let r0 = dir.path();
    write(r0, "a.md", b"a");
    write(r0, "b.md", b"b");
    let mut store = MemoryStore::default();
    index(r0, &mut store);
    fs::remove_file(r0.join("a.md")).unwrap();
    write(r0, "c.md", b"c");
    let r = index(r0, &mut store);
    assert_eq!((r.added, r.removed, r.unchanged), (1, 1, 1));
    assert_eq!(store.files.keys().collect::<Vec<_>>(), ["b.md", "c.md"]);
}

#[test]
fn f01_4_latin1_note_is_recorded_with_a_warning() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "Latin.md", b"caf\xe9");
    let mut store = MemoryStore::default();
    let r = index(dir.path(), &mut store);
    let f = &store.files["Latin.md"];
    assert!(f.note.is_none() && f.text.is_none() && f.hash.is_some());
    assert_eq!(f.warnings[0].code, FileWarningCode::NotUtf8);
    assert_eq!(r.warnings, 1);
}

#[test]
fn f01_5_unreadable_file_is_reported_and_others_indexed() {
    if fs::metadata("/proc/self")
        .map(|m| std::os::unix::fs::MetadataExt::uid(&m) == 0)
        .unwrap_or(false)
    {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "ok.md", b"[[x]]");
    write(dir.path(), "secret.md", b"y");
    write(dir.path(), "Locked/in.md", b"z");
    fs::set_permissions(
        dir.path().join("secret.md"),
        fs::Permissions::from_mode(0o000),
    )
    .unwrap();
    fs::set_permissions(dir.path().join("Locked"), fs::Permissions::from_mode(0o000)).unwrap();
    let mut store = MemoryStore::default();
    let r = index(dir.path(), &mut store);
    fs::set_permissions(dir.path().join("Locked"), fs::Permissions::from_mode(0o755)).unwrap();
    fs::set_permissions(
        dir.path().join("secret.md"),
        fs::Permissions::from_mode(0o644),
    )
    .unwrap();

    assert_eq!(
        store.files["secret.md"].warnings[0].code,
        FileWarningCode::Unreadable
    );
    assert!(store.files["ok.md"].note.is_some());
    assert_eq!(r.issues.len(), 1);
    assert_eq!(r.issues[0].path, "Locked");
}

#[test]
fn f01_6_empty_vault_gives_an_empty_index() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = MemoryStore::default();
    let r = index(dir.path(), &mut store);
    assert_eq!(
        r,
        IndexReport {
            duration: r.duration,
            ..IndexReport::default()
        }
    );
    assert!(store.files.is_empty());
}

#[test]
fn f01_8_large_note_is_stored_without_text() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "Big.md", &vec![b'a'; 6 * 1024 * 1024]);
    let mut store = MemoryStore::default();
    index(dir.path(), &mut store);
    let f = &store.files["Big.md"];
    assert!(f.text.is_none() && f.note.is_none() && f.hash.is_none());
    assert_eq!(f.warnings[0].code, FileWarningCode::TooLarge);
}

#[test]
fn broken_frontmatter_is_stored_with_a_warning() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "a.md", b"---\nx: [\n---\n[[B]]\n");
    let mut store = MemoryStore::default();
    index(dir.path(), &mut store);
    let f = &store.files["a.md"];
    assert_eq!(f.warnings[0].code, FileWarningCode::FrontmatterInvalid);
    assert_eq!(f.note.as_ref().unwrap().links.len(), 1);
}

#[test]
fn progress_and_batches() {
    let dir = vault(120);
    let mut store = MemoryStore::default();
    let mut seen = Vec::new();
    let opts = IndexOptions {
        batch_size: 50,
        ..IndexOptions::default()
    };
    index_vault(&dir.path().join("v"), &mut store, &opts, &mut |p| {
        seen.push(p)
    })
    .unwrap();
    assert_eq!(
        seen[0],
        Progress::Scanned {
            files: 122,
            changed: 122
        }
    );
    assert_eq!(
        seen.last(),
        Some(&Progress::Stored {
            done: 122,
            changed: 122
        })
    );
    assert_eq!(seen.len(), 1 + 2 + 1, "scan, two full batches, final batch");
}

#[test]
fn full_rereads_everything() {
    let dir = vault(30);
    let mut store = MemoryStore::default();
    index(&dir.path().join("v"), &mut store);
    let opts = IndexOptions {
        full: true,
        ..IndexOptions::default()
    };
    let r = index_vault(&dir.path().join("v"), &mut store, &opts, &mut |_| {}).unwrap();
    assert_eq!((r.unchanged, r.touched), (0, 30));
}

#[test]
fn f03_4_links_stored_before_a_stop_are_resolved_by_the_next_run() {
    use std::sync::atomic::{AtomicBool, Ordering};
    let dir = vault(200);
    let root = dir.path().join("v");
    let resolved = |s: &MemoryStore| {
        s.destinations
            .values()
            .filter(|(_, _, d)| d.is_some())
            .count()
    };
    let mut whole = MemoryStore::default();
    index(&root, &mut whole);

    // Stopped (Ctrl+C) once every file was stored, before links were
    // resolved: the next run finds nothing changed.
    let stop: &'static AtomicBool = Box::leak(Box::new(AtomicBool::new(false)));
    let options = IndexOptions {
        batch_size: 1,
        cancel: Some(stop),
        ..IndexOptions::default()
    };
    let mut store = MemoryStore::default();
    let stopped = index_vault(&root, &mut store, &options, &mut |p| {
        if let Progress::Stored { done, changed } = p
            && done == changed
        {
            stop.store(true, Ordering::Relaxed);
        }
    });
    assert!(stopped.is_err());
    assert_eq!(
        store.files.len(),
        whole.files.len(),
        "every file was stored"
    );

    index(&root, &mut store);
    assert_eq!(resolved(&store), resolved(&whole));
}
