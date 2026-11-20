//! The write path: plan/commit, journal, rollback, recovery, trash
//! (PRD "Gerbang 2: dampak arsitektur", point 1; F-10.3 to F-10.5).

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use patinote_core::write::{
    Actor, Conflict, EntryStatus, Expect, Journal, Op, Retention, Transaction, WriteError, commit,
    expect_current, plan, recover,
};

struct Vault {
    _dir: tempfile::TempDir,
    root: PathBuf,
    journal: Journal,
}

fn vault() -> Vault {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("vault");
    fs::create_dir_all(&root).unwrap();
    let journal = Journal::new(dir.path().join("state/journal"));
    Vault {
        _dir: dir,
        root,
        journal,
    }
}

impl Vault {
    fn write(&self, rel: &str, text: &str) {
        let p = self.root.join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, text).unwrap();
    }
    fn read(&self, rel: &str) -> Option<String> {
        fs::read_to_string(self.root.join(rel)).ok()
    }
    fn expect(&self, rel: &str) -> Expect {
        expect_current(&self.root, rel).unwrap()
    }
    fn commit(&self, ops: Vec<Op>) -> Result<patinote_core::write::Committed, WriteError> {
        commit(
            &self.root,
            &self.journal,
            &Transaction {
                actor: Actor::User,
                ops,
            },
        )
    }
    /// Every file in the vault with its bytes, to prove nothing else changed.
    fn snapshot(&self) -> Vec<(String, Vec<u8>)> {
        let mut out = Vec::new();
        let mut stack = vec![self.root.clone()];
        while let Some(d) = stack.pop() {
            for e in fs::read_dir(&d).unwrap() {
                let p = e.unwrap().path();
                let rel = p
                    .strip_prefix(&self.root)
                    .unwrap()
                    .to_string_lossy()
                    .into_owned();
                if p.is_dir() {
                    out.push((format!("{rel}/"), Vec::new()));
                    stack.push(p);
                } else {
                    out.push((rel, fs::read(&p).unwrap()));
                }
            }
        }
        out.sort();
        out
    }
}

fn write_op(path: &str, text: &str, expect: Expect) -> Op {
    Op::Write {
        path: path.into(),
        content: text.as_bytes().to_vec(),
        expect,
    }
}

#[test]
fn create_and_modify_with_journal() {
    let v = vault();
    let c = v
        .commit(vec![write_op("Proyek/a.md", "satu\n", Expect::Absent)])
        .unwrap();
    assert_eq!(v.read("Proyek/a.md").as_deref(), Some("satu\n"));
    assert_eq!(c.changed, ["Proyek/a.md"]);

    let c = v
        .commit(vec![write_op(
            "Proyek/a.md",
            "dua\n",
            v.expect("Proyek/a.md"),
        )])
        .unwrap();
    assert_eq!(v.read("Proyek/a.md").as_deref(), Some("dua\n"));
    let entry = v.journal.entry(&c.id).unwrap();
    assert_eq!(entry.status, EntryStatus::Committed);
    assert_eq!(entry.actor, "user");
    let patinote_core::write::EntryOp::Write {
        before: Some(blob), ..
    } = &entry.ops[0]
    else {
        panic!("{entry:?}")
    };
    assert_eq!(
        v.journal.blob(&c.id, blob).unwrap(),
        b"satu\n",
        "the old content is in the journal"
    );
    // No temporary files are left behind.
    assert_eq!(fs::read_dir(v.root.join("Proyek")).unwrap().count(), 1);
}

#[test]
fn a_changed_file_stops_the_whole_transaction() {
    let v = vault();
    v.write("a.md", "a\n");
    v.write("b.md", "b\n");
    let stale = v.expect("b.md");
    v.write("b.md", "b diubah dari luar\n");
    let before = v.snapshot();
    let err = v
        .commit(vec![
            write_op("a.md", "A\n", v.expect("a.md")),
            write_op("b.md", "B\n", stale),
        ])
        .unwrap_err();
    assert!(
        matches!(&err, WriteError::Conflict { paths } if paths == &["b.md"]),
        "{err:?}"
    );
    assert_eq!(v.snapshot(), before, "nothing was written");
    assert!(v.journal.ids().unwrap().is_empty(), "nothing was journaled");
}

