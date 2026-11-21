//! Changes from outside (F-04, F-11): the watcher reaches the app, open
//! notes reload or warn, nothing is overwritten, and Syncthing conflicts
//! are listed and resolved.

mod common;

use std::time::{Duration, Instant};

use common::{Env, command, keys, leader, press, send, wait};
use iced_runtime::futures::futures::{StreamExt, executor::block_on};
use patinote_app::app::{App, Message};
use patinote_app::session::{Session, VaultPaths};
use patinote_core::config::Config;
use patinote_core::ipc::Request;
use patinote_core::{Change, WatchMode};

fn open(env: &Env, path: &str) -> App {
    let mut app = env.start(env.launch(Some(env.vault()), Config::default()));
    send(
        &mut app,
        Message::Remote(Request::Open { path: path.into() }),
    );
    app
}

/// What the watcher would report for `paths`.
fn changed(app: &mut App, paths: &[&str]) {
    send(
        app,
        Message::External(Change {
            paths: paths.iter().map(|p| (*p).to_owned()).collect(),
            rescan: false,
            mode: WatchMode::Events,
        }),
    );
}

fn write(env: &Env, rel: &str, text: &str) {
    std::fs::write(env.path("Vault").join(rel), text).unwrap();
}

#[test]
fn f04_1_the_watcher_reports_a_change_within_a_second() {
    let env = Env::new();
    let vault = env.vault();
    let paths = VaultPaths::resolve(&vault, &env.path("cache"), &env.path("state")).unwrap();
    let (_session, events, mut changes) = Session::open_watching(paths, true);
    // The opening index update ends the first stream; the watcher is up.
    let _: Vec<_> = block_on(events.collect());
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        while let Some(c) = block_on(changes.next()) {
            if tx.send(c).is_err() {
                break;
            }
        }
    });
    let started = Instant::now();
    std::fs::write(vault.join("Inbox/Ide cepat.md"), "diubah di Neovim\n").unwrap();
    let change = rx
        .recv_timeout(Duration::from_secs(1))
        .expect("a change within 1 s");
    assert!(
        change.paths.contains(&"Inbox/Ide cepat.md".to_owned()),
        "{change:?}"
    );
    assert!(started.elapsed() < Duration::from_secs(1));
}

#[test]
fn f11_1_an_unchanged_note_reloads_and_keeps_the_cursor_line() {
    let env = Env::new();
    let mut app = open(&env, "Proyek/Desain indeks Patinote.md");
    keys(&mut app, "12G");
    let line = app.active_cursor().unwrap().0;
    let text = app
        .active_text()
        .unwrap()
        .replace("Tabel inti", "Tabel inti (dari Neovim)");
    write(&env, "Proyek/Desain indeks Patinote.md", &text);
    changed(&mut app, &["Proyek/Desain indeks Patinote.md"]);
    assert_eq!(app.active_text().unwrap(), text);
    assert_eq!(app.active_cursor().unwrap().0, line);
    assert_eq!(
        app.message().map(|m| m.1),
        Some("\"Desain indeks Patinote.md\" updated from disk")
    );
}

#[test]
fn f11_2_unsaved_text_and_the_disk_are_both_kept() {
    let env = Env::new();
    let mut app = open(&env, "Inbox/Ide cepat.md");
    keys(&mut app, "ggOketikan saya<Esc>");
    write(&env, "Inbox/Ide cepat.md", "dari luar\n");
    changed(&mut app, &["Inbox/Ide cepat.md"]);
    assert_eq!(
        app.external_title().as_deref(),
        Some("Ide cepat.md changed on disk while you were editing.")
    );
    wait(&mut app, Duration::from_secs(3));
    assert_eq!(
        std::fs::read_to_string(env.path("Vault/Inbox/Ide cepat.md")).unwrap(),
        "dari luar\n"
    );
    assert!(app.active_text().unwrap().starts_with("ketikan saya\n"));
}

#[test]
fn f11_3_a_missed_event_still_stops_the_save() {
    let env = Env::new();
    let mut app = open(&env, "Inbox/Ide cepat.md");
    keys(&mut app, "ggOketikan saya<Esc>");
    // Changed outside, and the watcher never says so.
    write(&env, "Inbox/Ide cepat.md", "dari luar\n");
    command(&mut app, "w");
    assert_eq!(
        std::fs::read_to_string(env.path("Vault/Inbox/Ide cepat.md")).unwrap(),
        "dari luar\n"
    );
    assert!(app.external_title().is_some(), "the banner shows");
}

