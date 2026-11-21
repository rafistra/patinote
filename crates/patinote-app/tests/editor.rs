//! The editor end to end (F-05, F-06.4): typing through the Vim engine,
//! autosave, line endings, failed saves and read-only notes.

mod common;

use std::time::Duration;

use common::{Env, command, keys, send, wait};
use patinote_app::app::{App, Message};
use patinote_core::config::Config;
use patinote_core::ipc::Request;

fn open(env: &Env, path: &str) -> App {
    let mut app = env.start(env.launch(Some(env.vault()), Config::default()));
    send(
        &mut app,
        Message::Remote(Request::Open { path: path.into() }),
    );
    assert!(app.active_text().is_some(), "{path} is open");
    app
}

fn disk(env: &Env, path: &str) -> Vec<u8> {
    std::fs::read(env.path("Vault").join(path)).unwrap()
}

#[test]
fn f05_2_one_second_after_typing_the_file_has_the_text() {
    let env = Env::new();
    let mut app = open(&env, "Inbox/Ide cepat.md");
    keys(&mut app, "ggOHalo dunia<Esc>");
    assert_eq!(app.active_dirty(), Some(true));
    // Not yet: less than a second has passed.
    wait(&mut app, Duration::from_millis(200));
    assert!(
        !String::from_utf8(disk(&env, "Inbox/Ide cepat.md"))
            .unwrap()
            .starts_with("Halo dunia\n")
    );
    wait(&mut app, Duration::from_millis(1100));
    let text = String::from_utf8(disk(&env, "Inbox/Ide cepat.md")).unwrap();
    assert!(text.starts_with("Halo dunia\n"), "{text:?}");
    assert_eq!(app.active_dirty(), Some(false));
}

#[test]
fn f05_4_crlf_and_bom_are_kept_and_unchanged_files_are_not_rewritten() {
    let env = Env::new();
    let vault = env.vault();
    std::fs::write(vault.join("crlf.md"), b"\xEF\xBB\xBFsatu\r\ndua\r\n").unwrap();
    let mut app = env.start(env.launch(Some(vault.clone()), Config::default()));
    send(
        &mut app,
        Message::Remote(Request::Open {
            path: "crlf.md".into(),
        }),
    );
    keys(&mut app, "Atiga<Esc>");
    command(&mut app, "w");
    assert_eq!(disk(&env, "crlf.md"), b"\xEF\xBB\xBFsatutiga\r\ndua\r\n");

    // An edit that is undone leaves the same bytes: nothing is written.
    let before = std::fs::metadata(vault.join("crlf.md"))
        .unwrap()
        .modified()
        .unwrap();
    std::thread::sleep(Duration::from_millis(20));
    keys(&mut app, "xu");
    command(&mut app, "w");
    let after = std::fs::metadata(vault.join("crlf.md"))
        .unwrap()
        .modified()
        .unwrap();
    assert_eq!(before, after, "the file was not written again");
}

#[test]
fn f05_5_a_failed_save_shows_an_error_keeps_the_text_and_the_file() {
    use std::os::unix::fs::PermissionsExt;
    let env = Env::new();
    let mut app = open(&env, "Inbox/Ide cepat.md");
    let folder = env.path("Vault/Inbox");
    let original = disk(&env, "Inbox/Ide cepat.md");
    // Writing fails: the folder does not accept the temporary file. (A full
    // disk fails the same way, at the same step.)
    std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o555)).unwrap();
    keys(&mut app, "ggOtidak hilang<Esc>");
    command(&mut app, "w");
    let banner = app.banner_title();
    std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(
        banner.as_deref(),
        Some("Couldn't save Ide cepat.md: permission denied.")
    );
    assert!(app.active_text().unwrap().starts_with("tidak hilang\n"));
    assert_eq!(disk(&env, "Inbox/Ide cepat.md"), original);

    // Patinote keeps retrying; once writing works again the text lands.
    wait(&mut app, Duration::from_secs(6));
    assert!(
        String::from_utf8(disk(&env, "Inbox/Ide cepat.md"))
            .unwrap()
            .starts_with("tidak hilang\n")
    );
    assert_eq!(app.banner_title(), None);
}

#[test]
fn f05_8_large_and_latin1_notes_are_read_only() {
    let env = Env::new();
    let vault = env.vault();
    std::fs::write(vault.join("latin1.md"), b"caf\xE9 au lait\n").unwrap();
    let mut big = b"# Besar\n".to_vec();
    big.resize(6 * 1024 * 1024, b'a');
    std::fs::write(vault.join("besar.md"), &big).unwrap();
    let mut app = env.start(env.launch(Some(vault.clone()), Config::default()));
    for (path, title) in [
        ("latin1.md", "Read-only: this file isn't UTF-8 text."),
        ("besar.md", "Read-only: this note is larger than 5 MB."),
    ] {
        let before = std::fs::read(vault.join(path)).unwrap();
        send(
            &mut app,
            Message::Remote(Request::Open { path: path.into() }),
        );
        assert_eq!(app.banner_title().as_deref(), Some(title));
        let text = app.active_text().unwrap();
        keys(&mut app, "ixyz<Esc>ddp");
        command(&mut app, "w");
        assert_eq!(app.active_text().unwrap(), text, "{path}: nothing changed");
        assert_eq!(
            std::fs::read(vault.join(path)).unwrap(),
            before,
            "{path}: same bytes"
        );
    }
}

