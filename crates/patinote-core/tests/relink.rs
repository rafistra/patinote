//! Updating links on rename and move (PRD F-10, "Ganti nama dan pindah";
//! F-10.6, F-10.7). The resolver runs over in-memory paths; no SQLite.

use std::fs;

use patinote_core::relink::{Moved, RelinkPlan, folder_moves, plan};
use patinote_core::write::{Actor, Expect, Journal, commit, expect_current};
use patinote_core::{FileKind, LinkKind, Resolver, Target};

fn target(path: &str, aliases: &[&str]) -> Target {
    Target {
        path: path.into(),
        kind: if path.ends_with(".md") {
            FileKind::Note
        } else {
            FileKind::Attachment
        },
        aliases: aliases.iter().map(|a| a.to_string()).collect(),
    }
}

/// A vault of paths (with aliases) and note texts.
struct Vault {
    targets: Vec<Target>,
    notes: Vec<(String, Vec<u8>)>,
}

impl Vault {
    fn new(paths: &[(&str, &[&str])], notes: &[(&str, &str)]) -> Self {
        Self {
            targets: paths.iter().map(|(p, a)| target(p, a)).collect(),
            notes: notes
                .iter()
                .map(|(p, t)| (p.to_string(), t.as_bytes().to_vec()))
                .collect(),
        }
    }

    /// Plans `moves` with resolvers before and after the move.
    fn plan(&self, moves: &[Moved]) -> RelinkPlan {
        let before = Resolver::new(self.targets.clone());
        let after = Resolver::new(self.targets.iter().map(|t| {
            let mut t = t.clone();
            if let Some(m) = moves.iter().find(|m| m.from == t.path) {
                t.path = m.to.clone();
            }
            t
        }));
        plan(
            moves,
            &self.notes,
            |src, link| {
                before
                    .resolve(src, link.kind, &link.target)
                    .map(|r| r.path.to_owned())
            },
            |t| {
                after
                    .resolve("", LinkKind::Wiki, t)
                    .is_some_and(|r| !r.is_ambiguous())
            },
        )
    }

    fn original(&self, path: &str) -> &[u8] {
        &self.notes.iter().find(|(p, _)| p == path).unwrap().1
    }
}

fn mv(from: &str, to: &str) -> Moved {
    Moved {
        from: from.into(),
        to: to.into(),
    }
}

fn new_text(plan: &RelinkPlan, path: &str) -> String {
    let f = plan
        .files
        .iter()
        .find(|f| f.path == path)
        .unwrap_or_else(|| panic!("{path} has no change"));
    String::from_utf8(f.content.clone()).unwrap()
}

/// Every byte outside the changed links is the original byte: the new file is
/// the original's gaps between the links, in order, with each link replaced.
fn assert_only_links_changed(vault: &Vault, plan: &RelinkPlan) {
    for f in &plan.files {
        let orig = vault.original(&f.path);
        let mut rebuilt = Vec::new();
        let mut at = 0;
        for c in &f.changes {
            assert_eq!(&orig[c.byte_start..c.byte_end], c.old.as_bytes());
            rebuilt.extend_from_slice(&orig[at..c.byte_start]);
            rebuilt.extend_from_slice(c.new.as_bytes());
            at = c.byte_end;
        }
        rebuilt.extend_from_slice(&orig[at..]);
        assert_eq!(rebuilt, f.content, "{}", f.path);
    }
}

fn changes(plan: &RelinkPlan, path: &str) -> Vec<(usize, String, String)> {
    plan.files
        .iter()
        .find(|f| f.path == path)
        .map(|f| {
            f.changes
                .iter()
                .map(|c| (c.line, c.old.clone(), c.new.clone()))
                .collect()
        })
        .unwrap_or_default()
}

const PATHS: &[(&str, &[&str])] = &[
    ("Proyek/Rapat.md", &["Meeting"]),
    ("Proyek/Patinote.md", &[]),
    ("Harian/2026-11-19.md", &[]),
    ("Arsip/Catatan lama.md", &[]),
    ("Bacaan.md", &[]),
    ("Proyek/Ide.md", &[]),
];

