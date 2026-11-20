//! The app shell (step 2.2): which screen shows, the bufferline, status line
//! and command line, the vault session, and what is remembered between
//! sessions. Logic lives in [`App::update`] so tests can drive it without a
//! window.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use iced::keyboard::{self, Key, key::Named};
use iced::widget::{column, container, text_input};
use iced::{Element, Event, Length, Padding, Size, Subscription, Task, event, window};
use patinote_core::config::{Config, Layout};
use patinote_core::ipc::{self, Request, Response};
use patinote_core::names::stem;

use crate::mode::Mode;
use crate::screens::start::{RECENT_SHOWN, RecentRow, Start, relative_time};
use crate::screens::{note, welcome};
use crate::session::{IndexEvent, Lookup, Session, VaultPaths, lookup};
use crate::state::{self, AppState, VaultState};
use crate::theme::styles::{self, Level};
use crate::theme::tokens::*;
use crate::widgets::bufferline::{self, Tab};
use crate::widgets::cmdline::{self, ERROR_MIN_SECONDS};
use crate::widgets::statusline::{Indicator, StatusButton, StatusLine};
use crate::widgets::typed;

/// The id of the command line input.
const CMDLINE: &str = "cmdline";

/// What the app was started with.
#[derive(Debug, Clone, Default)]
pub struct Launch {
    /// The vault to open: `--vault`, `$PATINOTE_VAULT`, the last vault, then
    /// `[vault] default`, in that order (F-01).
    pub vault: Option<PathBuf>,
    /// A note to open once the vault is open (`--open`).
    pub note: Option<String>,
    pub config: Config,
    /// `$XDG_CACHE_HOME`: indexes.
    pub cache_home: PathBuf,
    /// `$XDG_STATE_HOME`: what the app remembers, and the write journal.
    pub state_home: PathBuf,
    /// `$XDG_RUNTIME_DIR`; `None` runs without the single-instance socket.
    pub runtime_dir: Option<PathBuf>,
}

/// The app.
pub struct App {
    config: Config,
    cache_home: PathBuf,
    state_home: PathBuf,
    runtime_dir: Option<PathBuf>,
    app_state: AppState,
    screen: Screen,
    cmdline: Cmdline,
}

/// The screen shown.
pub enum Screen {
    /// First run: no vault yet (mockup 10).
    Welcome,
    /// The vault folder is gone (mockup 10).
    Missing(PathBuf),
    /// A vault is open (mockups 01, 11).
    Vault(Box<Workspace>),
}

/// An open vault and what the user has open in it.
pub struct Workspace {
    session: Session,
    /// Keeps the socket listening; removed when the app quits.
    server: Option<Arc<ipc::Server>>,
    state: VaultState,
    layout: Layout,
    index: IndexStatus,
    notes: Option<usize>,
    buffers: Vec<Buffer>,
    active: Option<usize>,
    /// The highlighted recent note on the start screen.
    selected: usize,
}

/// Where the opening index update is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IndexStatus {
    Indexing { done: usize, total: usize },
    Ready,
    Failed,
}

/// An open note.
#[derive(Debug, Clone)]
pub struct Buffer {
    pub path: String,
    pub text: Option<String>,
    pub backlinks: Option<usize>,
}

/// The command line: typing a `:` command, or showing a message.
#[derive(Debug, Default)]
struct Cmdline {
    input: Option<String>,
    message: Option<(Level, String, Instant)>,
}

#[derive(Debug, Clone)]
pub enum Message {
    ChooseFolder,
    FolderChosen(Option<PathBuf>),
    RetryVault,
    Index(IndexEvent),
    Remote(Request),
    /// A key press; `true` when a widget already handled it.
    Key(keyboard::Event, bool),
    CmdInput(String),
    CmdSubmit,
    OpenRecent(usize),
    SelectBuffer(usize),
    NoteLoaded(String, Result<String, String>),
    Backlinks(String, usize),
    Looked(String, Lookup),
    Resized(Size),
    CloseRequested(window::Id),
    Done,
}

