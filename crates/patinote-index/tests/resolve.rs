//! Link resolution in the SQLite index (step 1.4, docs/design/0001 section 4).

use std::fs;
use std::path::Path;

use patinote_core::store::Store;
use patinote_core::{IndexOptions, Resolver, index_vault};
use patinote_index::{OpenOptions, SqliteStore};

fn write(root: &Path, rel: &str, text: &str) {
    let p = root.join(rel);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(p, text).unwrap();
}

fn index(root: &Path, s: &mut SqliteStore) -> patinote_core::IndexReport {
    index_vault(root, s, &IndexOptions::default(), &mut |_| {}).unwrap()
}

/// Destination of the first link from `source` with `target`.
fn dest(s: &SqliteStore, source: &str, target: &str) -> Option<String> {
    s.connection()
        .query_row(
            "SELECT d.path FROM links l JOIN notes src ON src.id = l.src_id LEFT JOIN notes d ON d.id = l.dst_id
             WHERE src.path = ?1 AND l.target = ?2 LIMIT 1",
            [source, target],
            |r| r.get(0),
        )
        .unwrap()
}

fn count(s: &SqliteStore, sql: &str) -> i64 {
    s.connection().query_row(sql, [], |r| r.get(0)).unwrap()
}

#[test]
fn fixture_vault_links_resolve_like_obsidian() {
    let db = tempfile::tempdir().unwrap();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/vault-mockup");
    let mut s = SqliteStore::open(&db.path().join("index.db"), &OpenOptions::default()).unwrap();
    index(&root, &mut s);

    let daily = "Harian/2026-11-19.md";
    // Two notes are named "Rapat": the shorter path wins (mockup 12).
    assert_eq!(dest(&s, daily, "Rapat").as_deref(), Some("Proyek/Rapat.md"));
    assert_eq!(
        dest(&s, daily, "Desain indeks Patinote").as_deref(),
        Some("Proyek/Desain indeks Patinote.md")
    );
    let main = "Proyek/Desain indeks Patinote.md";
    assert_eq!(
        dest(&s, main, "Patinote").as_deref(),
        Some("Proyek/Patinote.md")
    );
    assert_eq!(
        dest(&s, main, "diagram-indeks.png").as_deref(),
        Some("Proyek/Lampiran/diagram-indeks.png")
    );
    // Notes that do not exist (fixtures/README.md).
    assert_eq!(dest(&s, main, "tantivy"), None);
    assert_eq!(dest(&s, main, "Ide Baru"), None);
    // The frontmatter link `project: "[[Patinote]]"`.
    assert_eq!(
        count(
            &s,
            "SELECT count(*) FROM links WHERE kind = 'frontmatter' AND dst_id IS NOT NULL"
        ),
        count(&s, "SELECT count(*) FROM links WHERE kind = 'frontmatter'")
    );
}

#[test]
fn destinations_follow_added_and_removed_notes() {
    let dir = tempfile::tempdir().unwrap();
    let v = dir.path().join("v");
    write(&v, "a.md", "[[Rapat]] dan [[Baru]]\n");
    write(&v, "Arsip/2025/Rapat.md", "lama\n");
    let mut s = SqliteStore::open(&dir.path().join("index.db"), &OpenOptions::default()).unwrap();
    index(&v, &mut s);
    assert_eq!(
        dest(&s, "a.md", "Rapat").as_deref(),
        Some("Arsip/2025/Rapat.md")
    );
    assert_eq!(dest(&s, "a.md", "Baru"), None);

    // A shorter path takes over; a note that did not exist now does.
    write(&v, "Rapat.md", "baru\n");
    write(&v, "Ide/Baru.md", "ada\n");
    let r = index(&v, &mut s);
    assert_eq!(r.links_updated, 2);
    assert_eq!(dest(&s, "a.md", "Rapat").as_deref(), Some("Rapat.md"));
    assert_eq!(dest(&s, "a.md", "Baru").as_deref(), Some("Ide/Baru.md"));

    // Removing it falls back to the other candidate, then to nothing.
    fs::remove_file(v.join("Rapat.md")).unwrap();
    index(&v, &mut s);
    assert_eq!(
        dest(&s, "a.md", "Rapat").as_deref(),
        Some("Arsip/2025/Rapat.md")
    );
    fs::remove_dir_all(v.join("Arsip")).unwrap();
    index(&v, &mut s);
    assert_eq!(dest(&s, "a.md", "Rapat"), None);

    // Nothing changed: resolution does not run and nothing moves.
    let r = index(&v, &mut s);
    assert_eq!((r.reprocessed(), r.links_updated), (0, 0));
}

#[test]
fn unresolved_and_ambiguous_counts_match_vaultgen() {
    let dir = tempfile::tempdir().unwrap();
    let v = dir.path().join("v");
    let summary = vaultgen::generate(
        &vaultgen::Config {
            notes: 1000,
            seed: 9,
            long_note_lines: 0,
        },
        &v,
    )
    .unwrap();
    let mut s = SqliteStore::open(&dir.path().join("index.db"), &OpenOptions::default()).unwrap();
    index(&v, &mut s);

    let unresolved = count(&s, "SELECT count(*) FROM links WHERE dst_id IS NULL");
    assert_eq!(unresolved as usize, summary.links_unresolved);

    let resolver = Resolver::new(s.targets().unwrap());
    let ambiguous = s
        .links()
        .unwrap()
        .iter()
        .filter(|l| {
            resolver
                .resolve(&l.source, l.kind, &l.target)
                .is_some_and(|r| r.is_ambiguous())
        })
        .count();
    assert_eq!(ambiguous, summary.links_ambiguous);
}

