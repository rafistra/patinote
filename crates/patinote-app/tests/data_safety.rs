//! Unsaved text survives saves that are still running or that cannot land:
//! closing, reopening, quitting, renaming and deleting a note at the wrong
//! moment never drops what was typed (F-05, F-10, F-11).

mod common;

use std::time::{Duration, Instant};

use common::{Env, command, keys, leader, press, run, send};
use iced::Task;
use patinote_app::app::{App, Message};
use patinote_app::theme::styles::Level;
use patinote_core::config::Config;
use patinote_core::ipc::Request;

const NOTE: &str = "Inbox/Ide cepat.md";

fn open(env: &Env) -> App {
    let mut app = env.start(env.launch(Some(env.vault()), Config::default()));
    send(
        &mut app,
        Message::Remote(Request::Open { path: NOTE.into() }),
    );
    assert!(app.active_text().is_some());
    app
}

fn disk(env: &Env, rel: &str) -> String {
    std::fs::read_to_string(env.path("Vault").join(rel)).unwrap()
}

/// Starts the autosave of what was typed, without letting it finish.
fn start_autosave(app: &mut App) -> Task<Message> {
    app.update(Message::Tick(Instant::now() + Duration::from_millis(1100)))
}

/// Runs a `:` command, without running what it starts.
fn command_held(app: &mut App, line: &str) -> Task<Message> {
    press(app, ":");
    send(app, Message::CmdInput(line.into()));
    app.update(Message::CmdSubmit)
}

/// The note changes on disk after it was read, so the next save of the
/// open note is refused (F-11.3) and waits for the user.
fn conflict(env: &Env, app: &mut App) {
    std::fs::write(env.path("Vault").join(NOTE), "dari luar\n").unwrap();
    command(app, "w");
    assert!(app.external_title().is_some(), "the banner is up");
}

fn titles(app: &App) -> Vec<String> {
    app.buffer_titles()
}

/// Opens the explorer, selects `path` and presses `key`.
fn explorer_key(app: &mut App, path: &str, key: &str) {
    if !app.explorer_is_open() {
        leader(app, "e");
    } else {
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

#[test]
fn closing_while_a_save_runs_saves_the_newer_text() {
    let env = Env::new();
    let mut app = open(&env);
    keys(&mut app, "ggOsatu<Esc>");
    let first = start_autosave(&mut app);
    keys(&mut app, "ggOdua<Esc>");
    let close = command_held(&mut app, "q");
    run(&mut app, first);
    run(&mut app, close);
    assert!(
        disk(&env, NOTE).starts_with("dua\nsatu\n"),
        "{:?}",
        disk(&env, NOTE)
    );
    assert!(!titles(&app).iter().any(|t| t == "Ide cepat"), "closed");
    assert!(!app.quitting());
}

#[test]
fn reopening_a_note_that_is_still_saving_shows_its_last_text() {
    let env = Env::new();
    let mut app = open(&env);
    keys(&mut app, "ggOtiga<Esc>");
    let close = command_held(&mut app, "q");
    send(
        &mut app,
        Message::Remote(Request::Open { path: NOTE.into() }),
    );
    run(&mut app, close);
    assert!(app.active_text().unwrap().starts_with("tiga\n"));
    assert_eq!(
        titles(&app).iter().filter(|t| *t == "Ide cepat").count(),
        1,
        "{:?}",
        titles(&app)
    );
    keys(&mut app, "ggOempat<Esc>");
    command(&mut app, "w");
    assert!(disk(&env, NOTE).starts_with("empat\ntiga\n"));
    assert_eq!(app.external_title(), None, "no false conflict");
}

#[test]
fn quitting_with_a_conflict_says_why_and_does_not_quit_later() {
    let env = Env::new();
    let mut app = open(&env);
    keys(&mut app, "ggOlima<Esc>");
    conflict(&env, &mut app);
    send(
        &mut app,
        Message::CloseRequested(iced::window::Id::unique()),
    );
    let (level, text) = app.message().expect("a message");
    assert_eq!(level, Level::Error, "{text}");
    assert!(text.contains("Ide cepat"), "{text}");
    assert!(!app.quitting(), "quitting is not left pending");
}

#[test]
fn renaming_a_note_whose_save_cannot_land_keeps_its_text() {
    let env = Env::new();
    let mut app = open(&env);
    keys(&mut app, "ggOenam<Esc>");
    conflict(&env, &mut app);
    explorer_key(&mut app, NOTE, "r");
    send(&mut app, Message::DialogInput("Ide baru".into()));
    send(&mut app, Message::DialogSubmit);
    assert_eq!(app.message().map(|(l, _)| l), Some(Level::Error));
    assert!(env.path("Vault").join(NOTE).exists(), "not renamed");
    let text = app.buffer_text(NOTE).expect("still open");
    assert!(text.starts_with("enam\n"), "{text:?}");
}

#[test]
fn deleting_a_note_whose_save_cannot_land_keeps_its_text() {
    let env = Env::new();
    let mut app = open(&env);
    keys(&mut app, "ggOtujuh<Esc>");
    conflict(&env, &mut app);
    explorer_key(&mut app, NOTE, "d");
    press(&mut app, "Enter");
    assert_eq!(app.message().map(|(l, _)| l), Some(Level::Error));
    assert!(env.path("Vault").join(NOTE).exists(), "not deleted");
    assert!(titles(&app).iter().any(|t| t == "Ide cepat"), "still open");
}

#[test]
fn text_typed_while_a_rename_runs_is_kept_at_the_new_path() {
    let env = Env::new();
    let mut app = open(&env);
    explorer_key(&mut app, NOTE, "r");
    send(&mut app, Message::DialogInput("Ide baru".into()));
    let rename = app.update(Message::DialogSubmit);
    // Typed after Enter, before the move lands.
    common::press_with(
        &mut app,
        iced::keyboard::Key::Character("w".into()),
        iced::keyboard::Modifiers::CTRL,
        None,
        false,
    );
    press(&mut app, "l");
    keys(&mut app, "ggOdelapan<Esc>");
    assert!(app.active_text().unwrap().starts_with("delapan\n"));
    run(&mut app, rename);
    let text = app
        .buffer_text("Inbox/Ide baru.md")
        .expect("the buffer follows the rename");
    assert!(text.starts_with("delapan\n"), "{text:?}");
    command(&mut app, "w");
    assert!(disk(&env, "Inbox/Ide baru.md").starts_with("delapan\n"));
    assert!(!env.path("Vault").join(NOTE).exists());
}
