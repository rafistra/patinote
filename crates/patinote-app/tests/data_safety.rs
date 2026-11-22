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

// --- Changes from outside (F-11, F-27) ------------------------------------

/// What the watcher would report for the note.
fn changed_outside(app: &mut App, rel: &str) {
    send(
        app,
        Message::External(patinote_core::Change {
            paths: vec![rel.to_owned()],
            rescan: false,
            mode: patinote_core::WatchMode::Events,
        }),
    );
}

#[test]
fn our_own_autosave_is_not_taken_for_a_change_from_outside() {
    let env = Env::new();
    let mut app = open(&env);
    keys(&mut app, "ggOsembilan<Esc>");
    let save = start_autosave(&mut app);
    // The watcher reports our write before the save has landed.
    let ours = app.active_text().unwrap().into_bytes();
    send(&mut app, Message::DiskState(NOTE.into(), Some(ours)));
    keys(&mut app, "ggOsepuluh<Esc>");
    run(&mut app, save);
    assert_eq!(app.external_title(), None, "no banner for our own save");
    command(&mut app, "w");
    assert!(disk(&env, NOTE).starts_with("sepuluh\nsembilan\n"));
}

#[test]
fn reading_mode_shows_a_note_updated_from_outside() {
    let env = Env::new();
    let mut app = open(&env);
    leader(&mut app, "r");
    assert!(app.reading());
    std::fs::write(env.path("Vault").join(NOTE), "Lihat [[Catatan baru]]\n").unwrap();
    changed_outside(&mut app, NOTE);
    assert_eq!(app.reading_targets(), vec!["Catatan baru".to_owned()]);
}

#[test]
fn use_disk_version_keeps_the_disk_format_and_saves_without_a_conflict() {
    let env = Env::new();
    let mut app = open(&env);
    keys(&mut app, "ggOmilikku<Esc>");
    std::fs::write(env.path("Vault").join(NOTE), "satu\r\ndua\r\n").unwrap();
    changed_outside(&mut app, NOTE);
    assert!(app.external_title().is_some());
    send(
        &mut app,
        Message::Choose(patinote_app::app::Choice::UseDisk),
    );
    keys(&mut app, "ggObaru<Esc>");
    command(&mut app, "w");
    assert_eq!(app.external_title(), None, "no false conflict");
    assert_eq!(disk(&env, NOTE), "baru\r\nsatu\r\ndua\r\n");
}

#[test]
fn use_copy_does_not_overwrite_an_original_changed_since_the_comparison() {
    let env = Env::new();
    let mut app = env.start(env.launch(Some(env.vault()), Config::default()));
    let original = "Rapat/Rapat 19 Nov.md";
    let copy = "Rapat/Rapat 19 Nov.sync-conflict-20261119-162104-ABCDEFG.md";
    command(&mut app, "conflicts");
    let i = app.conflict_index(original).unwrap();
    send(&mut app, Message::OpenConflict(i));
    assert!(app.compare_summary().is_some());
    // Syncthing brings yet another version while the comparison is open.
    std::fs::write(env.path("Vault").join(original), "versi terbaru\n").unwrap();
    send(
        &mut app,
        Message::Choose(patinote_app::app::Choice::UseCopy),
    );
    assert_eq!(disk(&env, original), "versi terbaru\n");
    assert!(env.path("Vault").join(copy).exists(), "the copy is kept");
    assert_eq!(app.message().map(|(l, _)| l), Some(Level::Error));
}

