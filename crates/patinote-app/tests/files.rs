//! Creating, deleting, renaming and moving notes and folders (F-10), from
//! the keyboard: `<leader>n`, and `a`, `A`, `r`, `m`, `d` in the explorer.

mod common;

use std::path::Path;
use std::time::Duration;

use common::{Env, command, keys, leader, press, send, wait};
use patinote_app::app::{App, Message};
use patinote_core::config::Config;
use patinote_core::ipc::Request;

fn start(env: &Env) -> App {
    env.start(env.launch(Some(env.vault()), Config::default()))
}

fn read(env: &Env, rel: &str) -> String {
    std::fs::read_to_string(env.path("Vault").join(rel)).unwrap()
}

/// Opens the explorer, selects `path` and presses `key`.
fn explorer_key(app: &mut App, path: &str, key: &str) {
    if !app.explorer_is_open() {
        leader(app, "e");
    } else {
        // Open from the last session: give it the focus.
        common::press_with(
            app,
            iced::keyboard::Key::Character("w".into()),
            iced::keyboard::Modifiers::CTRL,
            None,
            false,
        );
        press(app, "h");
    }
    app.select_in_explorer(path);
    press(app, key);
}

fn trash_files(vault: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack = vec![vault.join(".trash")];
    while let Some(d) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&d) else {
            continue;
        };
        for e in entries {
            let p = e.unwrap().path();
            if p.is_dir() {
                stack.push(p);
            } else {
                out.push(
                    p.strip_prefix(vault)
                        .unwrap()
                        .to_string_lossy()
                        .into_owned(),
                );
            }
        }
    }
    out.sort();
    out
}

#[test]
fn f10_1_leader_n_creates_the_note_and_the_switcher_finds_it() {
    let env = Env::new();
    let mut app = start(&env);
    leader(&mut app, "n");
    assert!(app.dialog_open());
    send(&mut app, Message::DialogInput("Catatan rapat".into()));
    send(&mut app, Message::DialogSubmit);
    assert!(!app.dialog_open());
    keys(&mut app, "Isi<Esc>");
    wait(&mut app, Duration::from_millis(1100));
    assert_eq!(read(&env, "Catatan rapat.md"), "Isi");
    leader(&mut app, "f");
    send(&mut app, Message::SwitcherInput("catatan rapat".into()));
    assert_eq!(
        app.switcher_titles().first().map(String::as_str),
        Some("Catatan rapat")
    );
}

#[test]
fn f10_1_a_in_the_explorer_creates_the_note_in_that_folder() {
    let env = Env::new();
    let mut app = start(&env);
    explorer_key(&mut app, "Proyek", "a");
    send(&mut app, Message::InlineInput("Baru".into()));
    send(&mut app, Message::InlineSubmit);
    keys(&mut app, "Isi<Esc>");
    wait(&mut app, Duration::from_millis(1100));
    assert_eq!(read(&env, "Proyek/Baru.md"), "Isi");
}

#[test]
fn f10_2_names_with_forbidden_characters_are_refused() {
    let env = Env::new();
    let mut app = start(&env);
    leader(&mut app, "n");
    send(&mut app, Message::DialogInput("Rapat: evaluasi".into()));
    send(&mut app, Message::DialogSubmit);
    assert!(
        app.dialog_open(),
        "the dialog stays, with the reason under the input"
    );
    assert!(app.active_text().is_none(), "no note was opened");
    // A risky name needs a second Enter.
    send(&mut app, Message::DialogInput("Ide #rapat".into()));
    send(&mut app, Message::DialogSubmit);
    assert!(app.dialog_open());
    send(&mut app, Message::DialogSubmit);
    assert!(!app.dialog_open());
    assert_eq!(app.title(), "Ide #rapat — Patinote");
}

#[test]
fn f10_3_delete_moves_the_note_to_trash_with_the_same_path() {
    let env = Env::new();
    let mut app = start(&env);
    let before = read(&env, "Referensi/Daftar bacaan.md");
    explorer_key(&mut app, "Referensi/Daftar bacaan.md", "d");
    assert!(app.dialog_open());
    press(&mut app, "Enter");
    assert!(!env.path("Vault/Referensi/Daftar bacaan.md").exists());
    assert_eq!(read(&env, ".trash/Referensi/Daftar bacaan.md"), before);
}

