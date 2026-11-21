//! The app shell without a window (step 2.2): screens, opening notes, the
//! command line, remembered state and the single-instance socket. Tasks are
//! run to completion and their messages fed back into `App::update`, as the
//! Iced runtime would.

mod common;

use std::path::{Path, PathBuf};

use common::{Env, command, press, send};
use iced::Size;
use iced::keyboard::{self, Key, Location, Modifiers, key};
use patinote_app::app::{App, Message, Screen};
use patinote_app::theme::styles::Level;
use patinote_core::config::{Config, Layout};

fn workspace(app: &App) -> &patinote_app::app::Workspace {
    match app.screen() {
        Screen::Vault(w) => w,
        _ => panic!("no vault is open"),
    }
}

fn open_paths(app: &App) -> Vec<String> {
    workspace(app)
        .buffers()
        .iter()
        .map(|b| b.path.clone())
        .collect()
}

#[test]
fn f01_first_run_shows_the_welcome_screen() {
    let env = Env::new();
    let app = env.start(env.launch(None, Config::default()));
    assert!(matches!(app.screen(), Screen::Welcome));
    assert_eq!(app.title(), "Patinote");
}

#[test]
fn f01_a_missing_vault_shows_vault_not_found_and_try_again_opens_it() {
    let env = Env::new();
    let gone = env.path("Vault");
    let mut app = env.start(env.launch(Some(gone.clone()), Config::default()));
    assert!(matches!(app.screen(), Screen::Missing(p) if *p == gone));
    // Nothing was created where the vault was.
    assert!(!gone.exists());
    env.vault();
    press(&mut app, "r");
    assert!(matches!(app.screen(), Screen::Vault(_)));
}

#[test]
fn f01_2_opening_the_vault_does_not_change_it() {
    let env = Env::new();
    let vault = env.vault();
    let before = snapshot(&vault);
    let mut app = env.start(env.launch(Some(vault.clone()), Config::default()));
    send(
        &mut app,
        Message::Remote(patinote_core::ipc::Request::Open {
            path: "Proyek/Patinote.md".into(),
        }),
    );
    command(&mut app, "q");
    assert_eq!(snapshot(&vault), before);
    // Index and state live outside the vault.
    assert!(env.path("cache/patinote").is_dir());
    assert!(env.path("state/patinote/app.json").is_file());
}

/// Every file under `dir` with its bytes.
fn snapshot(dir: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).unwrap() {
            let e = e.unwrap();
            if e.file_type().unwrap().is_dir() {
                stack.push(e.path());
            } else {
                out.push((e.path(), std::fs::read(e.path()).unwrap()));
            }
        }
    }
    out.sort();
    out
}

#[test]
fn f28_2_first_open_is_minimal_with_no_note_open() {
    let env = Env::new();
    let app = env.start(env.launch(Some(env.vault()), Config::default()));
    let w = workspace(&app);
    assert_eq!(w.layout(), Layout::Minimal);
    assert!(w.buffers().is_empty());
    assert_eq!(app.message(), None);
}

#[test]
fn f28_12_layout_classic_in_the_config_applies_to_a_new_install() {
    let env = Env::new();
    let config = Config {
        layout: Some(Layout::Classic),
        ..Config::default()
    };
    let app = env.start(env.launch(Some(env.vault()), config));
    assert_eq!(workspace(&app).layout(), Layout::Classic);
}

#[test]
fn e_opens_a_note_by_name_alias_or_path_and_reports_the_rest() {
    let env = Env::new();
    let mut app = env.start(env.launch(Some(env.vault()), Config::default()));
    command(&mut app, "e second brain");
    command(&mut app, "e Harian/2026-11-19.md");
    command(&mut app, "e Patinote");
    assert_eq!(
        open_paths(&app),
        ["Proyek/Patinote.md", "Harian/2026-11-19.md"],
        "opening an open note switches to it"
    );
    assert_eq!(workspace(&app).active(), Some(0));
    assert_eq!(app.title(), "Patinote — Patinote");

    command(&mut app, "e Rapat");
    assert_eq!(
        app.message(),
        Some((Level::Warning, "2 notes are named \"Rapat\""))
    );
    command(&mut app, "e Nothing here");
    assert_eq!(
        app.message(),
        Some((Level::Error, "No note named \"Nothing here\""))
    );
    command(&mut app, "frobnicate");
    assert_eq!(
        app.message(),
        Some((Level::Error, "Not an editor command: frobnicate"))
    );
}

