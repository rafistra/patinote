//! Reading mode (F-27): switching at the same place, links from the
//! keyboard, checkboxes that save, and images kept at the size they show.

mod common;

use common::{Env, keys, leader, press, press_with, send};
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

#[test]
fn leader_r_switches_reading_mode_on_and_off() {
    let env = Env::new();
    let mut app = open(&env, "Proyek/Desain indeks Patinote.md");
    leader(&mut app, "r");
    assert!(app.reading());
    // Edits do nothing in reading mode.
    keys(&mut app, "dd");
    leader(&mut app, "r");
    assert!(!app.reading());
    assert_eq!(app.active_dirty(), Some(false));
}

#[test]
fn f27_3_a_checkbox_changes_only_its_box_in_the_file() {
    let env = Env::new();
    let mut app = open(&env, "Proyek/Desain indeks Patinote.md");
    let file = env.path("Vault/Proyek/Desain indeks Patinote.md");
    let before = std::fs::read_to_string(&file).unwrap();
    let line = before
        .lines()
        .position(|l| l.contains("- [ ] Finalisasi"))
        .unwrap();
    leader(&mut app, "r");
    send(&mut app, Message::ToggleTask(line));
    let after = std::fs::read_to_string(&file).unwrap();
    assert_eq!(
        after,
        before.replacen("- [ ] Finalisasi", "- [x] Finalisasi", 1)
    );
}

#[test]
fn f27_5_tab_then_enter_opens_the_link() {
    let env = Env::new();
    let mut app = open(&env, "Proyek/Desain indeks Patinote.md");
    leader(&mut app, "r");
    // The first link is the `project` property, the second [[Patinote]].
    press_with(
        &mut app,
        Key::Named(key::Named::Tab),
        Modifiers::default(),
        None,
        false,
    );
    press_with(
        &mut app,
        Key::Named(key::Named::Tab),
        Modifiers::default(),
        None,
        false,
    );
    assert_eq!(app.reading_focus(), Some(1));
    press(&mut app, "Enter");
    assert_eq!(app.title(), "Patinote — Patinote");
}

#[test]
fn f27_6_back_to_editing_puts_the_cursor_where_reading_was() {
    let env = Env::new();
    let mut app = open(&env, "Proyek/Desain indeks Patinote.md");
    let lines = app.active_text().unwrap().lines().count();
    leader(&mut app, "r");
    send(&mut app, Message::ReadScrolled(1.0));
    leader(&mut app, "r");
    let (line, _) = app.active_cursor().unwrap();
    assert!(line + 3 >= lines, "cursor on line {line} of {lines}");
    // And reading starts where the cursor is: half way stays half way.
    keys(&mut app, &format!("{}G", lines / 2));
    leader(&mut app, "r");
    leader(&mut app, "r");
    let (line, _) = app.active_cursor().unwrap();
    assert!(line.abs_diff(lines / 2 - 1) <= 1, "cursor on line {line}");
}

#[test]
fn f27_4_a_large_image_is_kept_at_column_width() {
    let env = Env::new();
    let file = env.path("big.png");
    image::RgbaImage::new(4000, 3000).save(&file).unwrap();
    let (w, h) = patinote_app::app::decoded_size(&file, 720).unwrap();
    assert_eq!((w, h), (720, 540));
    // 1.5 MB kept, instead of 48 MB for the full image.
    assert!(u64::from(w) * u64::from(h) * 4 < 2_000_000);
}
