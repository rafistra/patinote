//! The quick switcher in the app (F-07): opened with the leader key or
//! `Ctrl+Shift+O`, driven only by the keyboard.

mod common;

use std::time::Duration;

use common::{Env, keys, leader, press, press_with, send, wait};
use iced::keyboard::{Key, Modifiers, key};
use patinote_app::app::{App, Message};
use patinote_app::theme::styles::Level;
use patinote_core::config::Config;

fn start(env: &Env) -> App {
    env.start(env.launch(Some(env.vault()), Config::default()))
}

fn open_note_path(app: &App) -> Option<String> {
    app.title().strip_suffix(" — Patinote").map(str::to_owned)
}

#[test]
fn f07_2_an_alias_finds_its_note_and_enter_opens_it() {
    let env = Env::new();
    let mut app = start(&env);
    leader(&mut app, "f");
    assert!(app.switcher_open());
    send(&mut app, Message::SwitcherInput("second brain".into()));
    assert_eq!(app.switcher_titles()[0], "Patinote");
    send(&mut app, Message::SwitcherSubmit);
    assert!(!app.switcher_open());
    assert_eq!(open_note_path(&app).as_deref(), Some("Patinote"));
}

#[test]
fn f07_4_the_empty_box_lists_recent_notes_newest_first() {
    let env = Env::new();
    let mut app = start(&env);
    for name in ["Patinote", "Ide cepat", "2026-11-19"] {
        common::command(&mut app, &format!("e {name}"));
    }
    leader(&mut app, "f");
    assert_eq!(
        app.switcher_titles(),
        ["2026-11-19", "Ide cepat", "Patinote"]
    );
}

#[test]
fn f07_5_shift_enter_creates_the_note_and_opens_it_in_insert_mode() {
    let env = Env::new();
    let mut app = start(&env);
    let shift = Modifiers::SHIFT;
    press_with(
        &mut app,
        Key::Character("o".into()),
        Modifiers::CTRL | shift,
        None,
        false,
    );
    assert!(app.switcher_open(), "Ctrl+Shift+O opens the switcher");
    send(&mut app, Message::SwitcherInput("Catatan baru".into()));
    send(
        &mut app,
        Message::Key(iced::keyboard::Event::ModifiersChanged(shift), false),
    );
    send(&mut app, Message::SwitcherSubmit);
    send(
        &mut app,
        Message::Key(
            iced::keyboard::Event::ModifiersChanged(Modifiers::default()),
            false,
        ),
    );
    assert_eq!(open_note_path(&app).as_deref(), Some("Catatan baru"));
    let file = env.path("Vault/Catatan baru.md");
    // Nothing on disk until the note has text.
    wait(&mut app, Duration::from_secs(2));
    assert!(!file.exists());
    // Already in insert mode: typed text goes into the note.
    keys(&mut app, "Isi pertama<Esc>");
    wait(&mut app, Duration::from_secs(2));
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "Isi pertama");
}

#[test]
fn f07_7_every_switcher_action_works_from_the_keyboard() {
    let env = Env::new();
    let mut app = start(&env);
    leader(&mut app, "f");
    send(&mut app, Message::SwitcherInput("desain".into()));
    let titles = app.switcher_titles();
    assert!(titles.len() >= 2, "{titles:?}");
    // Ctrl+n moves down, Ctrl+p back up, arrows too; Enter opens.
    press_with(
        &mut app,
        Key::Character("n".into()),
        Modifiers::CTRL,
        None,
        false,
    );
    press_with(
        &mut app,
        Key::Named(key::Named::ArrowDown),
        Modifiers::default(),
        None,
        false,
    );
    press_with(
        &mut app,
        Key::Character("p".into()),
        Modifiers::CTRL,
        None,
        false,
    );
    send(&mut app, Message::SwitcherSubmit);
    assert_eq!(open_note_path(&app).as_deref(), Some(titles[1].as_str()));
    // Escape closes without opening anything.
    leader(&mut app, "f");
    press_with(
        &mut app,
        Key::Named(key::Named::Escape),
        Modifiers::default(),
        None,
        true,
    );
    assert!(!app.switcher_open());
    assert_eq!(open_note_path(&app).as_deref(), Some(titles[1].as_str()));
}

#[test]
fn space_without_f_does_not_open_the_switcher() {
    let env = Env::new();
    let mut app = start(&env);
    leader(&mut app, "x");
    press(&mut app, "j");
    assert!(!app.switcher_open());
}

/// Shift+Enter in the open switcher with `query` typed.
fn shift_enter(app: &mut App, query: &str) {
    send(app, Message::SwitcherInput(query.into()));
    send(
        app,
        Message::Key(
            iced::keyboard::Event::ModifiersChanged(Modifiers::SHIFT),
            false,
        ),
    );
    send(app, Message::SwitcherSubmit);
    send(
        app,
        Message::Key(
            iced::keyboard::Event::ModifiersChanged(Modifiers::default()),
            false,
        ),
    );
}

#[test]
fn f10_2_shift_enter_checks_the_name_like_the_dialog() {
    let env = Env::new();
    let mut app = start(&env);
    let open = |app: &mut App| {
        press_with(
            app,
            Key::Character("o".into()),
            Modifiers::CTRL | Modifiers::SHIFT,
            None,
            false,
        )
    };
    open(&mut app);
    shift_enter(&mut app, "Ide: baru");
    assert!(app.switcher_open(), "a forbidden name is refused");
    assert_eq!(open_note_path(&app), None);
    assert_eq!(app.message().map(|(l, _)| l), Some(Level::Error));

    // A risky name warns first and is created on the second Shift+Enter.
    shift_enter(&mut app, "a#b");
    assert!(app.switcher_open());
    assert_eq!(app.message().map(|(l, _)| l), Some(Level::Warning));
    shift_enter(&mut app, "a#b");
    assert_eq!(open_note_path(&app).as_deref(), Some("a#b"));
}
