//! The side panel, navigation history and buffers (F-09), driven from the
//! keyboard only (F-09.8).

mod common;

use std::time::Duration;

use common::{Env, command, keys, leader, press, press_with, send, wait};
use iced::keyboard::{Key, Modifiers};
use patinote_app::app::{App, Message};
use patinote_core::config::Config;
use patinote_core::ipc::Request;

fn start(env: &Env) -> App {
    env.start(env.launch(Some(env.vault()), Config::default()))
}

fn open(app: &mut App, path: &str) {
    send(app, Message::Remote(Request::Open { path: path.into() }));
}

fn title(app: &App) -> String {
    app.title().trim_end_matches(" — Patinote").to_owned()
}

fn ctrl(app: &mut App, c: &str) {
    press_with(app, Key::Character(c.into()), Modifiers::CTRL, None, false);
}

fn line_text(app: &App) -> String {
    let (line, _) = app.active_cursor().unwrap();
    app.active_text()
        .unwrap()
        .lines()
        .nth(line)
        .unwrap()
        .to_owned()
}

#[test]
fn f09_1_a_new_link_shows_in_backlinks_right_after_the_save() {
    let env = Env::new();
    let mut app = start(&env);
    open(&mut app, "Referensi/Daftar bacaan.md");
    assert_eq!(app.active_backlinks(), Some(0));
    open(&mut app, "Inbox/Ide cepat.md");
    keys(&mut app, "ggOlihat [[Daftar bacaan]]<Esc>");
    wait(&mut app, Duration::from_millis(1100));
    command(&mut app, "b Daftar bacaan");
    assert_eq!(app.active_backlinks(), Some(1));
}

#[test]
fn f09_2_heading_block_and_property_links_are_marked() {
    let env = Env::new();
    let mut app = start(&env);
    open(&mut app, "Proyek/Desain indeks Patinote.md");
    let markers = app.backlink_markers();
    assert!(
        markers.contains(&("Harian/2026-11-19.md".into(), Some("→ #Tabel inti".into()))),
        "{markers:?}"
    );
    open(&mut app, "Proyek/Patinote.md");
    let markers = app.backlink_markers();
    assert!(
        markers.contains(&(
            "Proyek/Desain indeks Patinote.md".into(),
            Some("property".into())
        )),
        "{markers:?}"
    );
}

#[test]
fn f09_3_enter_on_a_backlink_opens_the_source_at_the_link() {
    let env = Env::new();
    let mut app = start(&env);
    open(&mut app, "Proyek/Desain indeks Patinote.md");
    leader(&mut app, "b");
    assert!(app.panel_is_open() && app.panel_focused());
    let target = app.backlink_markers()[0].0.clone();
    press(&mut app, "Enter");
    assert_eq!(
        format!("{}.md", title(&app)),
        target.rsplit('/').next().unwrap()
    );
    assert!(
        line_text(&app).contains("[[Desain indeks Patinote"),
        "{}",
        line_text(&app)
    );
    assert!(!app.panel_focused(), "the editor has the focus again");
}

#[test]
fn f09_4_the_outline_follows_the_cursor_and_enter_jumps() {
    let env = Env::new();
    let mut app = start(&env);
    open(&mut app, "Proyek/Desain indeks Patinote.md");
    keys(&mut app, "gg");
    leader(&mut app, "o");
    // The focus starts on the heading the cursor is under: none yet, so the
    // first; `j` moves to "Tabel inti".
    press(&mut app, "j");
    press(&mut app, "Enter");
    assert_eq!(line_text(&app), "## Tabel inti");
    // Ctrl+w l and Ctrl+w h move the focus without closing the panel.
    ctrl(&mut app, "w");
    press(&mut app, "l");
    assert!(app.panel_focused());
    ctrl(&mut app, "w");
    press(&mut app, "h");
    assert!(!app.panel_focused() && app.panel_is_open());
    leader(&mut app, "o");
    assert!(!app.panel_is_open(), "the leader key closes it again");
}

#[test]
fn f09_5_ctrl_o_and_ctrl_i_walk_the_jump_list() {
    let env = Env::new();
    let mut app = start(&env);
    open(&mut app, "Proyek/Patinote.md");
    keys(&mut app, "3G");
    let a = app.active_cursor();
    open(&mut app, "Inbox/Ide cepat.md");
    open(&mut app, "Referensi/Daftar bacaan.md");
    ctrl(&mut app, "o");
    ctrl(&mut app, "o");
    assert_eq!(title(&app), "Patinote");
    assert_eq!(app.active_cursor(), a);
    ctrl(&mut app, "i");
    assert_eq!(title(&app), "Ide cepat");
}

