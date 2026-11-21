//! The app: which screen shows, the bufferline, status line and command line,
//! the vault session, and what is remembered between sessions. Editing
//! (keys to the Vim engine, saving, banners) is in [`editing`]. Logic lives
//! in [`App::update`] so tests can drive it without a window.

mod editing;
mod linking;
mod panels;
mod switching;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use iced::keyboard::{self, Key, Modifiers, key::Named};
use iced::widget::{Column, container, text_input};
use iced::{Element, Event, Length, Padding, Size, Subscription, Task, event, window};
use patinote_core::config::{Config, Layout};
use patinote_core::ipc::{self, Request, Response};
use patinote_core::names::stem;
use patinote_core::write::Journal;

use crate::editor::document::{Document, SaveDone};
use crate::screens::start::{RECENT_SHOWN, RecentRow, Start, relative_time};
use crate::screens::welcome;
use crate::session::{IndexEvent, Lookup, Session, VaultPaths, lookup};
use crate::state::{self, AppState, VaultState};
use crate::switcher::{Candidate, Finder};
use crate::theme::styles::{self, Level};
use crate::theme::tokens::*;
use crate::widgets::bufferline::{self, Tab};
use crate::widgets::cmdline::{self, ERROR_MIN_SECONDS};
use crate::widgets::typed;

/// The id of the command line input.
const CMDLINE: &str = "cmdline";

/// What the app was started with.
#[derive(Debug, Clone, Default)]
pub struct Launch {
    /// The vault to open: `--vault`, `$PATINOTE_VAULT`, the last vault, then
    /// `[vault] default`, in that order (F-01, Q-014).
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
    /// The window's height, for Ctrl+d / Ctrl+u.
    window_height: f32,
    /// Quitting once every note is saved.
    quitting: bool,
    /// The clipboard text last seen, so focus changes only reload the
    /// register when the clipboard changed.
    clipboard: Option<String>,
    /// The quick switcher, when open (F-07).
    switcher: Option<switching::Switcher>,
    /// Modifier keys held now (for `Shift+Enter`).
    modifiers: Modifiers,
    /// The leader key (Space) was pressed and waits for the next key (F-28).
    leader: bool,
    /// Link suggestions after `[[` (F-08).
    autocomplete: Option<linking::Autocomplete>,
    /// The picker for an ambiguous link (F-08, mockup 12).
    picker: Option<linking::Picker>,
    /// A heading or block to show once its note has loaded: (path, subpath).
    pending_jump: Option<(String, String)>,
    /// Cursors to set once their notes have loaded (session restore,
    /// backlinks, the jump list).
    pending_cursor: HashMap<String, (usize, usize)>,
    /// The side panel is open (F-09).
    panel_open: bool,
    /// The item of the side panel with the keyboard focus; `None` when the
    /// editor has it.
    focus: Option<crate::screens::panel::Focused>,
    /// `Ctrl+w` was pressed and waits for `h` or `l`.
    window_prefix: bool,
    /// `]` or `[` was pressed in normal mode and waits for `b`.
    bracket: Option<char>,
    /// The jump list (F-09.5) and where `Ctrl+O`/`Ctrl+I` are in it.
    jumps: Vec<panels::Jump>,
    jump_at: usize,
    /// Moving through the jump list, which must not record jumps.
    navigating: bool,
    /// Counts buffer activations, to find the least recently used one.
    use_counter: u64,
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
    journal: Journal,
    /// Keeps the socket listening; removed when the app quits.
    server: Option<Arc<ipc::Server>>,
    state: VaultState,
    layout: Layout,
    index: IndexStatus,
    notes: Option<usize>,
    buffers: Vec<Buffer>,
    active: Option<usize>,
    /// Notes closed with unsaved changes, kept until their save lands.
    closing: Vec<Document>,
    /// The highlighted recent note on the start screen.
    selected: usize,
    /// Notes the quick switcher finds, loaded from the index.
    candidates: Vec<Candidate>,
    /// Attachments, suggested after `![[` (F-08).
    attachments: Vec<Candidate>,
    finder: Finder,
    /// Link targets (lowercase) that match several notes, per open note.
    ambiguous: HashMap<String, HashSet<String>>,
}

/// Where the opening index update is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IndexStatus {
    Indexing { done: usize, total: usize },
    Ready,
    Failed,
}

