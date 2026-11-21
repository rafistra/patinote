//! Links in the app (F-08): `[[` autocomplete, following links with `gd`,
//! the ambiguous link picker, and links to notes that do not exist yet.

mod common;

use std::time::Duration;

use common::{Env, command, keys, press_with, send, wait};
use iced::keyboard::{Key, Modifiers, key};
use patinote_app::app::{App, Message};
use patinote_core::config::Config;
use patinote_core::ipc::Request;

fn open(env: &Env, path: &str) -> App {
    let mut app = env.start(env.launch(Some(env.vault()), Config::default()));
    send(
        &mut app,
        Message::Remote(Request::Open { path: path.into() }),
    );
    app
}

fn line(app: &App, n: usize) -> String {
    app.active_text()
        .unwrap()
        .lines()
        .nth(n)
        .unwrap_or_default()
        .to_owned()
}

fn title(app: &App) -> String {
    app.title().trim_end_matches(" — Patinote").to_owned()
}

fn tab(app: &mut App) {
    press_with(
        app,
        Key::Named(key::Named::Tab),
        Modifiers::default(),
        None,
        false,
    );
}

#[test]
fn typing_two_brackets_closes_the_link() {
    let env = Env::new();
    let mut app = open(&env, "Inbox/Ide cepat.md");
    keys(&mut app, "ggO[[");
    assert_eq!(line(&app, 0), "[[]]");
    assert!(app.suggestions_open());
}

#[test]
fn f08_2_picking_an_alias_makes_it_the_display_text() {
    let env = Env::new();
    let mut app = open(&env, "Inbox/Ide cepat.md");
    keys(&mut app, "ggO[[second brain");
    tab(&mut app);
    assert_eq!(line(&app, 0), "[[Patinote|Second Brain]]");
    // The cursor is after `]]`: typing goes on outside the link.
    keys(&mut app, " ok<Esc>");
    assert_eq!(line(&app, 0), "[[Patinote|Second Brain]] ok");
}

#[test]
fn f08_3_a_name_shared_by_two_notes_gets_its_folder() {
    let env = Env::new();
    let mut app = open(&env, "Inbox/Ide cepat.md");
    keys(&mut app, "ggO[[Rapat");
    let items = app.suggestion_inserts();
    assert!(items.contains(&"Proyek/Rapat".to_owned()), "{items:?}");
    assert!(items.contains(&"2025/Rapat".to_owned()), "{items:?}");
    assert!(items.contains(&"Rapat 19 Nov".to_owned()), "{items:?}");
}

#[test]
fn f08_4_hash_lists_the_notes_headings_in_order() {
    let env = Env::new();
    let mut app = open(&env, "Inbox/Ide cepat.md");
    keys(&mut app, "ggO[[Desain indeks Patinote#");
    let items = app.suggestion_inserts();
    assert_eq!(
        items.first().map(String::as_str),
        Some("Desain indeks Patinote#Desain indeks Patinote")
    );
    assert!(items.contains(&"Desain indeks Patinote#Tabel inti".to_owned()));
    keys(&mut app, "tab");
    assert_eq!(
        app.suggestion_inserts(),
        ["Desain indeks Patinote#Tabel inti"]
    );
    tab(&mut app);
    assert_eq!(line(&app, 0), "[[Desain indeks Patinote#Tabel inti]]");
}

#[test]
fn f08_5_gd_on_a_heading_link_opens_the_note_at_the_heading() {
    let env = Env::new();
    let mut app = open(&env, "Harian/2026-11-19.md");
    keys(&mut app, "6G0fDgd");
    assert_eq!(title(&app), "Desain indeks Patinote");
    let cursor = app.active_cursor().unwrap();
    assert_eq!(line(&app, cursor.0), "## Tabel inti");
}