#[test]
fn writes_keep_permissions() {
    let v = vault();
    v.write("rahasia.md", "x\n");
    fs::set_permissions(v.root.join("rahasia.md"), fs::Permissions::from_mode(0o600)).unwrap();
    v.commit(vec![write_op("rahasia.md", "y\n", v.expect("rahasia.md"))])
        .unwrap();
    let mode = fs::metadata(v.root.join("rahasia.md"))
        .unwrap()
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(mode, 0o600);
}

#[test]
fn f10_3_trash_keeps_the_relative_path_and_content() {
    let v = vault();
    v.write("Proyek/Rapat.md", "isi rapat\n");
    let c = v
        .commit(vec![Op::Trash {
            path: "Proyek/Rapat.md".into(),
            expect: v.expect("Proyek/Rapat.md"),
        }])
        .unwrap();
    assert_eq!(v.read("Proyek/Rapat.md"), None);
    assert_eq!(
        v.read(".trash/Proyek/Rapat.md").as_deref(),
        Some("isi rapat\n")
    );
    assert_eq!(c.changed, ["Proyek/Rapat.md", ".trash/Proyek/Rapat.md"]);
}

#[test]
fn f10_4_same_path_deleted_twice_does_not_overwrite() {
    let v = vault();
    for text in ["pertama\n", "kedua\n", "ketiga\n"] {
        v.write("Proyek/Rapat.md", text);
        v.commit(vec![Op::Trash {
            path: "Proyek/Rapat.md".into(),
            expect: v.expect("Proyek/Rapat.md"),
        }])
        .unwrap();
    }
    let mut trashed: Vec<String> = fs::read_dir(v.root.join(".trash/Proyek"))
        .unwrap()
        .map(|e| fs::read_to_string(e.unwrap().path()).unwrap())
        .collect();
    trashed.sort();
    assert_eq!(trashed, ["kedua\n", "ketiga\n", "pertama\n"]);
    let names: Vec<String> = fs::read_dir(v.root.join(".trash/Proyek"))
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    assert!(
        names
            .iter()
            .all(|n| n.starts_with("Rapat") && n.ends_with(".md")),
        "{names:?}"
    );
}

#[test]
fn f10_5_trashing_a_folder_moves_everything() {
    let v = vault();
    for i in 0..10 {
        v.write(&format!("Arsip/2025/n{i}.md"), &format!("catatan {i}\n"));
    }
    v.commit(vec![Op::Trash {
        path: "Arsip/2025".into(),
        expect: v.expect("Arsip/2025"),
    }])
    .unwrap();
    assert!(!v.root.join("Arsip/2025").exists());
    for i in 0..10 {
        assert_eq!(
            v.read(&format!(".trash/Arsip/2025/n{i}.md")),
            Some(format!("catatan {i}\n"))
        );
    }
}

#[test]
fn trashing_a_folder_that_changed_is_a_conflict() {
    let v = vault();
    v.write("Arsip/a.md", "a\n");
    let expect = v.expect("Arsip");
    v.write("Arsip/b.md", "baru dari Syncthing\n");
    let err = v
        .commit(vec![Op::Trash {
            path: "Arsip".into(),
            expect,
        }])
        .unwrap_err();
    assert!(matches!(err, WriteError::Conflict { .. }));
    assert!(v.root.join("Arsip/b.md").exists());
}

#[test]
fn move_rename_and_rewrite() {
    let v = vault();
    v.write("Rapat.md", "[[Rapat]] diri sendiri\n");
    v.write("Lama/x.md", "x\n");
    v.commit(vec![
        Op::Move {
            from: "Rapat.md".into(),
            to: "Proyek/Rapat Mingguan.md".into(),
            content: Some(b"[[Rapat Mingguan]] diri sendiri\n".to_vec()),
            expect: v.expect("Rapat.md"),
        },
        Op::Move {
            from: "Lama".into(),
            to: "Baru".into(),
            content: None,
            expect: v.expect("Lama"),
        },
    ])
    .unwrap();
    assert_eq!(v.read("Rapat.md"), None);
    assert_eq!(
        v.read("Proyek/Rapat Mingguan.md").as_deref(),
        Some("[[Rapat Mingguan]] diri sendiri\n")
    );
    assert_eq!(v.read("Baru/x.md").as_deref(), Some("x\n"));

    v.write("c.md", "c\n");
    let p = plan(
        &v.root,
        &Transaction {
            actor: Actor::User,
            ops: vec![Op::Move {
                from: "c.md".into(),
                to: "Baru/x.md".into(),
                content: None,
                expect: v.expect("c.md"),
            }],
        },
    )
    .unwrap();
    assert_eq!(p.ops[0].conflict, Some(Conflict::DestinationExists));
}