#[test]
fn buffers_switch_and_close_from_the_command_line() {
    let env = Env::new();
    let mut app = env.start(env.launch(Some(env.vault()), Config::default()));
    for name in ["Patinote", "Ide cepat", "Daftar bacaan"] {
        command(&mut app, &format!("e {name}"));
    }
    assert_eq!(workspace(&app).active(), Some(2));
    command(&mut app, "bn");
    assert_eq!(workspace(&app).active(), Some(0));
    command(&mut app, "bp");
    assert_eq!(workspace(&app).active(), Some(2));
    command(&mut app, "b 2");
    assert_eq!(workspace(&app).active(), Some(1));
    command(&mut app, "bd");
    assert_eq!(
        open_paths(&app),
        ["Proyek/Patinote.md", "Referensi/Daftar bacaan.md"]
    );
}

#[test]
fn escape_closes_the_command_line() {
    let env = Env::new();
    let mut app = env.start(env.launch(Some(env.vault()), Config::default()));
    press(&mut app, ":");
    assert!(app.typing_command());
    // The input captures Escape, so the app sees it as a captured key.
    let esc = Key::Named(key::Named::Escape);
    send(
        &mut app,
        Message::Key(
            keyboard::Event::KeyPressed {
                key: esc.clone(),
                modified_key: esc,
                physical_key: key::Physical::Unidentified(key::NativeCode::Unidentified),
                location: Location::Standard,
                modifiers: Modifiers::default(),
                text: None,
                repeat: false,
            },
            true,
        ),
    );
    assert!(!app.typing_command());
}

#[test]
fn recent_notes_and_window_size_are_remembered() {
    let env = Env::new();
    let vault = env.vault();
    let mut app = env.start(env.launch(Some(vault.clone()), Config::default()));
    command(&mut app, "e Patinote");
    command(&mut app, "e Ide cepat");
    send(&mut app, Message::Resized(Size::new(1200.0, 700.0)));
    // `:q` closes a note (F-06.4); `:qa` quits and saves the window size.
    command(&mut app, "qa");

    // Next start: no vault given, the last one opens with its buffers back
    // (F-09.6). Closing them shows the start screen with recent notes,
    // newest first; j and Enter open the second.
    let mut app = env.start(env.launch(None, Config::default()));
    assert_eq!(
        open_paths(&app),
        ["Proyek/Patinote.md", "Inbox/Ide cepat.md"]
    );
    command(&mut app, "bd");
    command(&mut app, "bd");
    assert!(workspace(&app).buffers().is_empty());
    press(&mut app, "j");
    press(&mut app, "Enter");
    assert_eq!(open_paths(&app), ["Proyek/Patinote.md"]);
    assert_eq!(
        patinote_app::app::window_size(&env.path("state")),
        Size::new(1200.0, 700.0)
    );
}

#[test]
fn the_socket_hands_requests_to_the_running_app() {
    let env = Env::new();
    let vault = env.vault();
    let runtime = env.path("run");
    let mut launch = env.launch(Some(vault.clone()), Config::default());
    launch.runtime_dir = Some(runtime.clone());
    // The socket listener never ends, so only run the opening tasks that do.
    let (mut app, _task) = App::new(launch);
    assert!(matches!(app.screen(), Screen::Vault(_)));
    let id = patinote_core::location::vault_id(&vault).unwrap();
    let socket = patinote_core::ipc::socket_path(&runtime, &id);
    assert!(socket.exists());
    // A second instance for the same vault cannot bind.
    assert!(matches!(
        patinote_core::ipc::Server::bind(&socket),
        Err(patinote_core::ipc::BindError::AlreadyRunning)
    ));
    // No note is open, so `:q` quits.
    press(&mut app, ":");
    drop(app.update(Message::CmdInput("q".into())));
    drop(app.update(Message::CmdSubmit));
    assert!(!socket.exists(), "quitting removes the socket");
}