#[test]
fn f10_6_rename_updates_five_links_and_nothing_else() {
    let crlf_bom = "\u{feff}---\r\nrelated: \"[[Rapat]]\"\r\ntags: [rapat]\r\n---\r\n\
                    Lihat [[Proyek/Rapat#^blok-1|blok itu]].\r\n\
                    Dan [[Patinote]] tetap.\r\n";
    let vault = Vault::new(
        PATHS,
        &[
            (
                "Harian/2026-11-19.md",
                "# 19 November\n\n- [[Rapat]] jam 10\n- [[Meeting]] lewat alias\n- [[Tantivy]] belum ada\n",
            ),
            (
                "Proyek/Patinote.md",
                "Ringkasan [[Rapat|rapat kemarin]].\n\n![[Rapat#Keputusan]]\n",
            ),
            ("Arsip/Catatan lama.md", crlf_bom),
            (
                "Bacaan.md",
                "Baca [catatan](Proyek/Rapat.md) ini.\n\n`[[Rapat]]` di kode tidak dihitung.\n",
            ),
            (
                "Proyek/Ide.md",
                "Relatif: [r](Rapat.md#Keputusan) dan [[rapat]].\n",
            ),
            ("Proyek/Rapat.md", "# Rapat\n\nTautan ke [[Patinote]].\n"),
        ],
    );
    let p = vault.plan(&[mv("Proyek/Rapat.md", "Proyek/Rapat IKN.md")]);

    assert_eq!(p.note_count(), 5);
    assert_eq!(p.link_count(), 8);
    assert_only_links_changed(&vault, &p);

    assert_eq!(
        new_text(&p, "Harian/2026-11-19.md"),
        "# 19 November\n\n- [[Rapat IKN]] jam 10\n- [[Meeting]] lewat alias\n- [[Tantivy]] belum ada\n"
    );
    assert_eq!(
        changes(&p, "Proyek/Patinote.md"),
        [
            (
                1,
                "[[Rapat|rapat kemarin]]".into(),
                "[[Rapat IKN|rapat kemarin]]".into()
            ),
            (
                3,
                "![[Rapat#Keputusan]]".into(),
                "![[Rapat IKN#Keputusan]]".into()
            ),
        ]
    );
    assert_eq!(
        new_text(&p, "Arsip/Catatan lama.md"),
        "\u{feff}---\r\nrelated: \"[[Rapat IKN]]\"\r\ntags: [rapat]\r\n---\r\n\
         Lihat [[Proyek/Rapat IKN#^blok-1|blok itu]].\r\n\
         Dan [[Patinote]] tetap.\r\n"
    );
    assert_eq!(
        new_text(&p, "Bacaan.md"),
        "Baca [catatan](Proyek/Rapat%20IKN.md) ini.\n\n`[[Rapat]]` di kode tidak dihitung.\n"
    );
    assert_eq!(
        new_text(&p, "Proyek/Ide.md"),
        "Relatif: [r](Rapat%20IKN.md#Keputusan) dan [[Rapat IKN]].\n"
    );
    // The renamed note's own link to another note needs no change.
    assert!(p.files.iter().all(|f| f.path != "Proyek/Rapat.md"));
}

#[test]
fn f10_6_mockup_counts_three_links_in_two_notes() {
    let vault = Vault::new(
        PATHS,
        &[
            ("Harian/2026-11-19.md", "- [[Rapat]]\n"),
            (
                "Proyek/Patinote.md",
                "[[Rapat|rapat kemarin]]\n![[Rapat#Keputusan]]\n",
            ),
        ],
    );
    let p = vault.plan(&[mv("Proyek/Rapat.md", "Proyek/Rapat IKN.md")]);
    assert_eq!((p.link_count(), p.note_count()), (3, 2));
}