#[test]
fn notes_from_the_last_session_load_after_interrupted_writes_are_undone() {
    use patinote_app::session::VaultPaths;
    use patinote_core::write::{Actor, Journal, Op, Transaction, commit, expect_current};
    let env = Env::new();
    let vault = env.vault();
    let before = disk(&env, NOTE);
    {
        // The last session had the note open.
        let mut app = env.start(env.launch(Some(vault.clone()), Config::default()));
        send(
            &mut app,
            Message::Remote(Request::Open { path: NOTE.into() }),
        );
    }
    // A write that crashed half-way: the new bytes are on disk, the
    // journal entry still says pending.
    let paths = VaultPaths::resolve(&vault, &env.path("cache"), &env.path("state")).unwrap();
    let journal = Journal::for_vault(&paths.state_home, &paths.id);
    let done = commit(
        &vault,
        &journal,
        &Transaction {
            actor: Actor::User,
            ops: vec![Op::Write {
                path: NOTE.into(),
                content: b"setengah jadi\n".to_vec(),
                expect: expect_current(&vault, NOTE).unwrap(),
            }],
        },
    )
    .unwrap();
    let manifest = journal.dir().join(&done.id).join("manifest.json");
    let text = std::fs::read_to_string(&manifest)
        .unwrap()
        .replace("\"committed\"", "\"pending\"");
    std::fs::write(&manifest, text).unwrap();

    let app = env.start(env.launch(Some(vault), Config::default()));
    assert_eq!(disk(&env, NOTE), before, "recovered");
    assert_eq!(app.buffer_text(NOTE).as_deref(), Some(before.as_str()));
}

#[test]
fn keep_original_is_refused_when_the_original_is_gone() {
    let env = Env::new();
    let original = "Rapat/Rapat 19 Nov.md";
    let copy = "Rapat/Rapat 19 Nov.sync-conflict-20261119-162104-ABCDEFG.md";
    // Deleted on another device: only Syncthing's copy is left.
    let vault = env.vault();
    std::fs::remove_file(vault.join(original)).unwrap();
    let mut app = env.start(env.launch(Some(vault), Config::default()));
    command(&mut app, "conflicts");
    let i = app.conflict_index(original).expect("listed");
    send(&mut app, Message::OpenConflict(i));
    assert!(app.compare_summary().is_some());
    send(
        &mut app,
        Message::Choose(patinote_app::app::Choice::KeepOriginal),
    );
    assert!(
        env.path("Vault").join(copy).exists(),
        "the only version stays"
    );
    assert_eq!(app.message().map(|(l, _)| l), Some(Level::Error));
}

#[test]
fn a_change_from_outside_while_a_save_runs_is_read_once_it_lands() {
    let env = Env::new();
    let mut app = open(&env);
    keys(&mut app, "ggOsebelas<Esc>");
    let save = start_autosave(&mut app);
    // The watcher reports a change while the save is still running.
    send(
        &mut app,
        Message::DiskState(NOTE.into(), Some(b"dari luar\n".to_vec())),
    );
    let landed = common::outputs(save);
    // Syncthing's write landed right after ours.
    std::fs::write(env.path("Vault").join(NOTE), "dari luar\n").unwrap();
    for m in landed {
        send(&mut app, m);
    }
    assert_eq!(app.active_text().as_deref(), Some("dari luar\n"));
}

#[test]
fn quitting_goes_on_when_the_disk_changed_during_the_last_save() {
    let env = Env::new();
    let mut app = open(&env);
    keys(&mut app, "ggOduabelas<Esc>");
    let quit = command_held(&mut app, "wqa");
    // A watcher event (our own earlier write, say) while the save runs.
    send(
        &mut app,
        Message::DiskState(NOTE.into(), Some(b"lain\n".to_vec())),
    );
    run(&mut app, quit);
    assert!(disk(&env, NOTE).starts_with("duabelas\n"));
    assert!(app.exited(), "the app quit once the save landed");
}

#[test]
fn a_note_opened_at_startup_waits_for_interrupted_writes_to_be_undone() {
    use patinote_app::session::VaultPaths;
    use patinote_core::write::{Actor, Journal, Op, Transaction, commit, expect_current};
    let env = Env::new();
    let vault = env.vault();
    let before = disk(&env, NOTE);
    let paths = VaultPaths::resolve(&vault, &env.path("cache"), &env.path("state")).unwrap();
    let journal = Journal::for_vault(&paths.state_home, &paths.id);
    let done = commit(
        &vault,
        &journal,
        &Transaction {
            actor: Actor::User,
            ops: vec![Op::Write {
                path: NOTE.into(),
                content: b"setengah jadi\n".to_vec(),
                expect: expect_current(&vault, NOTE).unwrap(),
            }],
        },
    )
    .unwrap();
    let manifest = journal.dir().join(&done.id).join("manifest.json");
    let text = std::fs::read_to_string(&manifest)
        .unwrap()
        .replace("\"committed\"", "\"pending\"");
    std::fs::write(&manifest, text).unwrap();

    // A request arrives before the startup tasks (recovery) have run.
    let (mut app, startup) = App::new(env.launch(Some(vault), Config::default()));
    send(
        &mut app,
        Message::Remote(Request::Open { path: NOTE.into() }),
    );
    run(&mut app, startup);
    assert_eq!(disk(&env, NOTE), before, "recovered");
    assert_eq!(app.buffer_text(NOTE).as_deref(), Some(before.as_str()));
}

