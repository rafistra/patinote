//! Vault scanner (PRD F-01, docs/design/0001 section 7).

use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::Path;

use patinote_core::{
    FileKind, NoteContent, ScanError, ScanIssueKind, ScanOptions, load_note, scan,
};

fn write(root: &Path, rel: &str, data: &[u8]) {
    let path = root.join(rel);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, data).unwrap();
}

fn paths(root: &Path, options: ScanOptions) -> Vec<String> {
    scan(root, &options)
        .unwrap()
        .files
        .into_iter()
        .map(|f| f.path)
        .collect()
}

#[test]
fn f01_3_hidden_and_system_folders_are_skipped() {
    let dir = tempfile::tempdir().unwrap();
    let r = dir.path();
    for rel in [
        ".obsidian/app.json",
        ".git/HEAD",
        ".trash/old.md",
        ".stfolder/x",
        ".stversions/a.md",
        ".hidden.md",
        "Proyek/.draft/secret.md",
        ".patinote/config.toml",
        "Proyek/.syncthing.note.md.tmp",
        "Proyek/.note.md.swp",
        "Proyek/note.md~",
        "Proyek/note.md",
        "a.md",
    ] {
        write(r, rel, b"x");
    }
    // The temp dir itself is named `.tmpXXXX`: a hidden vault root still scans.
    assert!(r.file_name().unwrap().to_string_lossy().starts_with('.'));
    assert_eq!(paths(r, ScanOptions::default()), ["Proyek/note.md", "a.md"]);
}

#[test]
fn f01_4_latin1_note_is_listed_and_not_parsed() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "Café.md", b"caf\xe9 au lait");
    let s = scan(dir.path(), &ScanOptions::default()).unwrap();
    assert_eq!(s.files.len(), 1);
    assert_eq!(s.files[0].kind, FileKind::Note);
    assert_eq!(
        load_note(&dir.path().join("Café.md")).unwrap(),
        NoteContent::NotUtf8
    );
}

#[test]
fn f01_5_unreadable_folder_is_reported_and_others_listed() {
    let dir = tempfile::tempdir().unwrap();
    let r = dir.path();
    write(r, "ok.md", b"x");
    write(r, "Locked/inside.md", b"x");
    write(r, "Unreadable file.md", b"x");
    fs::set_permissions(r.join("Locked"), fs::Permissions::from_mode(0o000)).unwrap();
    fs::set_permissions(
        r.join("Unreadable file.md"),
        fs::Permissions::from_mode(0o000),
    )
    .unwrap();

    let s = scan(r, &ScanOptions::default()).unwrap();
    // Restore before asserting so the temp dir can always be removed.
    fs::set_permissions(r.join("Locked"), fs::Permissions::from_mode(0o755)).unwrap();
    let file_result = load_note(&r.join("Unreadable file.md"));
    fs::set_permissions(
        r.join("Unreadable file.md"),
        fs::Permissions::from_mode(0o644),
    )
    .unwrap();

    if nix_is_root() {
        return; // root can read everything; nothing to test.
    }
    let files: Vec<&str> = s.files.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(files, ["Unreadable file.md", "ok.md"]);
    assert_eq!(s.issues.len(), 1);
    assert_eq!(s.issues[0].path, "Locked");
    assert_eq!(s.issues[0].kind, ScanIssueKind::Unreadable);
    // The file is listed by the scan; reading it fails, and the indexer reports it (step 1.3).
    assert_eq!(
        file_result.unwrap_err().kind(),
        std::io::ErrorKind::PermissionDenied
    );
}

fn nix_is_root() -> bool {
    fs::metadata("/proc/self")
        .map(|m| std::os::unix::fs::MetadataExt::uid(&m) == 0)
        .unwrap_or(false)
}

#[test]
fn f01_6_empty_vault_scans_empty() {
    let dir = tempfile::tempdir().unwrap();
    let s = scan(dir.path(), &ScanOptions::default()).unwrap();
    assert!(s.files.is_empty() && s.conflicts.is_empty() && s.issues.is_empty());
}

#[test]
fn f01_7_symlinks_are_not_followed_and_loops_do_not_hang() {
    let outside = tempfile::tempdir().unwrap();
    write(outside.path(), "Outside.md", b"x");
    let dir = tempfile::tempdir().unwrap();
    let r = dir.path();
    write(r, "Notes/a.md", b"x");
    symlink(outside.path(), r.join("Linked")).unwrap();
    symlink(r.join("Notes"), r.join("Notes/loop")).unwrap();
    symlink(r.join("Notes/a.md"), r.join("alias.md")).unwrap();

    // Default: no symlink is followed, so nothing outside the vault appears.
    assert_eq!(paths(r, ScanOptions::default()), ["Notes/a.md"]);

    // When enabled, the loop is reported and the scan finishes.
    let s = scan(
        r,
        &ScanOptions {
            follow_symlinks: true,
        },
    )
    .unwrap();
    let files: Vec<&str> = s.files.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(files, ["Linked/Outside.md", "Notes/a.md", "alias.md"]);
    assert!(
        s.issues
            .iter()
            .any(|i| i.kind == ScanIssueKind::SymlinkLoop)
    );
}