#[test]
fn f10_7_move_updates_path_links_and_keeps_name_links() {
    let vault = Vault::new(
        PATHS,
        &[
            (
                "Harian/2026-11-19.md",
                "[[Rapat]] [[Proyek/Rapat]] [[Proyek/Rapat.md|x]] [[/Proyek/Rapat#Keputusan]]\n\
                 [a](Proyek/Rapat.md) [b](/Proyek/Rapat) [c](<Proyek/Rapat.md>)\n",
            ),
            (
                "Proyek/Patinote.md",
                "[r](Rapat.md) [[proyek/rapat]] [[Rapat]]\n",
            ),
            (
                "Proyek/Rapat.md",
                "[p](Patinote.md) [t](./Patinote.md) [[Patinote]] [h](../Harian/2026-11-19.md)\n",
            ),
        ],
    );
    let p = vault.plan(&[mv("Proyek/Rapat.md", "Arsip/Rapat.md")]);
    assert_only_links_changed(&vault, &p);
    assert_eq!(
        new_text(&p, "Harian/2026-11-19.md"),
        "[[Rapat]] [[Arsip/Rapat]] [[Arsip/Rapat.md|x]] [[/Arsip/Rapat#Keputusan]]\n\
         [a](Arsip/Rapat.md) [b](/Arsip/Rapat) [c](<Arsip/Rapat.md>)\n"
    );
    // Relative links follow; the user's spelling of the unchanged name stays.
    assert_eq!(
        new_text(&p, "Proyek/Patinote.md"),
        "[r](../Arsip/Rapat.md) [[Arsip/rapat]] [[Rapat]]\n"
    );
    // Inside the moved note, relative links are rewritten from its new folder.
    assert_eq!(
        new_text(&p, "Proyek/Rapat.md"),
        "[p](../Proyek/Patinote.md) [t](../Proyek/Patinote.md) [[Patinote]] [h](../Harian/2026-11-19.md)\n"
    );
}

#[test]
fn f10_7_move_that_leaves_a_name_ambiguous_writes_a_path() {
    // `[[Rapat]]` resolves to the shorter `Proyek/Rapat.md`; after the move
    // the other one would be shorter, so the link needs a path.
    let vault = Vault::new(
        &[
            ("Proyek/Rapat.md", &[]),
            ("Arsip/2025/Rapat.md", &[]),
            ("a.md", &[]),
        ],
        &[("a.md", "[[Rapat]] and [[2025/Rapat]]\n")],
    );
    let p = vault.plan(&[mv("Proyek/Rapat.md", "Arsip/2025/Lama/Rapat.md")]);
    assert_eq!(new_text(&p, "a.md"), "[[Lama/Rapat]] and [[2025/Rapat]]\n");
}

#[test]
fn f10_6_rename_to_a_taken_name_writes_the_shortest_unique_path() {
    let vault = Vault::new(
        &[
            ("Proyek/Catatan.md", &[]),
            ("Arsip/2025/Rapat.md", &[]),
            ("a.md", &[]),
        ],
        &[("a.md", "[[Catatan]] [[catatan.md|c]] [[Rapat]]\n")],
    );
    let p = vault.plan(&[mv("Proyek/Catatan.md", "Proyek/Rapat.md")]);
    assert_eq!(
        new_text(&p, "a.md"),
        "[[Proyek/Rapat]] [[Proyek/Rapat.md|c]] [[Rapat]]\n"
    );
}

#[test]
fn f10_7_folder_move_updates_nested_notes() {
    let paths: &[(&str, &[&str])] = &[
        ("Proyek/Rapat.md", &[]),
        ("Proyek/Sub/Desain.md", &[]),
        ("Proyek/Patinote.md", &[]),
        ("Harian/2026-11-19.md", &[]),
    ];
    let vault = Vault::new(
        paths,
        &[
            (
                "Harian/2026-11-19.md",
                "[[Proyek/Sub/Desain]] [[Sub/Desain]] [[Desain]] [d](../Proyek/Sub/Desain.md) [[Rapat]]\n",
            ),
            (
                "Proyek/Sub/Desain.md",
                "[r](../Rapat.md) [h](../../Harian/2026-11-19.md) [[Proyek/Patinote]]\n",
            ),
        ],
    );
    let note_paths: Vec<String> = paths.iter().map(|(p, _)| p.to_string()).collect();
    let moves = folder_moves("Proyek", "Arsip/Proyek", &note_paths);
    assert_eq!(moves.len(), 3);
    let p = vault.plan(&moves);
    assert_only_links_changed(&vault, &p);
    assert_eq!(
        new_text(&p, "Harian/2026-11-19.md"),
        "[[Arsip/Proyek/Sub/Desain]] [[Sub/Desain]] [[Desain]] [d](../Arsip/Proyek/Sub/Desain.md) [[Rapat]]\n"
    );
    // Links between notes that moved together keep working as written.
    assert_eq!(
        new_text(&p, "Proyek/Sub/Desain.md"),
        "[r](../Rapat.md) [h](../../../Harian/2026-11-19.md) [[Arsip/Proyek/Patinote]]\n"
    );
}