#[test]
fn a_damaged_journal_entry_is_reported_on_screen() {
    use patinote_app::session::VaultPaths;
    use patinote_core::write::{Actor, Journal, Op, Transaction, commit, expect_current};
    let env = Env::new();
    let vault = env.vault();
    let paths = VaultPaths::resolve(&vault, &env.path("cache"), &env.path("state")).unwrap();
    let journal = Journal::for_vault(&paths.state_home, &paths.id);
    let done = commit(
        &vault,
        &journal,
        &Transaction {
            actor: Actor::User,
            ops: vec![Op::Write {
                path: NOTE.into(),
                content: b"baru\n".to_vec(),
                expect: expect_current(&vault, NOTE).unwrap(),
            }],
        },
    )
    .unwrap();
    std::fs::write(journal.dir().join(&done.id).join("manifest.json"), "{rusak").unwrap();
    let app = env.start(env.launch(Some(vault), Config::default()));
    let (level, text) = app.message().expect("a message");
    assert_eq!(level, Level::Error);
    assert!(
        text.starts_with("Can't undo an interrupted write"),
        "{text}"
    );
}

#[test]
fn a_closed_note_whose_save_fails_comes_back_without_taking_the_keys() {
    let env = Env::new();
    let mut app = env.start(env.launch(Some(env.vault()), Config::default()));
    send(
        &mut app,
        Message::Remote(Request::Open {
            path: "Proyek/Rapat.md".into(),
        }),
    );
    send(
        &mut app,
        Message::Remote(Request::Open { path: NOTE.into() }),
    );
    keys(&mut app, "ggOtigabelas<Esc>");
    // The save of the closed note will fail: the file changed outside.
    std::fs::write(env.path("Vault").join(NOTE), "dari luar\n").unwrap();
    let close = command_held(&mut app, "q");
    assert_eq!(app.title(), "Rapat — Patinote");
    keys(&mut app, "i");
    run(&mut app, close);
    assert_eq!(
        app.title(),
        "Rapat — Patinote",
        "the note being typed in stays"
    );
    assert!(
        titles(&app).iter().any(|t| t == "Ide cepat"),
        "the unsaved note is back"
    );
    assert_eq!(app.message().map(|(l, _)| l), Some(Level::Error));
    assert!(app.buffer_text(NOTE).unwrap().starts_with("tigabelas\n"));
}

#[test]
fn a_key_held_for_insert_escape_is_typed_when_the_note_is_left() {
    let env = Env::new();
    let config = Config {
        insert_escape: Some("jk".into()),
        ..Config::default()
    };
    let mut app = env.start(env.launch(Some(env.vault()), config));
    send(
        &mut app,
        Message::Remote(Request::Open { path: NOTE.into() }),
    );
    // `j` waits to see whether `k` follows; another note opens meanwhile.
    keys(&mut app, "ggOfooj");
    send(
        &mut app,
        Message::Remote(Request::Open {
            path: "Proyek/Rapat.md".into(),
        }),
    );
    let text = app.buffer_text(NOTE).unwrap();
    assert!(text.starts_with("fooj\n"), "{text:?}");
    command(&mut app, "qa");
    assert!(
        disk(&env, NOTE).starts_with("fooj\n"),
        "{:?}",
        disk(&env, NOTE)
    );
}