#[test]
fn f01_8_large_note_is_listed_but_not_loaded() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "Big.md", &vec![b'a'; 6 * 1024 * 1024]);
    let s = scan(dir.path(), &ScanOptions::default()).unwrap();
    assert_eq!(s.files[0].size, 6 * 1024 * 1024);
    assert_eq!(
        load_note(&dir.path().join("Big.md")).unwrap(),
        NoteContent::TooLarge {
            size: 6 * 1024 * 1024
        }
    );
}

#[test]
fn kinds_case_only_names_and_nfc() {
    let dir = tempfile::tempdir().unwrap();
    let r = dir.path();
    for rel in [
        "Rapat.md",
        "rapat.md",
        "Board.canvas",
        "img/diagram.PNG",
        "LICENSE",
        "Cafe\u{301}.md",
    ] {
        write(r, rel, b"x");
    }
    let s = scan(r, &ScanOptions::default()).unwrap();
    let got: Vec<(&str, FileKind)> = s.files.iter().map(|f| (f.path.as_str(), f.kind)).collect();
    assert_eq!(
        got,
        [
            ("Board.canvas", FileKind::Canvas),
            ("Café.md", FileKind::Note),
            ("LICENSE", FileKind::Attachment),
            ("Rapat.md", FileKind::Note),
            ("img/diagram.PNG", FileKind::Attachment),
            ("rapat.md", FileKind::Note),
        ]
    );
    assert!(s.files.iter().all(|f| f.mtime_ns > 0));
}

#[test]
fn patinote_ignore_applies_and_gitignore_does_not() {
    let dir = tempfile::tempdir().unwrap();
    let r = dir.path();
    write(r, ".patinote/ignore", b"Arsip/\n*.tmp\n!keep.tmp\n");
    write(r, ".gitignore", b"*.png\nNotes/\n");
    write(r, ".ignore", b"*.md\n");
    for rel in [
        "Arsip/old.md",
        "Notes/a.md",
        "x.tmp",
        "keep.tmp",
        "pic.png",
        "Sub/Arsip/deep.md",
    ] {
        write(r, rel, b"x");
    }
    // Matches are listed as excluded, not dropped (Q-007); .gitignore and .ignore do nothing.
    let s = scan(r, &ScanOptions::default()).unwrap();
    let excluded: Vec<&str> = s
        .files
        .iter()
        .filter(|f| f.excluded)
        .map(|f| f.path.as_str())
        .collect();
    let included: Vec<&str> = s
        .files
        .iter()
        .filter(|f| !f.excluded)
        .map(|f| f.path.as_str())
        .collect();
    assert_eq!(excluded, ["Arsip/old.md", "Sub/Arsip/deep.md", "x.tmp"]);
    assert_eq!(included, ["Notes/a.md", "keep.tmp", "pic.png"]);
}

#[test]
fn sync_conflicts_are_listed_separately() {
    let dir = tempfile::tempdir().unwrap();
    let r = dir.path();
    write(r, "Rapat/Rapat 19 Nov.md", b"x");
    write(
        r,
        "Rapat/Rapat 19 Nov.sync-conflict-20261119-162104-ABCDEFG.md",
        b"y",
    );
    let s = scan(r, &ScanOptions::default()).unwrap();
    assert_eq!(s.files.len(), 1);
    assert_eq!(s.conflicts.len(), 1);
    assert_eq!(s.conflicts[0].original, "Rapat/Rapat 19 Nov.md");
}

#[test]
fn fixture_vault_scans_as_documented() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/vault-mockup");
    let s = scan(&root, &ScanOptions::default()).unwrap();
    let notes = s.files.iter().filter(|f| f.kind == FileKind::Note).count();
    let attachments: Vec<&str> = s
        .files
        .iter()
        .filter(|f| f.kind == FileKind::Attachment)
        .map(|f| f.path.as_str())
        .collect();
    assert_eq!(notes, 12);
    assert_eq!(attachments, ["Proyek/Lampiran/diagram-indeks.png"]);
    assert_eq!(s.conflicts.len(), 2);
    assert!(s.issues.is_empty());
}

#[test]
fn missing_vault_and_file_path_are_errors() {
    let dir = tempfile::tempdir().unwrap();
    assert!(matches!(
        scan(&dir.path().join("nope"), &ScanOptions::default()),
        Err(ScanError::NotFound(_))
    ));
    write(dir.path(), "file.md", b"x");
    assert!(matches!(
        scan(&dir.path().join("file.md"), &ScanOptions::default()),
        Err(ScanError::NotADirectory(_))
    ));
}

#[test]
fn scanning_does_not_change_the_vault() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/vault-mockup");
    let before = snapshot(&root);
    scan(&root, &ScanOptions::default()).unwrap();
    assert_eq!(before, snapshot(&root));
}

fn snapshot(root: &Path) -> Vec<(String, Vec<u8>)> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in fs::read_dir(&d).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                stack.push(p);
            } else {
                out.push((p.to_string_lossy().into_owned(), fs::read(&p).unwrap()));
            }
        }
    }
    out.sort();
    out
}