#[test]
fn f11_4_compare_shows_the_lines_that_differ() {
    let env = Env::new();
    let mut app = open(&env, "Inbox/Ide cepat.md");
    let original = app.active_text().unwrap();
    keys(&mut app, "ggOsaya<Esc>");
    write(&env, "Inbox/Ide cepat.md", &format!("disk\n{original}"));
    changed(&mut app, &["Inbox/Ide cepat.md"]);
    send(
        &mut app,
        Message::Choose(patinote_app::app::Choice::Compare),
    );
    assert_eq!(
        app.compare_summary().as_deref(),
        Some("1 change on disk, 1 in your version")
    );
    // Keep mine: my text replaces the disk version.
    send(
        &mut app,
        Message::Choose(patinote_app::app::Choice::KeepMine),
    );
    assert_eq!(app.external_title(), None);
    assert!(
        std::fs::read_to_string(env.path("Vault/Inbox/Ide cepat.md"))
            .unwrap()
            .starts_with("saya\n")
    );
}

#[test]
fn a_note_deleted_outside_can_be_saved_again() {
    let env = Env::new();
    let mut app = open(&env, "Inbox/Ide cepat.md");
    let text = app.active_text().unwrap();
    std::fs::remove_file(env.path("Vault/Inbox/Ide cepat.md")).unwrap();
    changed(&mut app, &["Inbox/Ide cepat.md"]);
    assert_eq!(
        app.external_title().as_deref(),
        Some("Ide cepat.md was deleted outside Patinote.")
    );
    send(
        &mut app,
        Message::Choose(patinote_app::app::Choice::SaveAgain),
    );
    assert_eq!(
        std::fs::read_to_string(env.path("Vault/Inbox/Ide cepat.md")).unwrap(),
        text
    );
}

#[test]
fn f04_6_our_own_save_is_not_reloaded() {
    let env = Env::new();
    let mut app = open(&env, "Inbox/Ide cepat.md");
    keys(&mut app, "ggOsimpan<Esc>");
    command(&mut app, "w");
    send(&mut app, Message::Done);
    // The watcher sees our own write; it is recognised by its hash.
    changed(&mut app, &["Inbox/Ide cepat.md"]);
    assert_eq!(app.message(), None, "no 'updated from disk'");
    assert_eq!(app.external_title(), None);
    assert_eq!(app.active_dirty(), Some(false));
}

#[test]
fn f11_5_conflict_copies_are_listed_but_not_in_the_switcher() {
    let env = Env::new();
    let mut app = env.start(env.launch(Some(env.vault()), Config::default()));
    assert_eq!(app.conflict_count(), 2);
    leader(&mut app, "f");
    send(&mut app, Message::SwitcherInput("sync-conflict".into()));
    assert!(
        app.switcher_titles()
            .iter()
            .all(|t| !t.contains("sync-conflict"))
    );
    press(&mut app, "Escape");
    leader(&mut app, "c");
    assert!(app.conflict_list_open());
}

#[test]
fn f11_6_use_copy_replaces_the_original_and_trashes_the_copy() {
    let env = Env::new();
    let mut app = env.start(env.launch(Some(env.vault()), Config::default()));
    let vault = env.path("Vault");
    let copy_rel = "Rapat/Rapat 19 Nov.sync-conflict-20261119-162104-ABCDEFG.md";
    let copy_text = std::fs::read_to_string(vault.join(copy_rel)).unwrap();
    command(&mut app, "conflicts");
    let i = app.conflict_index("Rapat/Rapat 19 Nov.md").unwrap();
    send(&mut app, Message::OpenConflict(i));
    assert!(app.compare_summary().is_some(), "the comparison is open");
    send(
        &mut app,
        Message::Choose(patinote_app::app::Choice::UseCopy),
    );
    assert_eq!(
        std::fs::read_to_string(vault.join("Rapat/Rapat 19 Nov.md")).unwrap(),
        copy_text
    );
    assert!(!vault.join(copy_rel).exists());
    assert_eq!(
        std::fs::read_to_string(vault.join(format!(".trash/{copy_rel}"))).unwrap(),
        copy_text
    );
    assert_eq!(app.conflict_count(), 1);
}

#[test]
fn f04_2_a_note_made_outside_is_in_the_switcher_after_the_change() {
    let env = Env::new();
    let mut app = env.start(env.launch(Some(env.vault()), Config::default()));
    write(&env, "baru.md", "dari terminal\n");
    changed(&mut app, &["baru.md"]);
    leader(&mut app, "f");
    send(&mut app, Message::SwitcherInput("baru".into()));
    assert_eq!(
        app.switcher_titles().first().map(String::as_str),
        Some("baru")
    );
}