/// An open note in the bufferline.
pub struct Buffer {
    pub path: String,
    /// `None` while the file is being read.
    pub doc: Option<Document>,
    /// Links into this note with their lines (F-09).
    pub backlinks: Option<Vec<patinote_core::commands::Backlink>>,
    /// Identifies the buffer to the editor widget.
    pub id: u64,
    /// When it was last made active, for closing the oldest (F-09.7).
    pub used: u64,
}

/// What the command line is doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Prompt {
    /// `:` commands.
    Command,
    /// `/` (forward) or `?` (backward) search.
    Search { forward: bool },
}

#[derive(Debug, Default)]
struct Cmdline {
    input: Option<(Prompt, String)>,
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
    NoteLoaded(String, Result<Vec<u8>, String>),
    Backlinks(String, Vec<patinote_core::commands::Backlink>),
    /// `Enter` (or a click) on an outline item.
    OutlineJump(usize),
    /// `Enter` (or a click) on a backlink.
    OpenBacklink(usize),
    Looked(String, Lookup),
    Resized(Size),
    CloseRequested(window::Id),
    /// The window gained (`true`) or lost focus.
    Focus(bool),
    /// Clipboard text, read when the window gains focus.
    Clipboard(Option<String>),
    /// Autosave and retry timer.
    Tick(Instant),
    Saved(SaveDone),
    /// A click in the editor at a line and character; `true` with Ctrl.
    Click(usize, usize, bool),
    /// The wheel scrolled the editor: first and last visible line.
    Scrolled(usize, usize),
    /// Notes and attachments for finding and linking, from the index.
    Candidates(Vec<Candidate>, Vec<Candidate>),
    /// A link was resolved: its name, the column it starts at, and where it
    /// leads.
    Followed(String, usize, linking::Followed),
    /// Heading or block suggestions: the link's start column, what was
    /// typed, and (level, text); level 0 is a block.
    Headings(usize, String, Vec<(u8, String)>),
    /// A link suggestion was clicked.
    Suggest(usize),
    /// A note was picked for an ambiguous link.
    Pick(usize),
    /// Ambiguous link targets of a note.
    Ambiguous(String, HashSet<String>),
    /// The index was refreshed after saving this note.
    Reindexed(String),
    SwitcherInput(String),
    SwitcherSubmit,
    SwitcherPick(usize),
    /// A banner button of the active note.
    RetrySave,
    OpenExternally,
    Done,
}