#[test]
fn f10_6_markdown_targets_keep_encoding_brackets_and_titles() {
    let vault = Vault::new(
        &[("Proyek/Rapat Mingguan.md", &[]), ("a.md", &[])],
        &[(
            "a.md",
            "[a](Proyek/Rapat%20Mingguan.md \"judul\") [b](<Proyek/Rapat Mingguan.md>) \
             ![c](Proyek/Rapat%20Mingguan#Bagian%20dua)\n",
        )],
    );
    let p = vault.plan(&[mv("Proyek/Rapat Mingguan.md", "Proyek/Rapat Akhir.md")]);
    assert_eq!(
        new_text(&p, "a.md"),
        "[a](Proyek/Rapat%20Akhir.md \"judul\") [b](<Proyek/Rapat Akhir.md>) \
         ![c](Proyek/Rapat%20Akhir#Bagian%20dua)\n"
    );
}

#[test]
fn f10_6_unrelated_and_unresolved_links_are_never_touched() {
    let vault = Vault::new(
        PATHS,
        &[(
            "a.md",
            "[[Patinote]] [[Meeting]] [[Nanti]] [x](https://e.x/Rapat.md) [[#Rapat]] %%[[Rapat]]%%\n",
        )],
    );
    let p = vault.plan(&[mv("Proyek/Rapat.md", "Proyek/Rapat IKN.md")]);
    assert!(p.files.is_empty());
    assert_eq!((p.link_count(), p.note_count()), (0, 0));
}

#[test]
fn f10_6_single_quoted_frontmatter_escapes_quotes() {
    let vault = Vault::new(PATHS, &[("a.md", "---\nup: '[[Rapat]]'\n---\nbody\n")]);
    let p = vault.plan(&[mv("Proyek/Rapat.md", "Proyek/Rapat Rafi's.md")]);
    assert_eq!(
        new_text(&p, "a.md"),
        "---\nup: '[[Rapat Rafi''s]]'\n---\nbody\n"
    );
}

struct Disk {
    _dir: tempfile::TempDir,
    root: std::path::PathBuf,
    journal: Journal,
}

fn disk(notes: &[(String, Vec<u8>)]) -> Disk {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("vault");
    for (p, bytes) in notes {
        let full = root.join(p);
        fs::create_dir_all(full.parent().unwrap()).unwrap();
        fs::write(full, bytes).unwrap();
    }
    let journal = Journal::new(dir.path().join("state/journal"));
    Disk {
        _dir: dir,
        root,
        journal,
    }
}

#[test]
fn f10_6_transaction_commits_the_rename_and_every_link() {
    let vault = Vault::new(
        PATHS,
        &[
            ("Harian/2026-11-19.md", "- [[Rapat]]\n"),
            ("Proyek/Patinote.md", "[[Rapat|rapat kemarin]]\n"),
            ("Proyek/Rapat.md", "# Rapat\n\n[[Rapat#Keputusan]]\n"),
            ("Proyek/Ide.md", "[[Patinote]]\n"),
        ],
    );
    let d = disk(&vault.notes);
    let p = vault.plan(&[mv("Proyek/Rapat.md", "Proyek/Rapat IKN.md")]);
    let tx = p.transaction(Actor::User, |path| expect_current(&d.root, path).unwrap());
    commit(&d.root, &d.journal, &tx).unwrap();

    let read = |p: &str| fs::read_to_string(d.root.join(p)).ok();
    assert_eq!(read("Proyek/Rapat.md"), None);
    assert_eq!(
        read("Proyek/Rapat IKN.md").as_deref(),
        Some("# Rapat\n\n[[Rapat IKN#Keputusan]]\n")
    );
    assert_eq!(
        read("Harian/2026-11-19.md").as_deref(),
        Some("- [[Rapat IKN]]\n")
    );
    assert_eq!(
        read("Proyek/Patinote.md").as_deref(),
        Some("[[Rapat IKN|rapat kemarin]]\n")
    );
    assert_eq!(read("Proyek/Ide.md").as_deref(), Some("[[Patinote]]\n"));
}