#[test]
fn io_failure_halfway_undoes_what_was_applied() {
    if fs::metadata("/proc/self")
        .map(|m| std::os::unix::fs::MetadataExt::uid(&m) == 0)
        .unwrap_or(false)
    {
        return;
    }
    let v = vault();
    v.write("a.md", "a lama\n");
    v.write("Hapus.md", "dihapus\n");
    v.write("Terkunci/b.md", "b lama\n");
    let ops = vec![
        write_op("a.md", "a baru\n", v.expect("a.md")),
        Op::Trash {
            path: "Hapus.md".into(),
            expect: v.expect("Hapus.md"),
        },
        write_op("Baru/c.md", "c\n", Expect::Absent),
        write_op("Terkunci/b.md", "b baru\n", v.expect("Terkunci/b.md")),
    ];
    let before = v.snapshot();
    fs::set_permissions(v.root.join("Terkunci"), fs::Permissions::from_mode(0o555)).unwrap();
    let err = v.commit(ops).unwrap_err();
    fs::set_permissions(v.root.join("Terkunci"), fs::Permissions::from_mode(0o755)).unwrap();

    assert!(
        matches!(&err, WriteError::RolledBack { path, .. } if path == "Terkunci/b.md"),
        "{err:?}"
    );
    assert_eq!(
        v.snapshot(),
        before,
        "the vault is exactly as before, created folders included"
    );
    let id = v.journal.ids().unwrap().pop().unwrap();
    assert_eq!(
        v.journal.entry(&id).unwrap().status,
        EntryStatus::RolledBack
    );
}

/// Marks an entry pending again, as if the process died before finishing.
fn mark_pending(journal: &Journal, id: &str) {
    let path = journal.dir().join(id).join("manifest.json");
    let text = fs::read_to_string(&path)
        .unwrap()
        .replace("\"committed\"", "\"pending\"");
    fs::write(path, text).unwrap();
}

#[test]
fn recover_undoes_a_transaction_interrupted_by_a_crash() {
    let v = vault();
    v.write("a.md", "a lama\n");
    v.write("Hapus.md", "dihapus\n");
    v.write("Pindah.md", "pindah lama\n");
    let before = v.snapshot();
    let c = v
        .commit(vec![
            write_op("a.md", "a baru\n", v.expect("a.md")),
            Op::Trash {
                path: "Hapus.md".into(),
                expect: v.expect("Hapus.md"),
            },
            Op::Move {
                from: "Pindah.md".into(),
                to: "Folder/Baru/Pindah.md".into(),
                content: Some(b"pindah baru\n".to_vec()),
                expect: v.expect("Pindah.md"),
            },
            write_op("Baru.md", "baru\n", Expect::Absent),
        ])
        .unwrap();

    // Crash after the third change: the fourth never happened, and the move
    // wrote its new file but had not removed the old one yet.
    fs::remove_file(v.root.join("Baru.md")).unwrap();
    v.write("Pindah.md", "pindah lama\n");
    mark_pending(&v.journal, &c.id);

    assert_eq!(
        recover(&v.root, &v.journal).unwrap(),
        std::slice::from_ref(&c.id)
    );
    assert_eq!(v.snapshot(), before);
    assert_eq!(
        v.journal.entry(&c.id).unwrap().status,
        EntryStatus::RolledBack
    );
    // Running it again changes nothing.
    assert!(recover(&v.root, &v.journal).unwrap().is_empty());
    assert_eq!(v.snapshot(), before);
}

#[test]
fn unsafe_paths_are_refused_and_nothing_is_written() {
    let v = vault();
    v.write(".obsidian/app.json", "{}");
    let before = v.snapshot();
    for path in [
        "../luar.md",
        "/etc/passwd",
        ".obsidian/app.json",
        ".git/HEAD",
        ".trash/x.md",
        "a/./b.md",
    ] {
        let err = v
            .commit(vec![write_op(path, "x", Expect::Absent)])
            .unwrap_err();
        assert!(
            matches!(err, WriteError::InvalidPath { .. }),
            "{path}: {err:?}"
        );
    }
    let err = v
        .commit(vec![
            write_op("a.md", "1", Expect::Absent),
            write_op("a.md", "2", Expect::Absent),
        ])
        .unwrap_err();
    assert!(matches!(err, WriteError::InvalidPath { .. }));
    assert_eq!(v.snapshot(), before);
    // Settings may be written to .patinote/ (CLAUDE.md section 5).
    v.commit(vec![write_op(
        ".patinote/config.toml",
        "config_version = 1\n",
        Expect::Absent,
    )])
    .unwrap();
}