impl App {
    /// Starts the app.
    pub fn new(launch: Launch) -> (Self, Task<Message>) {
        let app_state: AppState = state::load(&state::app_state_path(&launch.state_home));
        let mut app = Self {
            config: launch.config,
            cache_home: launch.cache_home,
            state_home: launch.state_home,
            runtime_dir: launch.runtime_dir,
            app_state,
            screen: Screen::Welcome,
            cmdline: Cmdline::default(),
        };
        let vault = launch
            .vault
            .or_else(|| app.app_state.last_vault.clone())
            .or_else(|| app.config.vault_default.clone());
        let task = match vault {
            Some(path) => {
                let open = app.open_vault(&path);
                match launch.note {
                    Some(note) => {
                        open.chain(Task::done(Message::Remote(Request::Open { path: note })))
                    }
                    None => open,
                }
            }
            None => Task::none(),
        };
        (app, task)
    }

    /// The screen shown, for tests.
    pub fn screen(&self) -> &Screen {
        &self.screen
    }

    /// The message on the command line, for tests.
    pub fn message(&self) -> Option<(Level, &str)> {
        self.cmdline
            .message
            .as_ref()
            .map(|(level, text, _)| (*level, text.as_str()))
    }

    /// Whether the `:` input is open, for tests.
    pub fn typing_command(&self) -> bool {
        self.cmdline.input.is_some()
    }

    /// The window title (F-28): `Note — Patinote`.
    pub fn title(&self) -> String {
        match self.workspace().and_then(Workspace::active_buffer) {
            Some(b) => format!("{} — Patinote", stem(&b.path)),
            None => "Patinote".into(),
        }
    }

    fn workspace(&self) -> Option<&Workspace> {
        match &self.screen {
            Screen::Vault(w) => Some(w),
            _ => None,
        }
    }

    fn workspace_mut(&mut self) -> Option<&mut Workspace> {
        match &mut self.screen {
            Screen::Vault(w) => Some(w),
            _ => None,
        }
    }

    fn say(&mut self, level: Level, text: impl Into<String>) {
        self.cmdline.message = Some((level, text.into(), Instant::now()));
    }

    /// Opens the vault at `path`, or shows "Vault not found".
    fn open_vault(&mut self, path: &Path) -> Task<Message> {
        let paths = match VaultPaths::resolve(path, &self.cache_home, &self.state_home) {
            Ok(p) => p,
            Err(_) => {
                self.screen = Screen::Missing(path.to_path_buf());
                return Task::none();
            }
        };
        let mut tasks = Vec::new();
        let server = match &self.runtime_dir {
            Some(dir) => match ipc::Server::bind(&ipc::socket_path(dir, &paths.id)) {
                Ok(server) => {
                    let server = Arc::new(server);
                    tasks.push(listen(Arc::clone(&server)));
                    Some(server)
                }
                Err(ipc::BindError::AlreadyRunning) => {
                    self.say(Level::Warning, "Patinote is already running for this vault");
                    None
                }
                Err(e) => {
                    tracing::warn!("{e}");
                    None
                }
            },
            None => None,
        };
        let state: VaultState = state::load(&state::vault_state_path(&self.state_home, &paths.id));
        let layout = state.layout.or(self.config.layout).unwrap_or_default();
        self.app_state.last_vault = Some(paths.root.clone());
        tasks.push(self.save_app_state());
        let (session, events) = Session::open(paths);
        tasks.push(Task::stream(events).map(Message::Index));
        self.screen = Screen::Vault(Box::new(Workspace {
            session,
            server,
            state,
            layout,
            index: IndexStatus::Indexing { done: 0, total: 0 },
            notes: None,
            buffers: Vec::new(),
            active: None,
            selected: 0,
        }));
        Task::batch(tasks)
    }

    fn save_app_state(&self) -> Task<Message> {
        let path = state::app_state_path(&self.state_home);
        let value = self.app_state.clone();
        Task::future(async move {
            if let Err(e) = state::save(&path, &value) {
                tracing::warn!("cannot save the app state: {e}");
            }
            Message::Done
        })
    }

    fn save_vault_state(&self) -> Task<Message> {
        let Some(w) = self.workspace() else {
            return Task::none();
        };
        let path = state::vault_state_path(&self.state_home, &w.session.paths.id);
        let value = w.state.clone();
        Task::future(async move {
            if let Err(e) = state::save(&path, &value) {
                tracing::warn!("cannot save the vault state: {e}");
            }
            Message::Done
        })
    }