#[test]
fn f10_7_transaction_moves_a_note_without_own_changes() {
    let vault = Vault::new(PATHS, &[("Bacaan.md", "[x](Proyek/Rapat.md)\n")]);
    let mut notes = vault.notes.clone();
    notes.push(("Proyek/Rapat.md".into(), b"# Rapat\n".to_vec()));
    let d = disk(&notes);
    let p = vault.plan(&[mv("Proyek/Rapat.md", "Arsip/Rapat.md")]);
    let tx = p.transaction(Actor::User, |path| expect_current(&d.root, path).unwrap());
    commit(&d.root, &d.journal, &tx).unwrap();
    let read = |p: &str| fs::read_to_string(d.root.join(p)).ok();
    assert_eq!(read("Arsip/Rapat.md").as_deref(), Some("# Rapat\n"));
    assert_eq!(read("Bacaan.md").as_deref(), Some("[x](Arsip/Rapat.md)\n"));
}

#[test]
fn f10_7_folder_transaction_moves_the_folder_and_rewrites_links() {
    let paths: &[(&str, &[&str])] = &[
        ("Proyek/Rapat.md", &[]),
        ("Proyek/Sub/Desain.md", &[]),
        ("Harian/2026-11-19.md", &[]),
        ("Proyek/Lampiran/a.png", &[]),
    ];
    let vault = Vault::new(
        paths,
        &[
            ("Harian/2026-11-19.md", "[[Proyek/Rapat]]\n"),
            (
                "Proyek/Sub/Desain.md",
                "[h](../../Harian/2026-11-19.md) ![](../Lampiran/a.png)\n",
            ),
            ("Proyek/Rapat.md", "# Rapat\n"),
        ],
    );
    let mut notes = vault.notes.clone();
    notes.push(("Proyek/Lampiran/a.png".into(), vec![0x89, 0x50]));
    let d = disk(&notes);
    let note_paths: Vec<String> = paths.iter().map(|(p, _)| p.to_string()).collect();
    let moves = folder_moves("Proyek", "Arsip/Proyek", &note_paths);
    let p = vault.plan(&moves);
    let tx = p.folder_transaction(
        Actor::User,
        "Proyek",
        "Arsip/Proyek",
        expect_current(&d.root, "Proyek").unwrap(),
    );
    commit(&d.root, &d.journal, &tx).unwrap();

    let read = |p: &str| fs::read_to_string(d.root.join(p)).ok();
    assert!(!d.root.join("Proyek").exists());
    assert_eq!(
        read("Harian/2026-11-19.md").as_deref(),
        Some("[[Arsip/Proyek/Rapat]]\n")
    );
    assert_eq!(
        read("Arsip/Proyek/Sub/Desain.md").as_deref(),
        Some("[h](../../../Harian/2026-11-19.md) ![](../Lampiran/a.png)\n")
    );
    assert_eq!(
        fs::read(d.root.join("Arsip/Proyek/Lampiran/a.png")).unwrap(),
        [0x89, 0x50]
    );
    assert!(matches!(
        expect_current(&d.root, "Arsip/Proyek/Rapat.md").unwrap(),
        Expect::Hash(_)
    ));
}

#[test]
fn f10_7_markdown_link_found_by_name_becomes_a_path_only_when_needed() {
    let vault = Vault::new(PATHS, &[("Harian/2026-11-19.md", "[x](Rapat.md)\n")]);
    let moved = vault.plan(&[mv("Proyek/Rapat.md", "Arsip/Rapat.md")]);
    assert!(moved.files.is_empty(), "the name still finds the note");
    let renamed = vault.plan(&[mv("Proyek/Rapat.md", "Proyek/Rapat IKN.md")]);
    assert_eq!(
        new_text(&renamed, "Harian/2026-11-19.md"),
        "[x](../Proyek/Rapat%20IKN.md)\n"
    );
}