#[test]
fn f09_6_buffers_and_cursors_come_back_after_a_restart() {
    let env = Env::new();
    let mut app = start(&env);
    open(&mut app, "Proyek/Patinote.md");
    open(&mut app, "Proyek/Desain indeks Patinote.md");
    keys(&mut app, "12G");
    let cursor = app.active_cursor();
    command(&mut app, "qa");
    let app = env.start(env.launch(None, Config::default()));
    assert_eq!(title(&app), "Desain indeks Patinote");
    assert_eq!(app.active_cursor(), cursor);
}

#[test]
fn f09_7_the_21st_buffer_closes_the_oldest_unchanged_one() {
    use std::os::unix::fs::PermissionsExt;
    let env = Env::new();
    let vault = env.vault();
    for i in 1..21 {
        std::fs::write(vault.join(format!("n{i:02}.md")), format!("# {i}\n")).unwrap();
    }
    // n00 cannot be saved (its folder refuses writes), so it stays unsaved.
    std::fs::create_dir(vault.join("keep")).unwrap();
    std::fs::write(vault.join("keep/n00.md"), "# 0\n").unwrap();
    let mut app = env.start(env.launch(Some(vault.clone()), Config::default()));
    open(&mut app, "keep/n00.md");
    std::fs::set_permissions(vault.join("keep"), std::fs::Permissions::from_mode(0o555)).unwrap();
    keys(&mut app, "Abaru<Esc>");
    for i in 1..21 {
        open(&mut app, &format!("n{i:02}.md"));
    }
    let titles: Vec<String> = app.buffer_titles();
    std::fs::set_permissions(vault.join("keep"), std::fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(titles.len(), 20, "{titles:?}");
    assert!(
        titles.contains(&"n00".to_owned()),
        "unsaved buffers stay open"
    );
    assert!(
        !titles.contains(&"n01".to_owned()),
        "the oldest unchanged one closed"
    );
    assert!(titles.contains(&"n20".to_owned()));
}

#[test]
fn buffers_move_with_gt_and_brackets_and_close_with_the_leader() {
    let env = Env::new();
    let mut app = start(&env);
    for p in [
        "Proyek/Patinote.md",
        "Inbox/Ide cepat.md",
        "Referensi/Daftar bacaan.md",
    ] {
        open(&mut app, p);
    }
    keys(&mut app, "gt");
    assert_eq!(title(&app), "Patinote");
    keys(&mut app, "gT");
    assert_eq!(title(&app), "Daftar bacaan");
    keys(&mut app, "[b");
    assert_eq!(title(&app), "Ide cepat");
    keys(&mut app, "]b");
    assert_eq!(title(&app), "Daftar bacaan");
    command(&mut app, "b Patinote");
    assert_eq!(title(&app), "Patinote");
    leader(&mut app, "x");
    assert_eq!(app.buffer_titles(), ["Ide cepat", "Daftar bacaan"]);
}

#[test]
fn a01_7_the_panel_shows_exactly_what_patinote_backlinks_prints() {
    let env = Env::new();
    let mut app = start(&env);
    open(&mut app, "Proyek/Patinote.md");
    let shown: Vec<(String, usize)> = app.backlink_lines();
    // `patinote backlinks` builds its output from the same index query.
    let index = patinote_index::index_path_for(&env.path("Vault"), &env.path("cache")).unwrap();
    let store = patinote_index::SqliteStore::open(&index, &Default::default()).unwrap();
    let row = store.note_by_path("Proyek/Patinote.md").unwrap().unwrap();
    let cli: Vec<(String, usize)> = store
        .backlinks_with_context(row.id)
        .unwrap()
        .into_iter()
        .map(|b| (b.path, b.line))
        .collect();
    assert!(!cli.is_empty());
    assert_eq!(shown, cli);
}

#[test]
fn f09_6_notes_from_the_last_session_show_their_backlinks() {
    let env = Env::new();
    let before = {
        let mut app = start(&env);
        open(&mut app, "Proyek/Desain indeks Patinote.md");
        app.active_backlinks()
    };
    assert!(before.is_some_and(|n| n > 0), "{before:?}");
    let app = start(&env);
    assert_eq!(title(&app), "Desain indeks Patinote");
    assert_eq!(app.active_backlinks(), before);
}

#[test]
fn f09_6_the_active_note_comes_back_when_an_earlier_one_is_gone() {
    let env = Env::new();
    let vault = env.vault();
    let launch = || env.start(env.launch(Some(vault.clone()), Config::default()));
    {
        let mut app = launch();
        open(&mut app, "Inbox/Ide cepat.md");
        open(&mut app, "Proyek/Rapat.md");
        open(&mut app, "Referensi/Daftar bacaan.md");
        command(&mut app, "b Rapat");
        assert_eq!(title(&app), "Rapat");
    }
    // Deleted between sessions.
    std::fs::remove_file(vault.join("Inbox/Ide cepat.md")).unwrap();
    let app = launch();
    assert_eq!(app.buffer_titles(), ["Rapat", "Daftar bacaan"]);
    assert_eq!(title(&app), "Rapat");
}