    /// Opens the note at `path` (relative to the vault) in a buffer.
    fn open_note(&mut self, path: String) -> Task<Message> {
        let Some(w) = self.workspace_mut() else {
            return Task::none();
        };
        if let Some(i) = w.buffers.iter().position(|b| b.path == path) {
            w.active = Some(i);
            w.state.touch(&path, state::now());
            return self.save_vault_state();
        }
        w.buffers.push(Buffer {
            path: path.clone(),
            text: None,
            backlinks: None,
        });
        w.active = Some(w.buffers.len() - 1);
        w.state.touch(&path, state::now());
        let file = w.session.paths.root.join(&path);
        let read = {
            let path = path.clone();
            Task::future(async move {
                let text = std::fs::read(&file)
                    .map(|b| String::from_utf8_lossy(&b).into_owned())
                    .map_err(|e| e.to_string());
                Message::NoteLoaded(path, text)
            })
        };
        let backlinks = {
            let p = path.clone();
            Task::perform(
                w.session.query(move |store| {
                    store
                        .note_by_path(&p)
                        .ok()
                        .flatten()
                        .and_then(|row| store.backlinks(row.id).ok())
                        .map_or(0, |b| b.len())
                }),
                move |count| Message::Backlinks(path.clone(), count.unwrap_or(0)),
            )
        };
        Task::batch([read, backlinks, self.save_vault_state()])
    }

    fn choose_folder() -> Task<Message> {
        Task::future(async {
            let folder = rfd::AsyncFileDialog::new()
                .set_title("Choose a vault folder")
                .pick_folder()
                .await
                .map(|h| h.path().to_path_buf());
            Message::FolderChosen(folder)
        })
    }

    /// Runs a `:` command.
    fn run_command(&mut self, line: &str) -> Task<Message> {
        let line = line.trim();
        let (cmd, arg) = line
            .split_once(char::is_whitespace)
            .map_or((line, ""), |(c, a)| (c, a.trim()));
        match cmd {
            "" => Task::none(),
            "q" | "q!" | "qa" | "qa!" | "quit" | "wq" | "x" => self.quit(),
            "e" | "edit" if arg.is_empty() => {
                self.say(Level::Error, "Argument required");
                Task::none()
            }
            "e" | "edit" => {
                let Some(w) = self.workspace() else {
                    self.say(Level::Error, "No vault is open");
                    return Task::none();
                };
                let name = arg.to_owned();
                let n = name.clone();
                Task::perform(w.session.query(move |s| lookup(s, &n)), move |found| {
                    Message::Looked(name.clone(), found.unwrap_or(Lookup::NotFound))
                })
            }
            "bn" | "bnext" => self.cycle_buffer(1),
            "bp" | "bprevious" | "bN" => self.cycle_buffer(-1),
            "b" | "buffer" => match arg.parse::<usize>() {
                Ok(n) if n >= 1 => self.select_buffer(n - 1),
                _ => {
                    self.say(Level::Error, format!("No such buffer: {arg}"));
                    Task::none()
                }
            },
            "bd" | "bdelete" => self.close_buffer(),
            other => {
                self.say(Level::Error, format!("Not an editor command: {other}"));
                Task::none()
            }
        }
    }

    fn select_buffer(&mut self, i: usize) -> Task<Message> {
        let Some(w) = self.workspace_mut() else {
            return Task::none();
        };
        if i >= w.buffers.len() {
            self.say(Level::Error, format!("No such buffer: {}", i + 1));
            return Task::none();
        }
        w.active = Some(i);
        let path = w.buffers[i].path.clone();
        w.state.touch(&path, state::now());
        self.save_vault_state()
    }

    fn cycle_buffer(&mut self, step: isize) -> Task<Message> {
        let Some(w) = self.workspace() else {
            return Task::none();
        };
        let n = w.buffers.len();
        let Some(active) = w.active else {
            return Task::none();
        };
        let next = (active as isize + step).rem_euclid(n as isize) as usize;
        self.select_buffer(next)
    }

    fn close_buffer(&mut self) -> Task<Message> {
        let Some(w) = self.workspace_mut() else {
            return Task::none();
        };
        if let Some(i) = w.active {
            w.buffers.remove(i);
            w.active = if w.buffers.is_empty() {
                None
            } else {
                Some(i.min(w.buffers.len() - 1))
            };
            w.selected = 0;
        }
        Task::none()
    }