#[test]
fn excluded_notes_are_targets_only() {
    let dir = tempfile::tempdir().unwrap();
    let v = dir.path().join("v");
    write(&v, ".patinote/ignore", "Arsip/\n");
    write(&v, "a.md", "[[Skema lama]] dan [[Nama Lama]]\n");
    write(
        &v,
        "Arsip/Skema lama.md",
        "---\naliases: [Nama Lama]\n---\n# Skema lama\nIsi [[a]] #arsip\n",
    );
    let mut s = SqliteStore::open(&dir.path().join("index.db"), &OpenOptions::default()).unwrap();
    let r = index(&v, &mut s);
    assert_eq!(r.excluded, 1);
    assert_eq!(
        dest(&s, "a.md", "Skema lama").as_deref(),
        Some("Arsip/Skema lama.md")
    );
    assert_eq!(
        dest(&s, "a.md", "Nama Lama").as_deref(),
        Some("Arsip/Skema lama.md")
    );
    let q = |sql: &str| count(&s, sql);
    assert_eq!(
        q("SELECT excluded FROM notes WHERE path = 'Arsip/Skema lama.md'"),
        1
    );
    assert_eq!(
        q("SELECT count(*) FROM notes_fts WHERE notes_fts MATCH 'isi'"),
        0
    );
    assert_eq!(
        q("SELECT count(*) FROM links l JOIN notes n ON n.id = l.src_id WHERE n.excluded = 1"),
        0
    );
    assert_eq!(q("SELECT count(*) FROM tags"), 0);

    // Removing the rule brings the note back in full on the next run.
    fs::remove_file(v.join(".patinote/ignore")).unwrap();
    let r = index(&v, &mut s);
    assert_eq!((r.excluded, r.updated), (0, 1));
    assert_eq!(
        count(
            &s,
            "SELECT count(*) FROM notes_fts WHERE notes_fts MATCH 'isi'"
        ),
        1
    );
    assert_eq!(count(&s, "SELECT count(*) FROM tags"), 1);
}

/// `link_candidates` loads only the files a target can match; resolving from
/// them must give exactly what resolving from every file gives (A-01.6).
#[test]
fn link_candidates_resolve_like_the_full_resolver() {
    let dir = tempfile::tempdir().unwrap();
    let v = dir.path().join("v");
    vaultgen::generate(
        &vaultgen::Config {
            notes: 1000,
            seed: 9,
            long_note_lines: 0,
        },
        &v,
    )
    .unwrap();
    // Names that stress the keys: dots, case, NFD, attachments, aliases, folders.
    write(&v, "Dua/Versi.v2.md", "x");
    write(&v, "Tiga/Versi.v2.md", "x");
    write(&v, "Lampiran/Foto.Liburan.PNG", "x");
    write(&v, "Lampiran/Foto.md", "x");
    write(
        &v,
        "Kafe.md",
        "---\naliases: [Tempat Ngopi, Café Lama]\n---\n",
    );
    write(&v, "Cafe\u{301}.md", "x");
    let mut s = SqliteStore::open(&dir.path().join("index.db"), &OpenOptions::default()).unwrap();
    index(&v, &mut s);

    let full = Resolver::new(s.targets().unwrap());
    let mut inputs: Vec<String> = s
        .connection()
        .prepare("SELECT DISTINCT target FROM links")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    inputs.extend(s.all_notes().unwrap().into_iter().flat_map(|n| {
        let no_ext = n.path.strip_suffix(".md").unwrap_or(&n.path).to_owned();
        let name = no_ext.rsplit('/').next().unwrap().to_owned();
        [n.path.to_uppercase(), no_ext, name]
    }));
    inputs.extend(
        [
            "versi.v2",
            "Tiga/Versi.v2",
            "foto.liburan.png",
            "Foto",
            "tempat ngopi",
            "CAFÉ LAMA",
            "Café",
            "/Kafe",
            "Kafe.md",
            "tidak ada",
        ]
        .map(str::to_owned),
    );
    assert!(inputs.len() > 3000);
    for t in &inputs {
        let narrow = Resolver::new(s.link_candidates(t).unwrap());
        assert_eq!(
            narrow.resolve("", patinote_core::model::LinkKind::Wiki, t),
            full.resolve("", patinote_core::model::LinkKind::Wiki, t),
            "{t}"
        );
    }
}

#[test]
fn a01_orphans_ignore_links_from_a_note_to_itself() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("v");
    write(
        &root,
        "Sendiri.md",
        "# Ringkasan\n\nLihat [[#Ringkasan]] dan [[Sendiri]].\n",
    );
    write(&root, "Keluar.md", "Ke [[Belum ada]].\n");
    write(&root, "Sepi.md", "Tanpa link.\n");
    let mut s = SqliteStore::open(&dir.path().join("index.db"), &OpenOptions::default()).unwrap();
    index(&root, &mut s);
    let orphans: Vec<String> = s.orphans().unwrap().into_iter().map(|n| n.path).collect();
    // A link to a note that does not exist yet still points out.
    assert_eq!(orphans, ["Sendiri.md", "Sepi.md"]);
}
