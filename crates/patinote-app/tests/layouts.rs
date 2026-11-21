//! Layouts and window controls (F-28): minimal, classic and focus, the
//! leader popup, text size, and panel widths kept between sessions.

mod common;

use common::{Env, command, keys, leader, press, press_with, send};
use iced::keyboard::{self, Key, Location, Modifiers, key};
use patinote_app::app::{App, Message};
use patinote_core::config::{Config, Layout};
use patinote_core::ipc::Request;

fn start(env: &Env) -> App {
    env.start(env.launch(Some(env.vault()), Config::default()))
}

fn space(app: &mut App) {
    // The leader's 300 ms timer is left out: as if the next key came fast.
    let k = Key::Named(key::Named::Space);
    drop(app.update(Message::Key(
        keyboard::Event::KeyPressed {
            key: k.clone(),
            modified_key: k,
            physical_key: key::Physical::Unidentified(key::NativeCode::Unidentified),
            location: Location::Standard,
            modifiers: Modifiers::default(),
            text: Some(" ".into()),
            repeat: false,
        },
        false,
    )));
}

#[test]
fn f28_4_the_popup_shows_after_a_pause_and_not_when_f_follows_fast() {
    let env = Env::new();
    let mut app = start(&env);
    space(&mut app);
    press(&mut app, "f");
    assert!(app.switcher_open());
    assert!(!app.leader_popup_shown(), "f came before 300 ms: no popup");
    press(&mut app, "Escape");
    space(&mut app);
    send(&mut app, Message::LeaderTimeout(u64::MAX));
    assert!(!app.leader_popup_shown(), "an old timer does nothing");
    // Any key ends the waiting leader.
    press(&mut app, "Escape");
    // The real timer, run by the driver, shows it.
    common::press_with(
        &mut app,
        Key::Named(key::Named::Space),
        Modifiers::default(),
        Some(" "),
        false,
    );
    assert!(app.leader_popup_shown());
    press(&mut app, "o");
    assert!(!app.leader_popup_shown());
    assert!(app.panel_is_open());
}

#[test]
fn f28_10_switching_layouts_keeps_buffers_and_the_cursor() {
    let env = Env::new();
    let mut app = start(&env);
    send(
        &mut app,
        Message::Remote(Request::Open {
            path: "Proyek/Desain indeks Patinote.md".into(),
        }),
    );
    send(
        &mut app,
        Message::Remote(Request::Open {
            path: "Proyek/Patinote.md".into(),
        }),
    );
    keys(&mut app, "5G");
    let cursor = app.active_cursor();
    let timed = |app: &mut App, f: &dyn Fn(&mut App)| {
        let started = std::time::Instant::now();
        f(app);
        let _ = app.view();
        let ms = started.elapsed().as_millis();
        assert!(ms < 100, "switching took {ms} ms");
    };
    timed(&mut app, &|app| command(app, "layout classic"));
    assert_eq!(app.layout(), Layout::Classic);
    assert!(app.explorer_is_open() && app.panel_is_open());
    timed(&mut app, &|app| command(app, "layout focus"));
    assert_eq!(app.layout(), Layout::Focus);
    // The leader's timer is not run here: it only waits to show the popup.
    timed(&mut app, &|app| {
        space(app);
        press(app, "l");
    });
    assert_eq!(
        app.layout(),
        Layout::Minimal,
        "the leader cycles back to minimal"
    );
    assert_eq!(app.active_cursor(), cursor);
    assert_eq!(app.buffer_titles(), ["Desain indeks Patinote", "Patinote"]);
    command(&mut app, "layout tiled");
    assert_eq!(app.message().map(|m| m.1), Some("Unknown layout: tiled"));
}

#[test]
fn f28_11_layout_and_panels_come_back_after_a_restart() {
    let env = Env::new();
    let mut app = start(&env);
    command(&mut app, "layout classic");
    command(&mut app, "qa");
    let app = env.start(env.launch(None, Config::default()));
    assert_eq!(app.layout(), Layout::Classic);
    assert!(app.explorer_is_open() && app.panel_is_open());
}

#[test]
fn f28_12_layout_classic_in_the_config_opens_in_classic() {
    let env = Env::new();
    let config = Config {
        layout: Some(Layout::Classic),
        ..Config::default()
    };
    let app = env.start(env.launch(Some(env.vault()), config));
    assert_eq!(app.layout(), Layout::Classic);
    assert!(app.explorer_is_open() && app.panel_is_open());
}

#[test]
fn f28_5_the_status_line_shows_the_new_mode_at_once() {
    let env = Env::new();
    let mut app = start(&env);
    send(
        &mut app,
        Message::Remote(Request::Open {
            path: "Proyek/Patinote.md".into(),
        }),
    );
    assert_eq!(app.status_mode(), "NORMAL");
    press(&mut app, "i");
    assert_eq!(app.status_mode(), "INSERT");
    press(&mut app, "Escape");
    press(&mut app, "v");
    assert_eq!(app.status_mode(), "VISUAL");
}

#[test]
fn f28_3_leader_e_b_o_open_and_close_their_panels() {
    let env = Env::new();
    let mut app = start(&env);
    send(
        &mut app,
        Message::Remote(Request::Open {
            path: "Proyek/Patinote.md".into(),
        }),
    );
    leader(&mut app, "e");
    assert!(app.explorer_is_open());
    press(&mut app, "Escape");
    leader(&mut app, "e");
    assert!(!app.explorer_is_open());
    assert!(
        app.explorer_rows().is_empty(),
        "a closed explorer drops its rows"
    );
    leader(&mut app, "b");
    assert!(app.panel_is_open());
    press(&mut app, "Escape");
    leader(&mut app, "o");
    assert!(!app.panel_is_open());
}

#[test]
fn f28_7_a_dragged_panel_width_is_kept() {
    let env = Env::new();
    let mut app = start(&env);
    send(
        &mut app,
        Message::Remote(Request::Open {
            path: "Proyek/Patinote.md".into(),
        }),
    );
    leader(&mut app, "e");
    press(&mut app, "Escape");
    send(
        &mut app,
        Message::DragStart(patinote_app::app::Edge::Explorer),
    );
    send(&mut app, Message::DragMove(320.0));
    send(&mut app, Message::DragEnd);
    // Too far is held at the limit.
    send(
        &mut app,
        Message::DragStart(patinote_app::app::Edge::Explorer),
    );
    send(&mut app, Message::DragMove(5.0));
    send(&mut app, Message::DragEnd);
    assert_eq!(app.explorer_w(), 180.0);
    send(
        &mut app,
        Message::DragStart(patinote_app::app::Edge::Explorer),
    );
    send(&mut app, Message::DragMove(320.0));
    send(&mut app, Message::DragEnd);
    command(&mut app, "qa");
    let app = env.start(env.launch(None, Config::default()));
    assert_eq!(app.explorer_w(), 320.0);
}

#[test]
fn text_size_steps_by_ten_percent_within_its_limits() {
    let env = Env::new();
    let mut app = start(&env);
    let ctrl = |app: &mut App, c: &str| {
        press_with(app, Key::Character(c.into()), Modifiers::CTRL, None, false)
    };
    ctrl(&mut app, "=");
    assert!((app.zoom() - 1.1).abs() < 0.001);
    for _ in 0..20 {
        ctrl(&mut app, "=");
    }
    assert!((app.zoom() - 2.0).abs() < 0.001);
    ctrl(&mut app, "0");
    assert_eq!(app.zoom(), 1.0);
    for _ in 0..10 {
        ctrl(&mut app, "-");
    }
    assert!((app.zoom() - 0.7).abs() < 0.001);
}