#[test]
fn f10_4_deleting_the_same_path_twice_keeps_both_in_trash() {
    let env = Env::new();
    let mut app = start(&env);
    explorer_key(&mut app, "Inbox/Ide cepat.md", "d");
    press(&mut app, "Enter");
    std::fs::write(env.path("Vault/Inbox/Ide cepat.md"), "kedua").unwrap();
    // The note came back from outside; the next start indexes it (the
    // watcher of step 2.7 does it live).
    let mut app = env.start(env.launch(None, Config::default()));
    explorer_key(&mut app, "Inbox/Ide cepat.md", "d");
    press(&mut app, "Enter");
    let trash = trash_files(&env.path("Vault"));
    assert_eq!(trash.len(), 2, "{trash:?}");
    assert!(trash.contains(&".trash/Inbox/Ide cepat.md".to_owned()));
}

#[test]
fn f10_5_deleting_a_folder_moves_all_ten_notes_to_trash() {
    let env = Env::new();
    let vault = env.vault();
    std::fs::create_dir(vault.join("Sepuluh")).unwrap();
    for i in 0..10 {
        std::fs::write(vault.join(format!("Sepuluh/n{i}.md")), format!("# {i}\n")).unwrap();
    }
    let mut app = env.start(env.launch(Some(vault.clone()), Config::default()));
    explorer_key(&mut app, "Sepuluh", "d");
    assert!(app.dialog_open());
    press(&mut app, "Enter");
    assert!(!vault.join("Sepuluh").exists());
    let trash = trash_files(&vault);
    assert_eq!(trash.len(), 10, "{trash:?}");
    assert!(trash.iter().all(|t| t.starts_with(".trash/Sepuluh/")));
}

/// A vault with a note linked from five notes in different ways.
fn linked_vault(env: &Env) -> std::path::PathBuf {
    let vault = env.vault();
    std::fs::create_dir_all(vault.join("Tujuan")).unwrap();
    std::fs::write(vault.join("Tujuan/Sasaran.md"), "# Sasaran\n\n## Bagian\n").unwrap();
    let sources = [
        ("s1.md", "Lihat [[Sasaran]] ya.\n"),
        ("s2.md", "Alias [[Sasaran|teks tampilan]] tetap.\n"),
        ("s3.md", "Heading [[Sasaran#Bagian]] juga.\n"),
        ("s4.md", "---\nproject: \"[[Sasaran]]\"\n---\nIsi lain.\n"),
        (
            "s5.md",
            "Path [[Tujuan/Sasaran]] dan md [x](Tujuan/Sasaran.md).\n",
        ),
    ];
    for (name, text) in sources {
        std::fs::write(vault.join(name), text).unwrap();
    }
    vault
}

#[test]
fn f10_6_renaming_updates_every_link_and_nothing_else() {
    let env = Env::new();
    let vault = linked_vault(&env);
    let mut app = env.start(env.launch(Some(vault.clone()), Config::default()));
    explorer_key(&mut app, "Tujuan/Sasaran.md", "r");
    assert!(app.dialog_open());
    send(&mut app, Message::DialogInput("Target Baru".into()));
    send(&mut app, Message::DialogSubmit);
    assert!(vault.join("Tujuan/Target Baru.md").exists());
    assert!(!vault.join("Tujuan/Sasaran.md").exists());
    assert_eq!(read(&env, "s1.md"), "Lihat [[Target Baru]] ya.\n");
    assert_eq!(
        read(&env, "s2.md"),
        "Alias [[Target Baru|teks tampilan]] tetap.\n"
    );
    assert_eq!(
        read(&env, "s3.md"),
        "Heading [[Target Baru#Bagian]] juga.\n"
    );
    assert_eq!(
        read(&env, "s4.md"),
        "---\nproject: \"[[Target Baru]]\"\n---\nIsi lain.\n"
    );
    assert_eq!(
        read(&env, "s5.md"),
        "Path [[Tujuan/Target Baru]] dan md [x](Tujuan/Target%20Baru.md).\n"
    );
}