    fn quit(&mut self) -> Task<Message> {
        if let Some(server) = self.workspace().and_then(|w| w.server.as_ref()) {
            let _ = std::fs::remove_file(server.path());
        }
        let save = [self.save_app_state(), self.save_vault_state()];
        Task::batch(save).chain(iced::exit())
    }

    /// Handles a message.
    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::ChooseFolder => Self::choose_folder(),
            Message::FolderChosen(Some(folder)) => self.open_vault(&folder),
            Message::FolderChosen(None) => Task::none(),
            Message::RetryVault => match &self.screen {
                Screen::Missing(path) => {
                    let path = path.clone();
                    self.open_vault(&path)
                }
                _ => Task::none(),
            },
            Message::Index(event) => {
                let mut failed = None;
                if let Some(w) = self.workspace_mut() {
                    match event {
                        IndexEvent::Progress { done, total } => {
                            w.index = IndexStatus::Indexing { done, total };
                        }
                        IndexEvent::Ready { notes } => {
                            w.index = IndexStatus::Ready;
                            w.notes = Some(notes);
                        }
                        IndexEvent::Failed(why) => {
                            w.index = IndexStatus::Failed;
                            failed = Some(why);
                        }
                    }
                }
                if let Some(why) = failed {
                    self.say(Level::Error, why);
                }
                Task::none()
            }
            Message::Remote(request) => {
                let focus = window::latest().and_then(window::gain_focus);
                match request {
                    Request::Activate => focus,
                    Request::Open { path } => self.open_note(path).chain(focus),
                }
            }
            Message::Key(event, captured) => self.on_key(event, captured),
            Message::CmdInput(text) => {
                self.cmdline.input = Some(text);
                Task::none()
            }
            Message::CmdSubmit => {
                let line = self.cmdline.input.take().unwrap_or_default();
                self.run_command(&line)
            }
            Message::OpenRecent(i) => {
                let path = self
                    .workspace()
                    .and_then(|w| w.state.recent.get(i))
                    .map(|r| r.path.clone());
                match path {
                    Some(p) => self.open_note(p),
                    None => Task::none(),
                }
            }
            Message::SelectBuffer(i) => self.select_buffer(i),
            Message::NoteLoaded(path, result) => {
                let mut error = None;
                if let Some(w) = self.workspace_mut()
                    && let Some(b) = w.buffers.iter_mut().find(|b| b.path == path)
                {
                    match result {
                        Ok(text) => b.text = Some(text),
                        Err(e) => error = Some(format!("Can't read \"{path}\": {e}")),
                    }
                }
                if let Some(e) = error {
                    self.say(Level::Error, e);
                }
                Task::none()
            }
            Message::Backlinks(path, count) => {
                if let Some(w) = self.workspace_mut()
                    && let Some(b) = w.buffers.iter_mut().find(|b| b.path == path)
                {
                    b.backlinks = Some(count);
                }
                Task::none()
            }
            Message::Looked(name, found) => match found {
                Lookup::Found(path) => self.open_note(path),
                Lookup::Ambiguous(paths) => {
                    self.say(
                        Level::Warning,
                        format!("{} notes are named \"{name}\"", paths.len()),
                    );
                    Task::none()
                }
                Lookup::NotFound => {
                    self.say(Level::Error, format!("No note named \"{name}\""));
                    Task::none()
                }
            },
            Message::Resized(size) => {
                self.app_state.window = Some((size.width, size.height));
                Task::none()
            }
            Message::CloseRequested(_) => self.quit(),
            Message::Done => Task::none(),
        }
    }

    fn on_key(&mut self, event: keyboard::Event, captured: bool) -> Task<Message> {
        let keyboard::Event::KeyPressed {
            key,
            text,
            modifiers,
            ..
        } = event
        else {
            return Task::none();
        };
        // Messages go away on the next key; errors stay at least 5 s (0003
        // section 6).
        if let Some((level, _, at)) = &self.cmdline.message
            && (*level != Level::Error || at.elapsed() >= Duration::from_secs(ERROR_MIN_SECONDS))
        {
            self.cmdline.message = None;
        }
        if self.cmdline.input.is_some() {
            if key == Key::Named(Named::Escape) {
                self.cmdline.input = None;
            }
            return Task::none();
        }
        if captured || modifiers.control() || modifiers.alt() || modifiers.logo() {
            return Task::none();
        }
        if text.as_deref() == Some(":") {
            self.cmdline.input = Some(String::new());
            return iced::widget::operation::focus(CMDLINE);
        }
        let ch = text.as_deref().unwrap_or_default();
        let enter = key == Key::Named(Named::Enter);
        match &mut self.screen {
            Screen::Welcome => {
                if enter {
                    return Self::choose_folder();
                }
            }
            Screen::Missing(_) => {
                if enter {
                    return Self::choose_folder();
                }
                if ch == "r" {
                    return self.update(Message::RetryVault);
                }
            }
            Screen::Vault(w) if w.active.is_none() => {
                let shown = w.state.recent.len().min(RECENT_SHOWN);
                match (ch, &key) {
                    ("j", _) | (_, Key::Named(Named::ArrowDown)) if shown > 0 => {
                        w.selected = (w.selected + 1).min(shown - 1);
                    }
                    ("k", _) | (_, Key::Named(Named::ArrowUp)) => {
                        w.selected = w.selected.saturating_sub(1);
                    }
                    _ if enter && shown > 0 => {
                        let i = w.selected;
                        return self.update(Message::OpenRecent(i));
                    }
                    _ => {}
                }
            }
            Screen::Vault(_) => {}
        }
        Task::none()
    }

    /// Background work the app listens to.
    pub fn subscription(&self) -> Subscription<Message> {
        Subscription::batch([
            event::listen_with(|event, status, _| match event {
                Event::Keyboard(k) => Some(Message::Key(k, status == event::Status::Captured)),
                _ => None,
            }),
            window::resize_events().map(|(_, size)| Message::Resized(size)),
            window::close_requests().map(Message::CloseRequested),
        ])
    }

    /// Draws the window.
    pub fn view(&self) -> Element<'_, Message> {
        let (tabs, active): (Vec<Tab>, Option<usize>) = match self.workspace() {
            Some(w) => (
                w.buffers
                    .iter()
                    .map(|b| Tab {
                        title: stem(&b.path).to_owned(),
                        modified: false,
                    })
                    .collect(),
                w.active,
            ),
            None => (Vec::new(), None),
        };
        let body: Element<'_, Message> = match &self.screen {
            Screen::Welcome => welcome::view(&welcome::Card::Welcome, &welcome_actions()),
            Screen::Missing(path) => welcome::view(
                &welcome::Card::Missing {
                    path: display_path(path),
                },
                &welcome_actions(),
            ),
            Screen::Vault(w) => match w.active_buffer() {
                Some(b) => match &b.text {
                    Some(text) => note::view(text),
                    None => container("").into(),
                },
                None => w.start().view(Message::OpenRecent),
            },
        };
        // Before a vault is open the status line is an empty bar (mockup 10).
        let status: Element<'_, Message> = if self.workspace().is_some() {
            self.status_line().view_with(|_| None)
        } else {
            container("")
                .width(Length::Fill)
                .height(SIZE_STATUSLINE_H)
                .style(styles::statusline)
                .into()
        };
        column![
            bufferline::view(&tabs, active, Message::SelectBuffer),
            container(body)
                .width(Length::Fill)
                .height(Length::Fill)
                .style(styles::app),
            status,
            self.cmdline_view(),
        ]
        .into()
    }

    fn status_line(&self) -> StatusLine {
        let Some(w) = self.workspace() else {
            return StatusLine::default();
        };
        let indicator = match &w.index {
            IndexStatus::Indexing { done, total } => Some(Indicator::Indexing {
                done: *done,
                total: *total,
            }),
            IndexStatus::Ready => Some(Indicator::Indexed),
            IndexStatus::Failed => None,
        };
        let mut line = StatusLine {
            mode: Mode::Normal,
            indicator,
            ..StatusLine::default()
        };
        if let Some(b) = w.active_buffer() {
            line.path = b.path.clone();
            if let Some(text) = &b.text {
                line.notes.push(word_count(text));
            }
            if let Some(n) = b.backlinks {
                line.notes.push(if n == 1 {
                    "⇠ 1 backlink".into()
                } else {
                    format!("⇠ {n} backlinks")
                });
            }
            line.buttons = vec![
                StatusButton {
                    label: "␣e explorer",
                    on: false,
                },
                StatusButton {
                    label: "␣b panel",
                    on: false,
                },
                StatusButton {
                    label: "␣ leader",
                    on: false,
                },
            ];
            line.position = Some("1:1".into());
        }
        line
    }

    fn cmdline_view(&self) -> Element<'_, Message> {
        if let Some(input) = &self.cmdline.input {
            let field = text_input("", input)
                .id(CMDLINE)
                .on_input(Message::CmdInput)
                .on_submit(Message::CmdSubmit)
                .font(TYPE_MONO_UI.font)
                .size(TYPE_MONO_UI.size)
                .padding(0)
                .style(styles::cmdline_input);
            return container(
                iced::widget::row![typed(":", TYPE_MONO_UI), field]
                    .align_y(iced::Alignment::Center),
            )
            .width(Length::Fill)
            .height(SIZE_CMDLINE_H)
            .padding(Padding::from([0.0, SPACE_12]))
            .center_y(SIZE_CMDLINE_H)
            .style(styles::cmdline)
            .into();
        }
        match &self.cmdline.message {
            Some((level, text, _)) => cmdline::message(*level, text),
            None => container("")
                .width(Length::Fill)
                .height(SIZE_CMDLINE_H)
                .style(styles::cmdline)
                .into(),
        }
    }
}