#[test]
fn a_conflict_during_quit_stops_the_quit_for_good() {
    let env = Env::new();
    let mut app = open(&env);
    keys(&mut app, "ggOempatbelas<Esc>");
    // The file changes outside while the quit's save runs.
    std::fs::write(env.path("Vault").join(NOTE), "dari luar\n").unwrap();
    command(&mut app, "qa");
    assert!(app.external_title().is_some(), "the conflict banner is up");
    assert!(!app.quitting(), "quitting is not left pending");
    assert_eq!(app.message().map(|(l, _)| l), Some(Level::Error));
    // Resolving it later does not quit on its own.
    send(
        &mut app,
        Message::Choose(patinote_app::app::Choice::KeepMine),
    );
    assert!(!app.exited());
    assert!(disk(&env, NOTE).starts_with("empatbelas\n"));
}

#[test]
fn a_late_recovery_from_the_previous_vault_does_not_load_the_new_one_early() {
    use patinote_app::session::VaultPaths;
    use patinote_core::write::{Actor, Journal, Op, Transaction, commit, expect_current};
    let env = Env::new();
    let vault = env.vault();
    let before = disk(&env, NOTE);
    {
        // The last session of this vault had the note open.
        let mut app = env.start(env.launch(Some(vault.clone()), Config::default()));
        send(
            &mut app,
            Message::Remote(Request::Open { path: NOTE.into() }),
        );
    }
    let paths = VaultPaths::resolve(&vault, &env.path("cache"), &env.path("state")).unwrap();
    let journal = Journal::for_vault(&paths.state_home, &paths.id);
    let done = commit(
        &vault,
        &journal,
        &Transaction {
            actor: Actor::User,
            ops: vec![Op::Write {
                path: NOTE.into(),
                content: b"setengah jadi\n".to_vec(),
                expect: expect_current(&vault, NOTE).unwrap(),
            }],
        },
    )
    .unwrap();
    let manifest = journal.dir().join(&done.id).join("manifest.json");
    let text = std::fs::read_to_string(&manifest)
        .unwrap()
        .replace("\"committed\"", "\"pending\"");
    std::fs::write(&manifest, text).unwrap();

    // Another vault is open first; its events are still on their way.
    let other = env.path("Lain");
    common::copy_dir(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/vault-mockup"),
        &other,
    );
    let (mut app, startup) = App::new(env.launch(Some(other), Config::default()));
    let late = common::outputs(startup);
    // This vault's recovery is held up (another write holds the lock).
    let lock = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(journal.dir().join(".lock"))
        .unwrap();
    lock.lock().unwrap();
    let opening = app.update(Message::FolderChosen(Some(vault)));
    // Its "writes recovered" arrives now. Only that one, and what it reads
    // is collected while the lock is held (anything asking the index would
    // wait on the lock too).
    let mut read = Vec::new();
    for m in late {
        if format!("{m:?}").contains("Recovered") {
            read.extend(common::outputs(app.update(m)));
        }
    }
    lock.unlock().unwrap();
    for m in read {
        send(&mut app, m);
    }
    common::run(&mut app, opening);
    assert_eq!(disk(&env, NOTE), before, "recovered");
    assert_eq!(app.buffer_text(NOTE).as_deref(), Some(before.as_str()));
}

#[test]
fn a_save_still_running_when_its_note_is_renamed_lands_at_the_new_path() {
    let env = Env::new();
    let mut app = open(&env);
    keys(&mut app, "ggOsembilan<Esc>");
    let save = start_autosave(&mut app);
    explorer_key(&mut app, NOTE, "r");
    send(&mut app, Message::DialogInput("Ide baru".into()));
    send(&mut app, Message::DialogSubmit);
    // The rename waits for the save that runs.
    assert!(!env.path("Vault/Inbox/Ide baru.md").exists(), "waits");
    run(&mut app, save);
    common::wait(&mut app, Duration::from_secs(2));
    assert!(disk(&env, "Inbox/Ide baru.md").starts_with("sembilan\n"));
    assert!(!env.path("Vault").join(NOTE).exists(), "not written back");
    assert_eq!(
        app.external_title(),
        None,
        "no banner: nothing changed outside"
    );
    common::press_with(
        &mut app,
        iced::keyboard::Key::Character("w".into()),
        iced::keyboard::Modifiers::CTRL,
        None,
        false,
    );
    press(&mut app, "l");
    command(&mut app, "qa");
    assert!(app.exited());
}