#[test]
fn f10_7_moving_updates_path_links_and_keeps_name_links() {
    let env = Env::new();
    let vault = linked_vault(&env);
    let mut app = env.start(env.launch(Some(vault.clone()), Config::default()));
    explorer_key(&mut app, "Tujuan/Sasaran.md", "m");
    assert!(app.dialog_open());
    send(&mut app, Message::DialogInput("arsip".into()));
    send(&mut app, Message::DialogSubmit);
    assert!(vault.join("Arsip/Sasaran.md").exists());
    assert_eq!(
        read(&env, "s1.md"),
        "Lihat [[Sasaran]] ya.\n",
        "a name link stays"
    );
    assert_eq!(
        read(&env, "s5.md"),
        "Path [[Arsip/Sasaran]] dan md [x](Arsip/Sasaran.md).\n"
    );
}

#[test]
fn f10_8_after_a_rename_backlinks_come_from_the_same_notes() {
    let env = Env::new();
    let vault = linked_vault(&env);
    let mut app = env.start(env.launch(Some(vault), Config::default()));
    send(
        &mut app,
        Message::Remote(patinote_core::ipc::Request::Open {
            path: "Tujuan/Sasaran.md".into(),
        }),
    );
    let mut before: Vec<String> = app.backlink_lines().into_iter().map(|(p, _)| p).collect();
    before.sort();
    before.dedup();
    explorer_key(&mut app, "Tujuan/Sasaran.md", "r");
    send(&mut app, Message::DialogInput("Target Baru".into()));
    send(&mut app, Message::DialogSubmit);
    assert_eq!(
        app.title(),
        "Target Baru — Patinote",
        "the open buffer follows the rename"
    );
    let mut after: Vec<String> = app.backlink_lines().into_iter().map(|(p, _)| p).collect();
    after.sort();
    after.dedup();
    assert_eq!(after, before);
}

#[test]
fn nothing_is_deleted_when_no_row_is_selected() {
    let env = Env::new();
    let mut app = start(&env);
    leader(&mut app, "e");
    app.select_in_explorer("No/such/file.md");
    for key in ["d", "r", "m"] {
        press(&mut app, key);
        assert!(
            !app.dialog_open(),
            "{key} with nothing selected does nothing"
        );
    }
}

#[test]
fn f10_7_renaming_a_folder_keeps_links_to_its_attachments() {
    let env = Env::new();
    let vault = env.vault();
    std::fs::create_dir_all(vault.join("Tujuan/img")).unwrap();
    std::fs::write(vault.join("Tujuan/img/foto.png"), b"\x89PNG\r\n").unwrap();
    std::fs::write(
        vault.join("Tujuan/Catatan.md"),
        "![a](img/foto.png) ![[img/foto.png]]\n",
    )
    .unwrap();
    std::fs::write(
        vault.join("luar.md"),
        "![b](Tujuan/img/foto.png) ![[Tujuan/img/foto.png]]\n",
    )
    .unwrap();
    let mut app = env.start(env.launch(Some(vault.clone()), Config::default()));
    explorer_key(&mut app, "Tujuan", "r");
    send(&mut app, Message::InlineInput("Sasaran".into()));
    send(&mut app, Message::InlineSubmit);
    assert!(vault.join("Sasaran/img/foto.png").exists());
    assert_eq!(
        read(&env, "Sasaran/Catatan.md"),
        "![a](img/foto.png) ![[img/foto.png]]\n",
        "links inside the folder moved with it"
    );
    assert_eq!(
        read(&env, "luar.md"),
        "![b](Sasaran/img/foto.png) ![[Sasaran/img/foto.png]]\n"
    );
}

