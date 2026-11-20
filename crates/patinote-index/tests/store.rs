//! SqliteStore: schema, batches, rebuilds, locking and cache cleaning (F-03).

use std::fs;
use std::path::Path;
use std::time::{Duration, Instant};

use patinote_core::scan::FileKind;
use patinote_core::store::{Batch, FileWarning, FileWarningCode, IndexedFile, Store};
use patinote_index::{
    CleanReport, IndexError, META_VAULT_PATH, OpenOptions, RebuildReason, SCHEMA_VERSION,
    SqliteStore, cache_clean,
};
use rusqlite::Connection;

fn note(path: &str, text: &str) -> IndexedFile {
    IndexedFile {
        path: path.to_owned(),
        kind: FileKind::Note,
        size: text.len() as u64,
        mtime_ns: 1,
        hash: Some([7; 32]),
        text: Some(text.to_owned()),
        note: Some(patinote_core::parse(text)),
        warnings: Vec::new(),
    }
}

fn count(store: &SqliteStore, sql: &str) -> i64 {
    store.connection().query_row(sql, [], |r| r.get(0)).unwrap()
}

fn open(path: &Path) -> SqliteStore {
    SqliteStore::open(path, &OpenOptions::default()).unwrap()
}

#[test]
fn new_index_has_schema_wal_and_fts5() {
    let dir = tempfile::tempdir().unwrap();
    let s = open(&dir.path().join("v/index.db"));
    assert!(s.created() && s.rebuilt().is_none());
    assert_eq!(count(&s, "PRAGMA user_version"), SCHEMA_VERSION as i64);
    let mode: String = s
        .connection()
        .query_row("PRAGMA journal_mode", [], |r| r.get(0))
        .unwrap();
    assert_eq!(mode, "wal");
    assert_eq!(count(&s, "PRAGMA foreign_keys"), 1);
    assert_eq!(count(&s, "PRAGMA mmap_size"), 0);
    for table in [
        "meta",
        "notes",
        "aliases",
        "links",
        "tags",
        "headings",
        "blocks",
        "properties",
        "warnings",
        "notes_fts",
    ] {
        assert_eq!(
            count(
                &s,
                &format!("SELECT count(*) FROM sqlite_master WHERE name = '{table}'")
            ),
            1,
            "{table}"
        );
    }
}

#[test]
fn apply_stores_replaces_and_removes_notes() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = open(&dir.path().join("index.db"));
    let text = "---\naliases: [Otak Kedua]\ntags: [proyek]\ncreated: 2026-11-19\n---\n# Judul\n\nLihat [[Rapat#Agenda|rapat]] dan #ide. ^b1\n";
    s.apply(&Batch {
        upsert: vec![note("Proyek/Catatan.md", text)],
        ..Batch::default()
    })
    .unwrap();

    assert_eq!(count(&s, "SELECT count(*) FROM notes"), 1);
    let (title, stem_lower, ext, fm): (String, String, String, String) = s
        .connection()
        .query_row(
            "SELECT title, stem_lower, ext, frontmatter FROM notes",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .unwrap();
    assert_eq!(
        (title.as_str(), stem_lower.as_str(), ext.as_str()),
        ("Judul", "catatan", "md")
    );
    assert!(fm.contains("\"aliases\""));
    assert_eq!(
        count(
            &s,
            "SELECT count(*) FROM aliases WHERE alias_lower = 'otak kedua'"
        ),
        1
    );
    assert_eq!(
        count(
            &s,
            "SELECT count(*) FROM links WHERE target_lower = 'rapat' AND dst_id IS NULL AND subpath = 'Agenda'"
        ),
        1
    );
    assert_eq!(count(&s, "SELECT count(*) FROM tags"), 2);
    assert_eq!(count(&s, "SELECT count(*) FROM headings"), 1);
    assert_eq!(count(&s, "SELECT count(*) FROM blocks"), 1);
    assert_eq!(
        count(
            &s,
            "SELECT CAST(value_num AS INTEGER) FROM properties WHERE key = 'created'"
        ),
        1_795_046_400
    );
    assert_eq!(
        count(
            &s,
            "SELECT count(*) FROM notes_fts WHERE notes_fts MATCH 'rapat'"
        ),
        1
    );

    // Replacing keeps the row id and swaps every derived row.
    let id_before = count(&s, "SELECT id FROM notes");
    s.apply(&Batch {
        upsert: vec![note("Proyek/Catatan.md", "Baru [[Lain]]\n")],
        ..Batch::default()
    })
    .unwrap();
    assert_eq!(count(&s, "SELECT id FROM notes"), id_before);
    assert_eq!(count(&s, "SELECT count(*) FROM links"), 1);
    assert_eq!(
        count(&s, "SELECT count(*) FROM aliases") + count(&s, "SELECT count(*) FROM tags"),
        0
    );
    assert_eq!(
        count(
            &s,
            "SELECT count(*) FROM notes_fts WHERE notes_fts MATCH 'rapat'"
        ),
        0
    );

    // Touch only moves the mtime; remove deletes the note, its rows and its text.
    s.apply(&Batch {
        touch: vec![("Proyek/Catatan.md".into(), 99)],
        ..Batch::default()
    })
    .unwrap();
    assert_eq!(s.file_states().unwrap()[0].mtime_ns, 99);
    s.apply(&Batch {
        remove: vec!["Proyek/Catatan.md".into()],
        ..Batch::default()
    })
    .unwrap();
    for table in ["notes", "links", "notes_fts"] {
        assert_eq!(
            count(&s, &format!("SELECT count(*) FROM {table}")),
            0,
            "{table}"
        );
    }
}