#[test]
fn a_new_note_typed_into_and_emptied_is_not_written_and_does_not_block_quitting() {
    for quit in ["q", "qa"] {
        let env = Env::new();
        let mut app = open(&env);
        keys(&mut app, "ggO[[Belum ada<Esc><Esc>0lgd");
        assert_eq!(app.active_text().as_deref(), Some(""));
        keys(&mut app, "ia");
        press(&mut app, "BackSpace");
        keys(&mut app, "<Esc>");
        command(&mut app, quit);
        wait_saves(&mut app);
        assert!(!env.path("Vault/Belum ada.md").exists(), "{quit}");
        if quit == "qa" {
            assert!(app.exited(), "quit");
        }
    }
}

fn wait_saves(app: &mut App) {
    common::wait(app, Duration::from_secs(2));
}

#[test]
fn quitting_goes_on_when_a_save_overtaken_by_a_rename_lands() {
    let env = Env::new();
    let mut app = open(&env);
    keys(&mut app, "ggOsepuluh<Esc>");
    let save = start_autosave(&mut app);
    explorer_key(&mut app, NOTE, "r");
    send(&mut app, Message::DialogInput("Ide baru".into()));
    send(&mut app, Message::DialogSubmit);
    common::press_with(
        &mut app,
        iced::keyboard::Key::Character("w".into()),
        iced::keyboard::Modifiers::CTRL,
        None,
        false,
    );
    press(&mut app, "l");
    let quit = command_held(&mut app, "qa");
    run(&mut app, quit);
    assert!(!app.exited(), "waits for the save");
    // The save started before the rename lands: it is redone at the new
    // path at once, and the quit goes on without waiting for a timer.
    run(&mut app, save);
    assert!(disk(&env, "Inbox/Ide baru.md").starts_with("sepuluh\n"));
    assert!(app.exited());
}

#[test]
fn a_closed_note_whose_save_runs_when_it_is_renamed_is_saved_at_the_new_path() {
    let env = Env::new();
    let mut app = open(&env);
    keys(&mut app, "ggOsebelas<Esc>");
    let save = start_autosave(&mut app);
    // Closed while its save runs: it waits in `closing`.
    command(&mut app, "q");
    explorer_key(&mut app, NOTE, "r");
    send(&mut app, Message::DialogInput("Ide baru".into()));
    send(&mut app, Message::DialogSubmit);
    run(&mut app, save);
    common::wait(&mut app, Duration::from_secs(2));
    assert_eq!(app.message().map(|(l, _)| l), None, "{:?}", app.message());
    assert!(disk(&env, "Inbox/Ide baru.md").starts_with("sebelas\n"));
    assert!(!env.path("Vault").join(NOTE).exists());
}

/// Changed outside, then renamed while a save runs: the save is refused and
/// the banner shows (F-11); the rename, which waited for the save, is then
/// refused too, as the note has text that is not on disk.
#[test]
fn f11_3_a_change_from_outside_before_a_rename_still_shows_the_banner() {
    let env = Env::new();
    let mut app = open(&env);
    keys(&mut app, "ggOempatbelas<Esc>");
    let save = start_autosave(&mut app);
    std::fs::write(env.path("Vault").join(NOTE), "dari luar\n").unwrap();
    explorer_key(&mut app, NOTE, "r");
    send(&mut app, Message::DialogInput("Ide baru".into()));
    send(&mut app, Message::DialogSubmit);
    run(&mut app, save);
    assert_eq!(disk(&env, NOTE), "dari luar\n", "not overwritten");
    assert!(app.external_title().is_some(), "the banner is up");
    assert!(!env.path("Vault/Inbox/Ide baru.md").exists(), "not renamed");
}