impl Workspace {
    fn active_buffer(&self) -> Option<&Buffer> {
        self.active.and_then(|i| self.buffers.get(i))
    }

    /// The start screen's content.
    fn start(&self) -> Start {
        let now = state::now();
        Start {
            name: self.session.paths.name.clone(),
            path: display_path(&self.session.paths.root),
            notes: self.notes,
            recent: self
                .state
                .recent
                .iter()
                .take(RECENT_SHOWN)
                .map(|r| RecentRow {
                    title: stem(&r.path).to_owned(),
                    folder: r
                        .path
                        .rsplit_once('/')
                        .map(|(dir, _)| format!("{dir}/"))
                        .unwrap_or_default(),
                    when: relative_time(now.saturating_sub(r.opened)),
                })
                .collect(),
            selected: self.selected,
        }
    }

    /// The layout in use.
    pub fn layout(&self) -> Layout {
        self.layout
    }

    /// The open notes, for tests.
    pub fn buffers(&self) -> &[Buffer] {
        &self.buffers
    }

    /// The active buffer index, for tests.
    pub fn active(&self) -> Option<usize> {
        self.active
    }
}

/// The window size to open with: the remembered one, or the mockups' size.
pub fn window_size(state_home: &Path) -> Size {
    let app_state: AppState = state::load(&state::app_state_path(state_home));
    app_state
        .window
        .map_or(Size::new(SIZE_WINDOW.0, SIZE_WINDOW.1), |(w, h)| {
            Size::new(w, h)
        })
}