#[test]
fn warnings_and_attachments_are_stored() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = open(&dir.path().join("index.db"));
    let latin1 = IndexedFile {
        path: "Café.md".into(),
        kind: FileKind::Note,
        size: 10,
        mtime_ns: 1,
        hash: Some([1; 32]),
        text: None,
        note: None,
        warnings: vec![FileWarning {
            code: FileWarningCode::NotUtf8,
            line: None,
        }],
    };
    let image = IndexedFile {
        path: "img/a.png".into(),
        kind: FileKind::Attachment,
        hash: None,
        warnings: vec![],
        ..latin1.clone()
    };
    s.apply(&Batch {
        upsert: vec![latin1, image],
        ..Batch::default()
    })
    .unwrap();
    assert_eq!(
        count(&s, "SELECT count(*) FROM warnings WHERE code = 'not_utf8'"),
        1
    );
    assert_eq!(count(&s, "SELECT count(*) FROM notes_fts"), 0);
    assert_eq!(
        count(
            &s,
            "SELECT count(*) FROM notes WHERE kind = 'attachment' AND title IS NULL AND hash IS NULL"
        ),
        1
    );
    let states = s.file_states().unwrap();
    assert_eq!(
        states.iter().find(|f| f.path == "Café.md").unwrap().hash,
        Some([1; 32])
    );
}

#[test]
fn meta_round_trips() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = open(&dir.path().join("index.db"));
    assert_eq!(s.meta("k").unwrap(), None);
    s.set_meta("k", "a").unwrap();
    s.set_meta("k", "b").unwrap();
    assert_eq!(s.meta("k").unwrap().as_deref(), Some("b"));
}

#[test]
fn f03_5_deleted_cache_is_recreated() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("cache/patinote/0123456789abcdef/index.db");
    drop(open(&path));
    fs::remove_dir_all(dir.path().join("cache")).unwrap();
    let s = open(&path);
    assert!(s.created());
    assert_eq!(count(&s, "SELECT count(*) FROM notes"), 0);
}

#[test]
fn f03_6_corrupt_database_is_rebuilt() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("index.db");
    {
        let mut s = open(&path);
        s.apply(&Batch {
            upsert: vec![note("a.md", "x")],
            ..Batch::default()
        })
        .unwrap();
    }
    for suffix in ["-wal", "-shm"] {
        let _ = fs::remove_file(format!("{}{suffix}", path.display()));
    }
    fs::write(
        &path,
        b"this is not a database at all, just garbage bytes ".repeat(100),
    )
    .unwrap();
    let s = open(&path);
    assert_eq!(s.rebuilt(), Some(RebuildReason::Corrupt));
    assert_eq!(count(&s, "SELECT count(*) FROM notes"), 0);
}

