//! Helpers shared by the app tests: a temporary environment with a copy of
//! the mockup vault, and a driver that runs tasks like the Iced runtime.

#![allow(dead_code)]

use std::path::{Path, PathBuf};

use iced::Task;
use iced::keyboard::{self, Key, Location, Modifiers, key};
use iced_runtime::futures::futures::{StreamExt, executor::block_on};
use patinote_app::app::{App, Launch, Message};
use patinote_core::config::Config;

pub struct Env {
    pub dir: tempfile::TempDir,
}

impl Default for Env {
    fn default() -> Self {
        Self::new()
    }
}

impl Env {
    pub fn new() -> Self {
        Self {
            dir: tempfile::tempdir().unwrap(),
        }
    }

    pub fn path(&self, rel: &str) -> PathBuf {
        self.dir.path().join(rel)
    }

    /// A copy of the mockup vault; the fixture itself is never written.
    pub fn vault(&self) -> PathBuf {
        let dest = self.path("Vault");
        copy_dir(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/vault-mockup"),
            &dest,
        );
        dest
    }

    pub fn launch(&self, vault: Option<PathBuf>, config: Config) -> Launch {
        Launch {
            vault,
            note: None,
            config,
            cache_home: self.path("cache"),
            state_home: self.path("state"),
            runtime_dir: None,
            watch: false,
            bench: None,
        }
    }

    /// Starts the app and runs what it starts with.
    pub fn start(&self, launch: Launch) -> App {
        let (mut app, task) = App::new(launch);
        run(&mut app, task);
        app
    }
}

pub fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), &target).unwrap();
        }
    }
}

/// Runs a task and every task its messages lead to.
pub fn run(app: &mut App, task: Task<Message>) {
    let mut queue = vec![task];
    while let Some(task) = queue.pop() {
        let Some(stream) = iced_runtime::task::into_stream(task) else {
            continue;
        };
        let mut stream = stream;
        // Actions are taken one at a time: window and widget actions are
        // dropped right away, which answers any that wait for a reply.
        while let Some(action) = block_on(stream.next()) {
            if let iced_runtime::Action::Output(message) = action {
                queue.push(app.update(message));
            }
        }
    }
}

/// Runs a task and returns its messages without handling them, so a test
/// can change the disk before the app sees what the task did.
pub fn outputs(task: Task<Message>) -> Vec<Message> {
    let Some(mut stream) = iced_runtime::task::into_stream(task) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    while let Some(action) = block_on(stream.next()) {
        if let iced_runtime::Action::Output(message) = action {
            out.push(message);
        }
    }
    out
}

/// Like [`run`], for tasks that never end (the watcher, the socket): every
/// stream is polled in turn until `done` holds, or for at most `limit`.
/// Returns whether `done` held.
pub fn run_until(
    app: &mut App,
    task: Task<Message>,
    limit: std::time::Duration,
    done: impl Fn(&App) -> bool,
) -> bool {
    use iced_runtime::futures::futures::future::{Either, select};
    use iced_runtime::futures::futures::stream::SelectAll;
    let deadline = std::time::Instant::now() + limit;
    let mut streams = SelectAll::new();
    if let Some(s) = iced_runtime::task::into_stream(task) {
        streams.push(s);
    }
    while !done(app) {
        let left = deadline.saturating_duration_since(std::time::Instant::now());
        if left.is_zero() || streams.is_empty() {
            return false;
        }
        let (tx, rx) = iced_runtime::futures::futures::channel::oneshot::channel::<()>();
        std::thread::spawn(move || {
            std::thread::sleep(left);
            let _ = tx.send(());
        });
        match block_on(select(streams.next(), rx)) {
            Either::Left((Some(iced_runtime::Action::Output(message)), _)) => {
                if let Some(s) = iced_runtime::task::into_stream(app.update(message)) {
                    streams.push(s);
                }
            }
            Either::Left((Some(_), _)) => {}
            Either::Left((None, _)) => return done(app),
            Either::Right(_) => return done(app),
        }
    }
    true
}

pub fn send(app: &mut App, message: Message) {
    let task = app.update(message);
    run(app, task);
}

pub fn press(app: &mut App, ch: &str) {
    let task = press_held(app, ch);
    run(app, task);
}

/// Like [`press`], but returns the key's task instead of running it, to
/// let its answer arrive late.
pub fn press_held(app: &mut App, ch: &str) -> Task<Message> {
    let key = match ch {
        "Enter" => Key::Named(key::Named::Enter),
        "Escape" => Key::Named(key::Named::Escape),
        "BackSpace" => Key::Named(key::Named::Backspace),
        c => Key::Character(c.into()),
    };
    let text = (ch.chars().count() == 1).then(|| ch.into());
    app.update(Message::Key(
        keyboard::Event::KeyPressed {
            key: key.clone(),
            modified_key: key.clone(),
            physical_key: key::Physical::Unidentified(key::NativeCode::Unidentified),
            location: Location::Standard,
            modifiers: Modifiers::default(),
            text,
            repeat: false,
        },
        false,
    ))
}

pub fn command(app: &mut App, line: &str) {
    press(app, ":");
    assert!(app.typing_command());
    send(app, Message::CmdInput(line.into()));
    send(app, Message::CmdSubmit);
}

/// Types each character of `keys` as its own key press; `<Esc>` and `<CR>`
/// stand for Escape and Enter.
pub fn keys(app: &mut App, keys: &str) {
    let mut rest = keys;
    while !rest.is_empty() {
        if let Some(r) = rest.strip_prefix("<Esc>") {
            press(app, "Escape");
            rest = r;
        } else if let Some(r) = rest.strip_prefix("<CR>") {
            press(app, "Enter");
            rest = r;
        } else {
            let c = rest.chars().next().unwrap();
            press(app, &c.to_string());
            rest = &rest[c.len_utf8()..];
        }
    }
}

/// Runs the autosave timer as if `after` had passed.
pub fn wait(app: &mut App, after: std::time::Duration) {
    send(app, Message::Tick(std::time::Instant::now() + after));
}

/// Presses a key with modifiers held; `captured` as if a widget handled it.
pub fn press_with(
    app: &mut App,
    key: Key,
    modifiers: Modifiers,
    text: Option<&str>,
    captured: bool,
) {
    send(
        app,
        Message::Key(keyboard::Event::ModifiersChanged(modifiers), false),
    );
    send(
        app,
        Message::Key(
            keyboard::Event::KeyPressed {
                key: key.clone(),
                modified_key: key,
                physical_key: key::Physical::Unidentified(key::NativeCode::Unidentified),
                location: Location::Standard,
                modifiers,
                text: text.map(Into::into),
                repeat: false,
            },
            captured,
        ),
    );
    send(
        app,
        Message::Key(
            keyboard::Event::ModifiersChanged(Modifiers::default()),
            false,
        ),
    );
}

/// Presses the leader key (Space) and then `next`.
pub fn leader(app: &mut App, next: &str) {
    press_with(
        app,
        Key::Named(key::Named::Space),
        Modifiers::default(),
        Some(" "),
        false,
    );
    press(app, next);
}