/// `186 words`.
fn word_count(text: &str) -> String {
    let n = text.split_whitespace().count();
    if n == 1 {
        "1 word".into()
    } else {
        format!("{n} words")
    }
}

/// A path with the home folder shown as `~`.
pub fn display_path(path: &Path) -> String {
    if let Some(home) = std::env::var_os("HOME")
        && let Ok(rest) = path.strip_prefix(&home)
    {
        return if rest.as_os_str().is_empty() {
            "~".into()
        } else {
            format!("~/{}", rest.display())
        };
    }
    path.display().to_string()
}

/// Forwards requests from the socket to the app. The server thread answers
/// each request right away; the app acts on it as a message.
fn listen(server: Arc<ipc::Server>) -> Task<Message> {
    let (tx, rx) = iced::futures::channel::mpsc::unbounded();
    std::thread::Builder::new()
        .name("patinote-socket".into())
        .spawn(move || {
            server.serve(|request| match tx.unbounded_send(request) {
                Ok(()) => Response::ok(),
                Err(_) => Response {
                    ok: false,
                    error: Some("the app is closing".into()),
                },
            });
        })
        .map_err(|e| tracing::warn!("cannot start the socket thread: {e}"))
        .ok();
    Task::stream(rx).map(Message::Remote)
}

fn welcome_actions() -> welcome::Actions<Message> {
    welcome::Actions {
        choose: Message::ChooseFolder,
        retry: Message::RetryVault,
    }
}