#[test]
fn f08_6_a_link_to_a_missing_note_opens_it_empty_and_writes_nothing() {
    let env = Env::new();
    let mut app = open(&env, "Proyek/Desain indeks Patinote.md");
    let n = app
        .active_text()
        .unwrap()
        .lines()
        .position(|l| l.contains("[[tantivy]]"))
        .unwrap();
    keys(&mut app, &format!("{}G0ftgd", n + 1));
    assert_eq!(title(&app), "tantivy");
    assert_eq!(app.active_text().as_deref(), Some(""));
    command(&mut app, "q");
    wait(&mut app, Duration::from_secs(2));
    assert!(!env.path("Vault/tantivy.md").exists());
}

#[test]
fn f08_7_a_link_from_autocomplete_counts_as_a_backlink_after_autosave() {
    let env = Env::new();
    let mut app = open(&env, "Inbox/Ide cepat.md");
    keys(&mut app, "ggO[[Daftar bac");
    tab(&mut app);
    keys(&mut app, "<Esc>");
    wait(&mut app, Duration::from_secs(2));
    send(
        &mut app,
        Message::Remote(Request::Open {
            path: "Referensi/Daftar bacaan.md".into(),
        }),
    );
    assert_eq!(app.active_backlinks(), Some(1));
}

#[test]
fn an_ambiguous_link_opens_a_picker() {
    let env = Env::new();
    let mut app = open(&env, "Harian/2026-11-19.md");
    keys(&mut app, "7G0f[gd");
    assert_eq!(
        app.picker_choices(),
        ["Proyek/Rapat.md", "Arsip/2025/Rapat.md"]
    );
    assert!(
        app.message()
            .is_some_and(|(_, m)| m.starts_with("2 notes are named \"Rapat\""))
    );
    keys(&mut app, "j<CR>");
    assert_eq!(app.title(), "Rapat — Patinote");
    assert_eq!(app.picker_choices(), Vec::<String>::new());
}

#[test]
fn ambiguous_links_are_marked_in_the_note() {
    let env = Env::new();
    let app = open(&env, "Harian/2026-11-19.md");
    assert_eq!(app.ambiguous_targets(), ["rapat"]);
}

#[test]
fn f08_6_a_relative_markdown_link_to_a_missing_note_opens_it_in_that_folder() {
    let env = Env::new();
    let mut app = open(&env, "Inbox/Ide cepat.md");
    keys(&mut app, "ggO[x](../Arsip/Baru.md)<Esc>0lgd");
    assert_eq!(title(&app), "Baru");
    assert!(
        app.buffer_text("Arsip/Baru.md").is_some(),
        "{:?}",
        app.buffer_titles()
    );
    keys(&mut app, "iisi<Esc>");
    command(&mut app, "w");
    assert!(env.path("Vault/Arsip/Baru.md").exists());
}

#[test]
fn f08_suggestions_and_the_picker_close_when_another_note_opens() {
    let env = Env::new();
    let mut app = open(&env, "Inbox/Ide cepat.md");
    keys(&mut app, "ggO[[");
    assert!(app.suggestions_open());
    send(
        &mut app,
        Message::Remote(Request::Open {
            path: "Referensi/Daftar bacaan.md".into(),
        }),
    );
    assert!(
        !app.suggestions_open(),
        "the suggestions belonged to the other note"
    );

    let mut app = open(&env, "Harian/2026-11-19.md");
    keys(&mut app, "7G0f[gd");
    assert!(!app.picker_choices().is_empty());
    send(
        &mut app,
        Message::Remote(Request::Open {
            path: "Referensi/Daftar bacaan.md".into(),
        }),
    );
    assert_eq!(app.picker_choices(), Vec::<String>::new());
}

#[test]
fn f08_6_a_link_to_a_new_note_outside_the_vault_says_so() {
    let env = Env::new();
    let mut app = open(&env, "Inbox/Ide cepat.md");
    keys(&mut app, "ggO[x](../../Luar.md)<Esc>0lgd");
    assert_eq!(title(&app), "Ide cepat");
    assert_eq!(
        app.message().map(|(l, _)| l),
        Some(patinote_app::theme::styles::Level::Error)
    );
}