#[test]
fn f10_1_creating_a_note_that_exists_opens_it_instead() {
    let env = Env::new();
    let vault = env.vault();
    // Excluded notes are not among the switcher's candidates either.
    std::fs::create_dir_all(vault.join(".patinote")).unwrap();
    std::fs::write(vault.join(".patinote/ignore"), "Proyek/Rahasia.md\n").unwrap();
    std::fs::write(vault.join("Proyek/Rahasia.md"), "isi lama\n").unwrap();
    let before = read(&env, "Proyek/Rapat.md");
    let mut app = start(&env);
    explorer_key(&mut app, "Proyek", "a");
    send(&mut app, Message::InlineInput("Rapat".into()));
    send(&mut app, Message::InlineSubmit);
    assert_eq!(app.active_text().as_deref(), Some(before.as_str()));

    command(&mut app, "q");
    leader(&mut app, "n");
    send(&mut app, Message::DialogInput("Rahasia".into()));
    send(&mut app, Message::DialogSubmit);
    assert_eq!(app.active_text().as_deref(), Some("isi lama\n"));
    keys(&mut app, "ggOtambah<Esc>");
    command(&mut app, "w");
    assert_eq!(read(&env, "Proyek/Rahasia.md"), "tambah\nisi lama\n");
}

#[test]
fn f10_5_deleting_a_note_left_of_the_active_one_keeps_the_active_one() {
    let env = Env::new();
    let mut app = start(&env);
    for p in [
        "Inbox/Ide cepat.md",
        "Proyek/Rapat.md",
        "Referensi/Daftar bacaan.md",
    ] {
        send(
            &mut app,
            Message::Remote(patinote_core::ipc::Request::Open { path: p.into() }),
        );
    }
    command(&mut app, "b Rapat");
    assert_eq!(app.title(), "Rapat — Patinote");
    explorer_key(&mut app, "Inbox/Ide cepat.md", "d");
    press(&mut app, "Enter");
    assert!(!env.path("Vault/Inbox/Ide cepat.md").exists());
    assert_eq!(app.title(), "Rapat — Patinote");
}

#[test]
fn f10_2_names_that_start_with_a_dot_are_refused() {
    let env = Env::new();
    let mut app = start(&env);
    leader(&mut app, "n");
    send(&mut app, Message::DialogInput(".draft".into()));
    send(&mut app, Message::DialogSubmit);
    assert!(app.dialog_open(), "hidden notes are never written");
    assert!(app.active_text().is_none());
    // In the explorer too.
    send(&mut app, Message::DialogInput(String::new()));
    keys(&mut app, "<Esc>");
    explorer_key(&mut app, "Proyek", "a");
    send(&mut app, Message::InlineInput(".rahasia".into()));
    send(&mut app, Message::InlineSubmit);
    assert!(app.active_text().is_none());
}

#[test]
fn f09_5_jumps_and_recent_notes_follow_a_moved_folder() {
    let env = Env::new();
    let mut app = start(&env);
    send(
        &mut app,
        Message::Remote(Request::Open {
            path: "Proyek/Patinote.md".into(),
        }),
    );
    send(
        &mut app,
        Message::Remote(Request::Open {
            path: "Inbox/Ide cepat.md".into(),
        }),
    );
    explorer_key(&mut app, "Proyek", "r");
    send(&mut app, Message::InlineInput("Lama".into()));
    send(&mut app, Message::InlineSubmit);
    assert!(
        env.path("Vault/Lama/Patinote.md").exists(),
        "{:?}",
        app.message()
    );
    // Back to the editor; Ctrl+O goes back to the note at its new path.
    common::press_with(
        &mut app,
        iced::keyboard::Key::Character("w".into()),
        iced::keyboard::Modifiers::CTRL,
        None,
        false,
    );
    press(&mut app, "l");
    common::press_with(
        &mut app,
        iced::keyboard::Key::Character("o".into()),
        iced::keyboard::Modifiers::CTRL,
        None,
        false,
    );
    assert_eq!(app.title(), "Patinote — Patinote");
    assert!(
        !app.buffer_titles().iter().any(|t| t.is_empty()),
        "{:?}",
        app.buffer_titles()
    );
    assert_eq!(app.message(), None, "{:?}", app.message());
}