#[test]
fn plan_writes_nothing_and_shows_a_diff() {
    let v = vault();
    v.write("a.md", "satu\ndua\ntiga\n");
    v.write("b.md", "b\n");
    let before = v.snapshot();
    let p = plan(
        &v.root,
        &Transaction {
            actor: Actor::Cli,
            ops: vec![
                write_op("a.md", "satu\nDUA\ntiga\n", v.expect("a.md")),
                Op::Trash {
                    path: "b.md".into(),
                    expect: v.expect("b.md"),
                },
            ],
        },
    )
    .unwrap();
    assert_eq!(v.snapshot(), before);
    let diff = p.ops[0].diff.as_deref().unwrap();
    assert!(diff.contains("-dua") && diff.contains("+DUA"), "{diff}");
    assert_eq!(p.ops[1].to.as_deref(), Some(".trash/b.md"));
    assert!(p.conflicts().is_empty());
}

/// Writes a fake committed entry, `days_old` days before `now`, of `bytes` bytes.
fn fake_entry(j: &Journal, n: usize, now: SystemTime, days_old: u64, bytes: usize) {
    let time = now.duration_since(UNIX_EPOCH).unwrap().as_secs() - days_old * 86_400;
    let id = format!("{time:020}-{n:06}");
    let dir = j.dir().join(&id);
    fs::create_dir_all(&dir).unwrap();
    let manifest = format!(
        r#"{{"v":1,"id":"{id}","time":{time},"actor":"user","status":"committed","ops":[],"created_dirs":[]}}"#
    );
    fs::write(dir.join("manifest.json"), manifest).unwrap();
    fs::write(dir.join("before-0"), vec![b'x'; bytes]).unwrap();
}

#[test]
fn journal_retention_follows_the_prd() {
    let now = SystemTime::now();
    let v = vault();
    // 150 old entries and 20 from the last week.
    for n in 0..150 {
        fake_entry(&v.journal, n, now, 30 + (150 - n as u64), 10);
    }
    for n in 150..170 {
        fake_entry(&v.journal, n, now, 1, 10);
    }
    let removed = v.journal.prune(now).unwrap();
    // All 20 recent entries stay, plus enough old ones to keep the last 100.
    assert_eq!(removed.len(), 70);
    assert_eq!(v.journal.ids().unwrap().len(), 100);

    // The size limit wins, oldest first, even for recent entries.
    let rules = Retention {
        max_bytes: 50 * 1000,
        ..Retention::PRD
    };
    let w = vault();
    for n in 0..10 {
        fake_entry(&w.journal, n, now, 0, 10_000);
    }
    w.journal.prune_with(now, &rules).unwrap();
    let left = w.journal.ids().unwrap();
    assert_eq!(left.len(), 4, "{left:?}");
    assert!(left.iter().all(|id| id.ends_with("000006")
        || id.ends_with("000007")
        || id.ends_with("000008")
        || id.ends_with("000009")));
}

#[test]
fn concurrent_commits_do_not_interleave() {
    let v = vault();
    v.write("a.md", "0\n");
    let root = v.root.clone();
    let journal = v.journal.clone();
    let threads: Vec<_> = (0..8)
        .map(|_| {
            let (root, journal) = (root.clone(), journal.clone());
            std::thread::spawn(move || {
                // Each thread retries until its read-modify-write wins.
                loop {
                    let expect = expect_current(&root, "a.md").unwrap();
                    let n: u32 = fs::read_to_string(root.join("a.md"))
                        .unwrap()
                        .trim()
                        .parse()
                        .unwrap();
                    let op = write_op("a.md", &format!("{}\n", n + 1), expect);
                    match commit(
                        &root,
                        &journal,
                        &Transaction {
                            actor: Actor::Cli,
                            ops: vec![op],
                        },
                    ) {
                        Ok(_) => break,
                        Err(WriteError::Conflict { .. }) => {
                            std::thread::sleep(Duration::from_millis(1))
                        }
                        Err(e) => panic!("{e}"),
                    }
                }
            })
        })
        .collect();
    for t in threads {
        t.join().unwrap();
    }
    assert_eq!(
        fs::read_to_string(root.join("a.md")).unwrap(),
        "8\n",
        "no update was lost"
    );
}