/// A new note's first save still runs when its folder is moved: the move
/// waits for it, so the note lands in the folder and moves with it, and no
/// old folder is made again.
#[test]
fn f05_2_a_folder_move_waits_for_a_new_notes_first_save() {
    let env = Env::new();
    let mut app = open(&env);
    explorer_key(&mut app, "Inbox", "a");
    send(&mut app, Message::InlineInput("Baru".into()));
    send(&mut app, Message::InlineSubmit);
    keys(&mut app, "limabelas<Esc>");
    let save = start_autosave(&mut app);
    explorer_key(&mut app, "Inbox", "r");
    send(&mut app, Message::InlineInput("Kotak".into()));
    send(&mut app, Message::InlineSubmit);
    assert!(!env.path("Vault/Kotak").exists(), "waits for the save");
    run(&mut app, save);
    common::wait(&mut app, Duration::from_secs(2));
    assert_eq!(disk(&env, "Kotak/Baru.md"), "limabelas");
    assert!(!env.path("Vault/Inbox").exists(), "no old folder left");
    assert_eq!(app.message().map(|(l, _)| l), None, "{:?}", app.message());
}

/// A new note closed while its first save runs, then made again at once:
/// the one waiting to be saved comes back, rather than a second, empty one.
#[test]
fn f08_6_a_new_note_made_again_while_its_first_save_runs_is_the_same_note() {
    let env = Env::new();
    let mut app = open(&env);
    keys(&mut app, "ggO[[Belum ada<Esc><Esc>0lgd");
    keys(&mut app, "ienambelas<Esc>");
    let save = start_autosave(&mut app);
    command(&mut app, "q");
    assert_eq!(app.title(), "Ide cepat — Patinote");
    keys(&mut app, "gg0lgd");
    assert_eq!(app.active_text().as_deref(), Some("enambelas"));
    run(&mut app, save);
    common::wait(&mut app, Duration::from_secs(2));
    assert_eq!(disk(&env, "Belum ada.md"), "enambelas");
    assert_eq!(app.external_title(), None, "no conflict");
}

/// Made again from the explorer (which types into a new note at once), the
/// note that was waiting for its save comes back in insert mode too.
#[test]
fn f10_1_a_new_note_made_again_while_saving_opens_in_insert_mode() {
    let env = Env::new();
    let mut app = open(&env);
    explorer_key(&mut app, "Inbox", "a");
    send(&mut app, Message::InlineInput("Baru".into()));
    send(&mut app, Message::InlineSubmit);
    keys(&mut app, "satu<Esc>");
    let save = start_autosave(&mut app);
    command(&mut app, "q");
    explorer_key(&mut app, "Inbox", "a");
    send(&mut app, Message::InlineInput("Baru".into()));
    send(&mut app, Message::InlineSubmit);
    keys(&mut app, "dua<Esc>");
    run(&mut app, save);
    common::wait(&mut app, Duration::from_secs(2));
    // `i` at the cursor, which Esc left on the last letter.
    assert_eq!(disk(&env, "Inbox/Baru.md"), "satduau");
}

// --- Saves and vault changes never overlap ----------------------------------

/// Renames the note at `path` through the dialog, without running what it
/// starts.
fn rename_held(app: &mut App, path: &str, name: &str) -> Task<Message> {
    explorer_key(app, path, "r");
    send(app, Message::DialogInput(name.into()));
    app.update(Message::DialogSubmit)
}

/// Back from the explorer to the editor.
fn to_editor(app: &mut App) {
    common::press_with(
        app,
        iced::keyboard::Key::Character("w".into()),
        iced::keyboard::Modifiers::CTRL,
        None,
        false,
    );
    press(app, "l");
}

/// `:q` while a rename runs: the note's save waits for the rename and
/// lands at the new path, with no conflict.
#[test]
fn f10_6_closing_a_note_while_it_is_renamed_saves_it_at_the_new_path() {
    let env = Env::new();
    let mut app = open(&env);
    keys(&mut app, "ggOtujuhbelas<Esc>");
    let rename = rename_held(&mut app, NOTE, "Ide baru");
    to_editor(&mut app);
    keys(&mut app, "ggOdelapanbelas<Esc>");
    let close = command_held(&mut app, "q");
    run(&mut app, close);
    run(&mut app, rename);
    common::wait(&mut app, Duration::from_secs(2));
    let text = disk(&env, "Inbox/Ide baru.md");
    assert!(text.starts_with("delapanbelas\ntujuhbelas\n"), "{text:?}");
    assert!(!env.path("Vault").join(NOTE).exists());
    assert_eq!(app.message().map(|(l, _)| l), None, "{:?}", app.message());
}