impl App {
    /// Starts the app.
    pub fn new(launch: Launch) -> (Self, Task<Message>) {
        let app_state: AppState = state::load(&state::app_state_path(&launch.state_home));
        let window_height = app_state.window.map_or(SIZE_WINDOW.1, |(_, h)| h);
        let mut app = Self {
            config: launch.config,
            cache_home: launch.cache_home,
            state_home: launch.state_home,
            runtime_dir: launch.runtime_dir,
            app_state,
            screen: Screen::Welcome,
            cmdline: Cmdline::default(),
            window_height,
            quitting: false,
            clipboard: None,
            switcher: None,
            modifiers: Modifiers::default(),
            leader: false,
            autocomplete: None,
            picker: None,
            pending_jump: None,
            pending_cursor: HashMap::new(),
            panel_open: false,
            focus: None,
            window_prefix: false,
            bracket: None,
            jumps: Vec::new(),
            jump_at: 0,
            navigating: false,
            use_counter: 0,
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

    /// Whether a prompt (`:`, `/`, `?`) is open, for tests.
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
        let journal = Journal::for_vault(&paths.state_home, &paths.id);
        let root = paths.root.clone();
        let (session, events) = Session::open(paths);
        tasks.push(Task::stream(events).map(Message::Index));
        self.panel_open = state.panel_open;
        // The last session's buffers and cursors come back (F-09.6).
        let mut buffers = Vec::new();
        for (id, path) in state.buffers.iter().enumerate() {
            if !root.join(path).is_file() {
                continue;
            }
            if let Some(c) = state.cursors.get(path) {
                self.pending_cursor.insert(path.clone(), *c);
            }
            let file = root.join(path);
            let p = path.clone();
            tasks.push(Task::future(async move {
                let bytes = std::fs::read(&file).map_err(|e| e.to_string());
                Message::NoteLoaded(p, bytes)
            }));
            buffers.push(Buffer {
                path: path.clone(),
                doc: None,
                backlinks: None,
                id: id as u64,
                used: 0,
            });
        }
        let active = state
            .active
            .filter(|_| !buffers.is_empty())
            .map(|a| a.min(buffers.len() - 1));
        self.screen = Screen::Vault(Box::new(Workspace {
            session,
            journal,
            server,
            state,
            layout,
            index: IndexStatus::Indexing { done: 0, total: 0 },
            notes: None,
            buffers,
            active,
            closing: Vec::new(),
            selected: 0,
            candidates: Vec::new(),
            attachments: Vec::new(),
            finder: Finder::new(),
            ambiguous: HashMap::new(),
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
        let mut value = w.state.clone();
        // The session as it is now: open buffers and their cursors.
        value.buffers = w.buffers.iter().map(|b| b.path.clone()).collect();
        value.active = w.active;
        value.cursors.retain(|p, _| value.buffers.contains(p));
        for b in &w.buffers {
            if let Some(d) = &b.doc {
                let c = d.buffer.cursor();
                value.cursors.insert(b.path.clone(), (c.line, c.col));
            }
        }
        Task::future(async move {
            if let Err(e) = state::save(&path, &value) {
                tracing::warn!("cannot save the vault state: {e}");
            }
            Message::Done
        })
    }

    /// Opens the note at `path` (relative to the vault) in a buffer. The
    /// note being left is saved first (F-05).
    fn open_note(&mut self, path: String) -> Task<Message> {
        if self.active_document().is_some_and(|d| d.path != path) {
            self.record_jump();
        }
        let leave = self.autosave(true);
        self.use_counter += 1;
        let used = self.use_counter;
        let Some(w) = self.workspace_mut() else {
            return Task::none();
        };
        w.state.touch(&path, state::now());
        if let Some(i) = w.buffers.iter().position(|b| b.path == path) {
            w.active = Some(i);
            w.buffers[i].used = used;
            self.apply_pending_cursor();
            return Task::batch([leave, self.save_vault_state()]);
        }
        let id = w.buffers.iter().map(|b| b.id).max().map_or(0, |m| m + 1);
        w.buffers.push(Buffer {
            path: path.clone(),
            doc: None,
            backlinks: None,
            id,
            used,
        });
        w.active = Some(w.buffers.len() - 1);
        self.enforce_buffer_limit();
        let Some(w) = self.workspace_mut() else {
            return Task::none();
        };
        let file = w.session.paths.root.join(&path);
        let read = {
            let path = path.clone();
            Task::future(async move {
                let bytes = std::fs::read(&file).map_err(|e| e.to_string());
                Message::NoteLoaded(path, bytes)
            })
        };
        let backlinks = self.load_backlinks(path.clone());
        Task::batch([leave, read, backlinks, self.save_vault_state()])
    }

    /// How many links point at the note at `path` (status line).
    fn load_backlinks(&self, path: String) -> Task<Message> {
        let Some(w) = self.workspace() else {
            return Task::none();
        };
        let p = path.clone();
        Task::perform(
            w.session.query(move |store| {
                store
                    .note_by_path(&p)
                    .ok()
                    .flatten()
                    .and_then(|row| store.backlinks_with_context(row.id).ok())
                    .unwrap_or_default()
            }),
            move |links| Message::Backlinks(path.clone(), links.unwrap_or_default()),
        )
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

    /// Runs a `:` command (F-06).
    fn run_command(&mut self, line: &str) -> Task<Message> {
        let line = line.trim();
        let (cmd, arg) = line
            .split_once(char::is_whitespace)
            .map_or((line, ""), |(c, a)| (c, a.trim()));
        match cmd {
            "" => Task::none(),
            // `:q` closes the note, and quits only when none is open
            // (F-06.4). `:q!` drops changes since the last save.
            "q" | "quit" => self.close_or_quit(false),
            "q!" | "quit!" => self.close_or_quit(true),
            "w" | "write" => self.autosave(true),
            "wq" | "x" | "xit" => {
                let save = self.autosave(true);
                save.chain(Task::done(Message::Done))
                    .chain(self.close_or_quit(false))
            }
            "qa" | "qall" | "wqa" | "xa" => self.quit(false),
            "qa!" | "qall!" => self.quit(true),
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
                _ => match self.buffer_by_name(arg) {
                    Some(i) => self.select_buffer(i),
                    None => {
                        self.say(Level::Error, format!("No such buffer: {arg}"));
                        Task::none()
                    }
                },
            },
            "bd" | "bdelete" => self.close_or_quit(false),
            other => {
                self.say(Level::Error, format!("Not an editor command: {other}"));
                Task::none()
            }
        }
    }

    fn select_buffer(&mut self, i: usize) -> Task<Message> {
        let count = self.workspace().map_or(0, |w| w.buffers.len());
        if i >= count {
            self.say(Level::Error, format!("No such buffer: {}", i + 1));
            return Task::none();
        }
        if self.workspace().and_then(|w| w.active) != Some(i) {
            self.record_jump();
        }
        let leave = self.autosave(true);
        self.use_counter += 1;
        let used = self.use_counter;
        let Some(w) = self.workspace_mut() else {
            return Task::none();
        };
        w.active = Some(i);
        w.buffers[i].used = used;
        let path = w.buffers[i].path.clone();
        w.state.touch(&path, state::now());
        self.apply_pending_cursor();
        Task::batch([leave, self.save_vault_state()])
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

    /// Closes the active note, or quits when no note is open.
    fn close_or_quit(&mut self, discard: bool) -> Task<Message> {
        let Some(w) = self.workspace_mut() else {
            return self.quit(discard);
        };
        let Some(i) = w.active else {
            return self.quit(discard);
        };
        let buffer = w.buffers.remove(i);
        w.active = if w.buffers.is_empty() {
            None
        } else {
            Some(i.min(w.buffers.len() - 1))
        };
        w.selected = 0;
        let Some(mut doc) = buffer.doc else {
            return Task::none();
        };
        if discard || !doc.dirty() || doc.read_only.is_some() {
            return Task::none();
        }
        // Unsaved text is kept until it is on disk.
        let job = doc.start_save(&w.session.paths.root, &w.journal);
        w.closing.push(doc);
        Task::future(async move { Message::Saved(job.run()) })
    }

    /// Quits once every note is saved; `discard` quits right away.
    fn quit(&mut self, discard: bool) -> Task<Message> {
        if !discard {
            let pending = self.autosave(true);
            if self.unsaved() || self.waiting_on_vim() {
                self.quitting = true;
                return pending;
            }
        }
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
                let ready = matches!(event, IndexEvent::Ready { .. });
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
                if ready {
                    self.load_candidates()
                } else {
                    Task::none()
                }
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
                if let Some((_, input)) = &mut self.cmdline.input {
                    *input = text;
                }
                Task::none()
            }
            Message::CmdSubmit => match self.cmdline.input.take() {
                Some((Prompt::Command, line)) => self.run_command(&line),
                Some((Prompt::Search { forward }, pattern)) => self.search(&pattern, forward),
                None => Task::none(),
            },
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
            Message::OutlineJump(i) => {
                self.focus = None;
                self.outline_jump(i);
                Task::none()
            }
            Message::OpenBacklink(i) => {
                self.focus = None;
                self.open_backlink(i)
            }
            Message::NoteLoaded(path, result) => self.note_loaded(path, result),
            Message::Backlinks(path, links) => {
                if let Some(w) = self.workspace_mut()
                    && let Some(b) = w.buffers.iter_mut().find(|b| b.path == path)
                {
                    b.backlinks = Some(links);
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
                self.window_height = size.height;
                Task::none()
            }
            Message::CloseRequested(_) => self.quit(false),
            Message::Focus(true) => iced::clipboard::read().map(Message::Clipboard),
            Message::Focus(false) => self.autosave(true),
            Message::Clipboard(text) => {
                self.clipboard_changed(text);
                Task::none()
            }
            Message::Tick(now) => {
                let pending = self.vim_deadlines(now);
                Task::batch([pending, self.autosave_at(now, false)])
            }
            Message::Saved(done) => self.saved(done),
            Message::Click(line, col, ctrl) => self.click(line, col, ctrl),
            Message::Scrolled(first, last) => {
                self.scrolled(first, last);
                Task::none()
            }
            Message::Candidates(candidates, attachments) => {
                if let Some(w) = self.workspace_mut() {
                    w.candidates = candidates;
                    w.attachments = attachments;
                }
                self.refresh_switcher();
                Task::none()
            }
            Message::Followed(name, col, followed) => self.followed(name, col, followed),
            Message::Headings(start, typed, found) => {
                self.headings_loaded(start, typed, found);
                Task::none()
            }
            Message::Suggest(i) => self.accept_suggestion(i),
            Message::Pick(i) => self.pick(i),
            Message::Ambiguous(path, set) => {
                if let Some(w) = self.workspace_mut() {
                    w.ambiguous.insert(path, set);
                }
                Task::none()
            }
            Message::Reindexed(path) => {
                // A save can add links to any open note (F-09.1).
                let open: Vec<String> = self
                    .workspace()
                    .map(|w| w.buffers.iter().map(|b| b.path.clone()).collect())
                    .unwrap_or_default();
                let mut tasks: Vec<_> = open.into_iter().map(|p| self.load_backlinks(p)).collect();
                tasks.push(self.load_candidates());
                tasks.push(self.load_ambiguous(path));
                Task::batch(tasks)
            }
            Message::SwitcherInput(query) => {
                self.switcher_input(query);
                Task::none()
            }
            Message::SwitcherSubmit => self.switcher_submit(),
            Message::SwitcherPick(i) => self.switcher_pick(i),
            Message::RetrySave => self.autosave(true),
            Message::OpenExternally => {
                self.open_externally();
                Task::none()
            }
            Message::Done => Task::none(),
        }
    }

    fn on_key(&mut self, event: keyboard::Event, captured: bool) -> Task<Message> {
        if let keyboard::Event::ModifiersChanged(m) = event {
            self.modifiers = m;
            return Task::none();
        }
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
        if self.switcher.is_some() {
            // The input takes typed text; Escape and movement come here.
            if captured && key != Key::Named(Named::Escape) {
                return Task::none();
            }
            return self
                .switcher_key(&key, modifiers)
                .unwrap_or_else(Task::none);
        }
        if captured {
            return Task::none();
        }
        if let Some(task) = self.picker_key(&key, text.as_deref(), modifiers) {
            return task;
        }
        if std::mem::take(&mut self.window_prefix) {
            return self.window_key(text.as_deref());
        }
        if self.focus.is_some() {
            return self.panel_key(&key, text.as_deref(), modifiers);
        }
        // `Ctrl+Shift+O` opens the quick switcher anywhere (F-07).
        if modifiers.control()
            && modifiers.shift()
            && matches!(&key, Key::Character(c) if c.eq_ignore_ascii_case("o"))
        {
            return self.open_switcher();
        }
        // The leader key (Space) in normal mode, then one key (F-28). The
        // popup after 300 ms comes with step 2.9.
        if std::mem::take(&mut self.leader) {
            return match text.as_deref() {
                Some("f") => self.open_switcher(),
                Some("b") => self.toggle_panel(true),
                Some("o") => self.toggle_panel(false),
                Some("x") if self.active_document().is_some() => self.close_or_quit(false),
                _ => Task::none(),
            };
        }
        let normal_mode = self
            .active_document()
            .is_none_or(|d| d.vim.mode() == crate::vim::Mode::Normal);
        if normal_mode
            && self.workspace().is_some()
            && modifiers.control()
            && key == Key::Character("w".into())
        {
            // `Ctrl+w h/l`: focus between editor and panel (F-28).
            self.window_prefix = true;
            return Task::none();
        }
        let normal = self
            .active_document()
            .is_none_or(|d| d.vim.mode() == crate::vim::Mode::Normal && d.vim.pending().is_empty());
        if normal
            && self.workspace().is_some()
            && key == Key::Named(Named::Space)
            && !modifiers.control()
            && !modifiers.alt()
        {
            self.leader = true;
            return Task::none();
        }
        // An open note gets every key: Vim decides what it means.
        if self.active_document().is_some() {
            return self.edit_key(&key, text.as_deref(), modifiers);
        }
        if modifiers.control() || modifiers.alt() || modifiers.logo() {
            return Task::none();
        }
        if text.as_deref() == Some(":") {
            return self.open_prompt(Prompt::Command);
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

    fn open_prompt(&mut self, prompt: Prompt) -> Task<Message> {
        self.cmdline.input = Some((prompt, String::new()));
        iced::widget::operation::focus(CMDLINE)
    }

    /// Background work the app listens to.
    pub fn subscription(&self) -> Subscription<Message> {
        let mut subs = vec![
            event::listen_with(|event, status, _| match event {
                Event::Keyboard(k) => Some(Message::Key(k, status == event::Status::Captured)),
                Event::Window(window::Event::Focused) => Some(Message::Focus(true)),
                Event::Window(window::Event::Unfocused) => Some(Message::Focus(false)),
                _ => None,
            }),
            window::resize_events().map(|(_, size)| Message::Resized(size)),
            window::close_requests().map(Message::CloseRequested),
        ];
        // The timer runs only while something waits for it.
        if self.unsaved() || self.waiting_on_vim() {
            subs.push(Subscription::run(ticks).map(Message::Tick));
        }
        Subscription::batch(subs)
    }

    /// Draws the window.
    pub fn view(&self) -> Element<'_, Message> {
        let insert = self
            .active_document()
            .is_some_and(|d| d.vim.mode() == crate::vim::Mode::Insert);
        let (tabs, active): (Vec<Tab>, Option<usize>) = match self.workspace() {
            Some(w) => (
                w.buffers
                    .iter()
                    .map(|b| Tab {
                        title: stem(&b.path).to_owned(),
                        modified: b.doc.as_ref().is_some_and(Document::dirty),
                        insert,
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
                Some(b) => self.editor_view(b),
                None => w.start().view(Message::OpenRecent),
            },
        };
        let body = self.with_switcher(self.with_panel(body));
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
        let mut page = Column::new().push(bufferline::view(&tabs, active, Message::SelectBuffer));
        if let Some(banner) = self.banner() {
            page = page.push(banner);
        }
        page.push(
            container(body)
                .width(Length::Fill)
                .height(Length::Fill)
                .style(styles::app),
        )
        .push(status)
        .push(self.cmdline_view())
        .into()
    }

    fn cmdline_view(&self) -> Element<'_, Message> {
        if let Some((prompt, input)) = &self.cmdline.input {
            let lead = match prompt {
                Prompt::Command => ":",
                Prompt::Search { forward: true } => "/",
                Prompt::Search { forward: false } => "?",
            };
            let field = text_input("", input)
                .id(CMDLINE)
                .on_input(Message::CmdInput)
                .on_submit(Message::CmdSubmit)
                .font(TYPE_MONO_UI.font)
                .size(TYPE_MONO_UI.size)
                .padding(0)
                .style(styles::cmdline_input);
            return container(
                iced::widget::row![typed(lead, TYPE_MONO_UI), field]
                    .align_y(iced::Alignment::Center),
            )
            .width(Length::Fill)
            .height(SIZE_CMDLINE_H)
            .padding(Padding::from([0.0, SPACE_12]))
            .center_y(SIZE_CMDLINE_H)
            .style(styles::cmdline)
            .into();
        }
        if let Some((level, text, _)) = &self.cmdline.message {
            return cmdline::message(*level, text);
        }
        // `-- INSERT --` and friends, as Vim shows them.
        let mode = self.active_document().map(|d| d.vim.mode());
        let shown = match mode {
            Some(crate::vim::Mode::Insert) => Some(("-- INSERT --", MODE_INSERT)),
            Some(crate::vim::Mode::Visual) => Some(("-- VISUAL --", MODE_VISUAL)),
            Some(crate::vim::Mode::VisualLine) => Some(("-- VISUAL LINE --", MODE_VISUAL)),
            _ => None,
        };
        let content: Element<'_, Message> = match shown {
            Some((text, color)) => typed(text, TYPE_MONO_UI).color(color).into(),
            None => container("").into(),
        };
        container(content)
            .width(Length::Fill)
            .height(SIZE_CMDLINE_H)
            .padding(Padding::from([0.0, SPACE_12]))
            .center_y(SIZE_CMDLINE_H)
            .style(styles::cmdline)
            .into()
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

/// How often the autosave timer ticks.
const TICK: Duration = Duration::from_millis(250);

/// A tick every [`TICK`], from a thread that stops when nobody listens.
fn ticks() -> impl iced::futures::Stream<Item = Instant> {
    let (tx, rx) = iced::futures::channel::mpsc::unbounded();
    std::thread::Builder::new()
        .name("patinote-timer".into())
        .spawn(move || {
            loop {
                std::thread::sleep(TICK);
                if tx.unbounded_send(Instant::now()).is_err() {
                    break;
                }
            }
        })
        .map_err(|e| tracing::warn!("cannot start the timer thread: {e}"))
        .ok();
    rx
}

fn welcome_actions() -> welcome::Actions<Message> {
    welcome::Actions {
        choose: Message::ChooseFolder,
        retry: Message::RetryVault,
    }
}