#[test]
fn open_with_a_note_opens_it_while_the_watcher_and_socket_run() {
    // `patinote open` starts the app with `--open` when none is running.
    // The vault's tasks never end, so the note must not wait for them.
    let env = Env::new();
    let runtime = env.path("run");
    std::fs::create_dir_all(&runtime).unwrap();
    let launch = patinote_app::app::Launch {
        note: Some("Proyek/Patinote.md".into()),
        watch: true,
        runtime_dir: Some(runtime),
        ..env.launch(Some(env.vault()), Config::default())
    };
    let (mut app, task) = App::new(launch);
    let opened = common::run_until(&mut app, task, std::time::Duration::from_secs(10), |a| {
        a.active_cursor().is_some()
    });
    assert!(opened, "the note did not open");
    assert_eq!(open_paths(&app), ["Proyek/Patinote.md"]);
}

#[test]
fn an_open_request_only_opens_notes_of_the_vault() {
    let env = Env::new();
    let vault = env.vault();
    std::fs::write(env.path("luar.md"), "di luar vault\n").unwrap();
    let mut app = env.start(env.launch(Some(vault), Config::default()));
    for path in ["../luar.md", "/etc/hostname", "Tidak ada.md"] {
        send(
            &mut app,
            Message::Remote(patinote_core::ipc::Request::Open { path: path.into() }),
        );
        assert_eq!(open_paths(&app), Vec::<String>::new(), "{path}");
        assert_eq!(app.message().map(|(l, _)| l), Some(Level::Error), "{path}");
    }
    // A name, as `--open Patinote` gives it, is looked up like `:e`.
    send(
        &mut app,
        Message::Remote(patinote_core::ipc::Request::Open {
            path: "Daftar bacaan".into(),
        }),
    );
    assert_eq!(open_paths(&app), ["Referensi/Daftar bacaan.md"]);
}

#[test]
fn a_second_app_for_an_open_vault_hands_over_instead_of_opening_it() {
    let env = Env::new();
    let vault = env.vault();
    let runtime = env.path("run");
    let launch = || patinote_app::app::Launch {
        runtime_dir: Some(runtime.clone()),
        ..env.launch(Some(vault.clone()), Config::default())
    };
    // The socket listener never ends, so the first app's tasks are not run.
    let (first, _task) = App::new(launch());
    assert!(matches!(first.screen(), Screen::Vault(_)));
    // As when the vault is picked on the welcome screen of a second app.
    let (second, task) = App::new(launch());
    assert!(
        !matches!(second.screen(), Screen::Vault(_)),
        "two apps would autosave the same notes"
    );
    let mut second = second;
    common::run(&mut second, task);
    drop(first);
}

#[test]
fn a_note_asked_for_at_startup_is_handed_to_the_running_app() {
    let env = Env::new();
    let vault = env.vault();
    let runtime = env.path("run");
    let launch = |note: Option<&str>| patinote_app::app::Launch {
        runtime_dir: Some(runtime.clone()),
        note: note.map(str::to_owned),
        ..env.launch(Some(vault.clone()), Config::default())
    };
    let (mut first, first_task) = App::new(launch(None));
    let (mut second, task) = App::new(launch(Some("Proyek/Patinote.md")));
    common::run(&mut second, task);
    assert_ne!(
        second.message().map(|(l, _)| l),
        Some(Level::Error),
        "{:?}",
        second.message()
    );
    let opened = common::run_until(
        &mut first,
        first_task,
        std::time::Duration::from_secs(10),
        |a| a.active_cursor().is_some(),
    );
    assert!(opened, "the running app opened the note");
    assert_eq!(open_paths(&first), ["Proyek/Patinote.md"]);
}