/// Two renames asked for while a save runs both happen, in order.
#[test]
fn f10_6_two_renames_while_a_save_runs_both_happen() {
    let env = Env::new();
    let mut app = open(&env);
    keys(&mut app, "ggOsembilanbelas<Esc>");
    let save = start_autosave(&mut app);
    explorer_key(&mut app, NOTE, "r");
    send(&mut app, Message::DialogInput("Ide baru".into()));
    send(&mut app, Message::DialogSubmit);
    explorer_key(&mut app, "Referensi/Daftar bacaan.md", "r");
    send(&mut app, Message::DialogInput("Bacaan".into()));
    send(&mut app, Message::DialogSubmit);
    run(&mut app, save);
    assert!(disk(&env, "Inbox/Ide baru.md").starts_with("sembilanbelas\n"));
    assert!(env.path("Vault/Referensi/Bacaan.md").exists());
    assert!(!env.path("Vault/Referensi/Daftar bacaan.md").exists());
}

/// A folder move that waited lists the folder when it runs: a note saved
/// there meanwhile moves with its links updated.
#[test]
fn f10_7_a_folder_move_that_waited_includes_the_note_saved_meanwhile() {
    let env = Env::new();
    let vault = env.vault();
    std::fs::write(vault.join("Tautan.md"), "[b](Inbox/Baru.md)\n").unwrap();
    let mut app = env.start(env.launch(Some(vault), Config::default()));
    explorer_key(&mut app, "Inbox", "a");
    send(&mut app, Message::InlineInput("Baru".into()));
    send(&mut app, Message::InlineSubmit);
    keys(&mut app, "duapuluh<Esc>");
    let save = start_autosave(&mut app);
    explorer_key(&mut app, "Inbox", "r");
    send(&mut app, Message::InlineInput("Kotak".into()));
    send(&mut app, Message::InlineSubmit);
    run(&mut app, save);
    assert_eq!(disk(&env, "Kotak/Baru.md"), "duapuluh");
    assert_eq!(disk(&env, "Tautan.md"), "[b](Kotak/Baru.md)\n");
}

/// Deleting a folder while a new note in it has its first save running:
/// the delete waits, the note goes to the trash with the folder, and the
/// folder does not come back.
#[test]
fn f10_5_deleting_a_folder_waits_for_a_new_notes_first_save() {
    let env = Env::new();
    let mut app = open(&env);
    explorer_key(&mut app, "Inbox", "a");
    send(&mut app, Message::InlineInput("Baru".into()));
    send(&mut app, Message::InlineSubmit);
    keys(&mut app, "duasatu<Esc>");
    let save = start_autosave(&mut app);
    explorer_key(&mut app, "Inbox", "d");
    press(&mut app, "Enter");
    run(&mut app, save);
    common::wait(&mut app, Duration::from_secs(2));
    assert!(!env.path("Vault/Inbox").exists(), "the folder stays gone");
    assert!(env.path("Vault/.trash/Inbox/Baru.md").exists());
}

/// A rename waits for a closed note's save; the note is made again at the
/// old path meanwhile. The same note comes back (with its text), and the
/// rename then moves it with everything typed.
#[test]
fn f05_2_a_note_made_again_while_its_rename_waits_moves_with_its_text() {
    let env = Env::new();
    let mut app = open(&env);
    keys(&mut app, "ggOduadua<Esc>");
    let save = start_autosave(&mut app);
    command(&mut app, "q");
    explorer_key(&mut app, NOTE, "r");
    send(&mut app, Message::DialogInput("Ide baru".into()));
    send(&mut app, Message::DialogSubmit);
    explorer_key(&mut app, "Inbox", "a");
    send(&mut app, Message::InlineInput("Ide cepat".into()));
    send(&mut app, Message::InlineSubmit);
    assert!(
        app.active_text().unwrap().starts_with("duadua\n"),
        "the same note"
    );
    keys(&mut app, "<Esc>ggOduatiga<Esc>");
    run(&mut app, save);
    common::wait(&mut app, Duration::from_secs(2));
    let text = disk(&env, "Inbox/Ide baru.md");
    assert!(text.starts_with("duatiga\nduadua\n"), "{text:?}");
    assert!(!env.path("Vault").join(NOTE).exists());
}