#[test]
fn f06_4_q_closes_the_note_and_q_bang_drops_unsaved_changes() {
    let env = Env::new();
    let mut app = open(&env, "Inbox/Ide cepat.md");
    let original = disk(&env, "Inbox/Ide cepat.md");
    keys(&mut app, "ggOdibuang<Esc>");
    command(&mut app, "q!");
    assert!(
        !app.screen().note_open(),
        "the note closed, the app did not quit"
    );
    assert_eq!(disk(&env, "Inbox/Ide cepat.md"), original);

    send(
        &mut app,
        Message::Remote(Request::Open {
            path: "Inbox/Ide cepat.md".into(),
        }),
    );
    keys(&mut app, "ggOdisimpan<Esc>");
    command(&mut app, "q");
    assert!(!app.screen().note_open());
    assert!(
        String::from_utf8(disk(&env, "Inbox/Ide cepat.md"))
            .unwrap()
            .starts_with("disimpan\n")
    );
}

#[test]
fn switching_notes_saves_the_one_left() {
    let env = Env::new();
    let mut app = open(&env, "Inbox/Ide cepat.md");
    keys(&mut app, "ggOpindah<Esc>");
    send(
        &mut app,
        Message::Remote(Request::Open {
            path: "Proyek/Patinote.md".into(),
        }),
    );
    assert!(
        String::from_utf8(disk(&env, "Inbox/Ide cepat.md"))
            .unwrap()
            .starts_with("pindah\n")
    );
}

#[test]
fn f06_1_wq_and_x_save_and_close() {
    let env = Env::new();
    for cmd in ["wq", "x"] {
        let mut app = open(&env, "Inbox/Ide cepat.md");
        keys(&mut app, &format!("ggO{cmd}<Esc>"));
        command(&mut app, cmd);
        assert!(!app.screen().note_open(), ":{cmd} closes the note");
        let text = String::from_utf8(disk(&env, "Inbox/Ide cepat.md")).unwrap();
        assert!(
            text.starts_with(&format!("{cmd}\n")),
            ":{cmd} saved: {text:?}"
        );
    }
}

#[test]
fn f05_relative_line_numbers_can_be_turned_off() {
    use patinote_app::theme::styles::Level;
    let env = Env::new();
    let vault = env.vault();
    let mut app = env.start(env.launch(Some(vault.clone()), Config::default()));
    assert!(app.relative_numbers(), "on by default");
    command(&mut app, "set nornu");
    assert!(!app.relative_numbers());
    command(&mut app, "set rnu?");
    assert_eq!(app.message(), Some((Level::Info, "norelativenumber")));
    command(&mut app, "set rnu!");
    assert!(app.relative_numbers());
    command(&mut app, "set norelativenumber");
    command(&mut app, "set nowrap");
    assert_eq!(
        app.message(),
        Some((Level::Error, "Unknown option: nowrap"))
    );

    // Remembered for this vault.
    let app = env.start(env.launch(Some(vault.clone()), Config::default()));
    assert!(!app.relative_numbers());

    // The configuration sets the default for a vault that never used `:set`.
    let other = Env::new();
    let config = Config {
        relative_numbers: Some(false),
        ..Config::default()
    };
    let app = other.start(other.launch(Some(other.vault()), config));
    assert!(!app.relative_numbers());
}

#[test]
fn f05_clicking_past_the_end_of_a_line_in_normal_mode_lands_on_its_last_character() {
    let env = Env::new();
    let mut app = open(&env, "Inbox/Ide cepat.md");
    keys(&mut app, "ggOabc<Esc>");
    send(&mut app, Message::Click(0, 3, false));
    assert_eq!(app.active_cursor(), Some((0, 2)));
    keys(&mut app, "x");
    assert!(app.active_text().unwrap().starts_with("ab\n"));
    // In insert mode the cursor may sit after the last character.
    keys(&mut app, "A");
    send(&mut app, Message::Click(0, 2, false));
    assert_eq!(app.active_cursor(), Some((0, 2)));
}

#[test]
fn f06_a_click_in_insert_mode_does_not_touch_the_line_clicked() {
    let env = Env::new();
    let mut app = open(&env, "Inbox/Ide cepat.md");
    keys(&mut app, "ggO  a b<Esc>");
    // `o` opens an auto-indented line; nothing is typed on it.
    keys(&mut app, "o");
    send(&mut app, Message::Click(0, 3, false));
    keys(&mut app, "<Esc>");
    let text = app.active_text().unwrap();
    assert!(text.starts_with("  a b\n"), "{text:?}");
    // The empty line `o` opened loses its indent, as with an arrow key.
    assert!(text.starts_with("  a b\n\n# Ide cepat"), "{text:?}");
}

#[test]
fn f05_8_a_read_only_note_does_not_stay_in_insert_mode() {
    let env = Env::new();
    let vault = env.vault();
    std::fs::write(vault.join("latin1.md"), b"caf\xE9 [[Rap\n").unwrap();
    let config = Config {
        insert_escape: Some("jk".into()),
        ..Config::default()
    };
    let mut app = env.start(env.launch(Some(vault), config));
    send(
        &mut app,
        Message::Remote(Request::Open {
            path: "latin1.md".into(),
        }),
    );
    let text = app.active_text().unwrap();
    // A held `j` (part of `jk`) would be typed when its time runs out.
    keys(&mut app, "$ij");
    wait(&mut app, Duration::from_millis(1500));
    assert_eq!(app.active_text().unwrap(), text);
    assert_eq!(app.active_dirty(), Some(false));
    assert!(
        !app.suggestions_open(),
        "no link suggestions on a read-only note"
    );
}