#[test]
fn f03_6_damaged_pages_are_detected() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("index.db");
    {
        let mut s = open(&path);
        let many: Vec<IndexedFile> = (0..300)
            .map(|i| note(&format!("n{i}.md"), &"isi [[x]] #t\n".repeat(50)))
            .collect();
        s.apply(&Batch {
            upsert: many,
            ..Batch::default()
        })
        .unwrap();
        s.connection()
            .execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
            .unwrap();
    }
    // Overwrite the middle of the file, keeping the header intact.
    let mut bytes = fs::read(&path).unwrap();
    let mid = bytes.len() / 2;
    for b in &mut bytes[mid..mid + 8192] {
        *b = 0xA5;
    }
    fs::write(&path, bytes).unwrap();
    let s = open(&path);
    assert_eq!(s.rebuilt(), Some(RebuildReason::Corrupt));
}

#[test]
fn newer_schema_is_rebuilt() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("index.db");
    drop(open(&path));
    Connection::open(&path)
        .unwrap()
        .execute_batch("PRAGMA user_version = 99;")
        .unwrap();
    let s = open(&path);
    assert_eq!(s.rebuilt(), Some(RebuildReason::NewerSchema));
    assert_eq!(count(&s, "PRAGMA user_version"), SCHEMA_VERSION as i64);
}

#[test]
fn f03_7_writer_waits_then_reports_locked_while_reads_continue() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("index.db");
    let mut app = open(&path);
    app.apply(&Batch {
        upsert: vec![note("a.md", "x")],
        ..Batch::default()
    })
    .unwrap();

    // The "app" holds the write lock.
    let holder = Connection::open(&path).unwrap();
    holder
        .execute_batch("BEGIN IMMEDIATE; UPDATE notes SET size = 2;")
        .unwrap();

    let mut cli = SqliteStore::open(
        &path,
        &OpenOptions {
            busy_timeout: Duration::from_millis(300),
        },
    )
    .unwrap();
    assert_eq!(
        cli.file_states().unwrap().len(),
        1,
        "reads are not blocked (WAL)"
    );
    let started = Instant::now();
    let err = cli
        .apply(&Batch {
            upsert: vec![note("b.md", "y")],
            ..Batch::default()
        })
        .unwrap_err();
    assert!(matches!(err, IndexError::Locked), "{err:?}");
    assert!(
        started.elapsed() >= Duration::from_millis(250),
        "the writer waited for the busy timeout"
    );

    holder.execute_batch("COMMIT;").unwrap();
    cli.apply(&Batch {
        upsert: vec![note("b.md", "y")],
        ..Batch::default()
    })
    .unwrap();
    assert_eq!(cli.file_states().unwrap().len(), 2);
}

#[test]
fn f03_9_cache_clean_removes_only_indexes_of_missing_vaults() {
    let cache = tempfile::tempdir().unwrap();
    let alive = tempfile::tempdir().unwrap();
    let make = |id: &str, vault: Option<&Path>| {
        let mut s = open(&cache.path().join("patinote").join(id).join("index.db"));
        if let Some(v) = vault {
            s.set_meta(META_VAULT_PATH, &v.to_string_lossy()).unwrap();
        }
    };
    make("aaaaaaaaaaaaaaaa", Some(alive.path()));
    make(
        "bbbbbbbbbbbbbbbb",
        Some(Path::new("/nonexistent/vault/that/was/moved")),
    );
    make("cccccccccccccccc", None);
    fs::create_dir_all(cache.path().join("patinote/not-a-vault-id")).unwrap();

    let report = cache_clean(cache.path()).unwrap();
    assert_eq!(
        report,
        CleanReport {
            removed: vec!["bbbbbbbbbbbbbbbb".into()],
            kept: 1,
            unknown: vec!["cccccccccccccccc".into()]
        }
    );
    let left: Vec<String> = fs::read_dir(cache.path().join("patinote"))
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    assert_eq!(left.len(), 3);
    assert!(!left.contains(&"bbbbbbbbbbbbbbbb".to_owned()));
    // Nothing to clean is not an error.
    assert_eq!(
        cache_clean(&cache.path().join("empty")).unwrap(),
        CleanReport::default()
    );
}
