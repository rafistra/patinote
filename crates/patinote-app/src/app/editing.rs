//! Editing an open note (F-05, F-06): keys to the Vim engine, the system
//! clipboard, search, autosave and its failures, read-only notes, and the
//! editor, banner and status line views.

use std::time::Instant;

use iced::keyboard::{Key, Modifiers, key::Named};
use iced::{Element, Task};

use super::{App, Buffer, Message, Prompt, Screen};
use crate::editor::document::{Document, Save, SaveDone};
use crate::editor::file::ReadOnly;
use crate::editor::view::{Editor, Shape};
use crate::fonts;
use crate::mode::Mode;
use crate::theme::styles::{BannerKind, Level};
use crate::theme::tokens::*;
use crate::vim::{self, Outcome, Register, Request};
use crate::widgets::banner::{Action, Banner, Icon};
use crate::widgets::statusline::{Indicator, StatusButton, StatusLine};

/// A Vim key from an Iced key press.
pub fn vim_key(key: &Key, text: Option<&str>, modifiers: Modifiers) -> Option<vim::Key> {
    if let Key::Named(named) = key {
        let k = match named {
            Named::Escape => vim::Key::Esc,
            Named::Enter => vim::Key::Enter,
            Named::Backspace => vim::Key::Backspace,
            Named::Delete => vim::Key::Delete,
            Named::Tab => vim::Key::Tab,
            Named::ArrowLeft => vim::Key::Left,
            Named::ArrowRight => vim::Key::Right,
            Named::ArrowUp => vim::Key::Up,
            Named::ArrowDown => vim::Key::Down,
            Named::Home => vim::Key::Home,
            Named::End => vim::Key::End,
            Named::Space if !modifiers.control() => vim::Key::Char(' '),
            _ => return None,
        };
        return Some(k);
    }
    if modifiers.control() {
        let Key::Character(c) = key else { return None };
        let c = c.chars().next()?.to_ascii_lowercase();
        // Ctrl+[ is Escape, as in Vim.
        return Some(if c == '[' {
            vim::Key::Esc
        } else {
            vim::Key::Ctrl(c)
        });
    }
    if modifiers.alt() || modifiers.logo() {
        return None;
    }
    let mut chars = text?.chars();
    let c = chars.next()?;
    (chars.next().is_none() && !c.is_control()).then_some(vim::Key::Char(c))
}

/// The error to show after a save that failed.
fn landed_message(doc: &Document) -> Option<(Level, String)> {
    match &doc.save {
        Save::Failed { reason, .. } => Some((
            Level::Error,
            format!("Can't save \"{}\": {reason}", doc.path),
        )),
        _ => None,
    }
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

impl App {
    /// The active note's text, for tests.
    pub fn active_text(&self) -> Option<String> {
        self.active_document().map(|d| d.buffer.text())
    }

    /// The active note's cursor (line, character), for tests.
    pub fn active_cursor(&self) -> Option<(usize, usize)> {
        self.active_document().map(|d| {
            let c = d.buffer.cursor();
            (c.line, c.col)
        })
    }

    /// Links into the active note, as the status line shows, for tests.
    pub fn active_backlinks(&self) -> Option<usize> {
        self.workspace()?
            .active_buffer()?
            .backlinks
            .as_ref()
            .map(Vec::len)
    }

    /// The text of the open note at `path`, for tests.
    pub fn buffer_text(&self, path: &str) -> Option<String> {
        self.workspace()?
            .buffers
            .iter()
            .find(|b| b.path == path)?
            .doc
            .as_ref()
            .map(|d| d.buffer.text())
    }

    /// Whether the app waits for saves to land before quitting, for tests.
    pub fn quitting(&self) -> bool {
        self.quitting
    }

    /// Whether the active note has unsaved changes, for tests.
    pub fn active_dirty(&self) -> Option<bool> {
        self.active_document().map(Document::dirty)
    }

    /// The banner title shown above the active note, for tests.
    pub fn banner_title(&self) -> Option<String> {
        let doc = self.active_document()?;
        if let Some(why) = doc.read_only {
            return Some(why.title().into());
        }
        match &doc.save {
            Save::Failed { reason, .. } => Some(format!(
                "Couldn't save {}: {reason}.",
                doc.path.rsplit('/').next().unwrap_or(&doc.path)
            )),
            _ => None,
        }
    }

    pub(super) fn active_document(&self) -> Option<&Document> {
        self.workspace()
            .and_then(|w| w.active_buffer())
            .and_then(|b| b.doc.as_ref())
    }

    pub(super) fn active_document_mut(&mut self) -> Option<&mut Document> {
        let w = self.workspace_mut()?;
        let i = w.active?;
        w.buffers.get_mut(i)?.doc.as_mut()
    }

    /// Text lines that fit in the editor, for Ctrl+d and Ctrl+u.
    fn viewport_lines(&self) -> usize {
        let chrome = SIZE_BUFFERLINE_H + SIZE_STATUSLINE_H + SIZE_CMDLINE_H + PAD_EDITOR_TOP;
        (((self.window_height - chrome) / TYPE_EDITOR.line_height).floor() as usize).max(1)
    }

    /// A key for the open note.
    pub(super) fn edit_key(
        &mut self,
        key: &Key,
        text: Option<&str>,
        modifiers: Modifiers,
    ) -> Task<Message> {
        if let Some(task) = self.autocomplete_key(key, modifiers) {
            return task;
        }
        let Some(vk) = vim_key(key, text, modifiers) else {
            return Task::none();
        };
        let lines = self.viewport_lines();
        let now = Instant::now();
        let Some(doc) = self.active_document_mut() else {
            return Task::none();
        };
        // `gd`, `gx` and `Enter` on a link (F-08); `gt`/`gT`, `]b`/`[b`
        // and `Ctrl+O`/`Ctrl+I` (F-09). Vim leaves them alone.
        if doc.vim.mode() == vim::Mode::Normal {
            let pending_g = doc.vim.pending() == "g";
            if let Some(b) = self.bracket.take() {
                if vk == vim::Key::Char('b') {
                    return self.cycle_buffer(if b == ']' { 1 } else { -1 });
                }
                return Task::none();
            }
            let Some(doc) = self.active_document_mut() else {
                return Task::none();
            };
            match vk {
                vim::Key::Char('t') | vim::Key::Char('T') if pending_g => {
                    doc.vim.handle(&mut doc.buffer, vim::Key::Esc, now, lines);
                    return self.cycle_buffer(if vk == vim::Key::Char('t') { 1 } else { -1 });
                }
                vim::Key::Char(c @ (']' | '[')) if doc.vim.pending().is_empty() => {
                    self.bracket = Some(c);
                    return Task::none();
                }
                vim::Key::Ctrl('o') if doc.vim.pending().is_empty() => return self.jump(true),
                vim::Key::Ctrl('i') if doc.vim.pending().is_empty() => return self.jump(false),
                vim::Key::Char('d') | vim::Key::Char('x') if pending_g => {
                    doc.vim.handle(&mut doc.buffer, vim::Key::Esc, now, lines);
                    return if vk == vim::Key::Char('d') {
                        self.follow_link()
                    } else {
                        self.open_link_externally()
                    };
                }
                vim::Key::Enter if doc.vim.pending().is_empty() => {
                    let c = doc.buffer.cursor();
                    let on_link = doc
                        .buffer
                        .lines()
                        .get(c.line)
                        .and_then(|l| crate::links::link_at(l, c.col))
                        .is_some();
                    if on_link {
                        return self.follow_link();
                    }
                }
                _ => {}
            }
        }
        let Some(doc) = self.active_document_mut() else {
            return Task::none();
        };
        // Jump motions are remembered for `Ctrl+O` (F-09), as in Vim.
        let jump_key = doc.vim.mode() == vim::Mode::Normal
            && matches!(
                vk,
                vim::Key::Char('G' | 'n' | 'N' | '%' | '{' | '}' | '(' | ')')
            )
            || (doc.vim.pending() == "g" && vk == vim::Key::Char('g'));
        let before = doc.buffer.cursor();
        let outcome = doc.vim.handle(&mut doc.buffer, vk, now, lines);
        let moved_line = doc.buffer.cursor().line != before.line;
        let path = doc.path.clone();
        if jump_key && moved_line && !self.navigating {
            self.jumps.truncate(self.jump_at);
            self.jumps.push((path, (before.line, before.col)));
            if self.jumps.len() > super::panels::MAX_JUMPS {
                self.jumps.remove(0);
            }
            self.jump_at = self.jumps.len();
        }
        let Some(doc) = self.active_document_mut() else {
            return Task::none();
        };
        if doc.read_only.is_some() && outcome.changed {
            // Edits do nothing on a read-only note (F-05.8): take the change
            // back and leave insert mode.
            doc.vim.undo(&mut doc.buffer);
            if doc.vim.mode() == vim::Mode::Insert {
                doc.vim.handle(&mut doc.buffer, vim::Key::Esc, now, lines);
            }
            doc.saved_rev = doc.buffer.revision();
            self.say(Level::Error, "Cannot make changes, the note is read-only");
            return Task::none();
        }
        if outcome.changed {
            doc.edited(now);
            doc.kinds();
        }
        let typed = match vk {
            vim::Key::Char(c) => Some(c),
            _ => None,
        };
        let suggest = self.after_insert_key(typed);
        Task::batch([self.after_outcome(outcome), suggest])
    }

    fn after_outcome(&mut self, outcome: Outcome) -> Task<Message> {
        let mut tasks = Vec::new();
        if let Some(register) = outcome.yanked {
            // `y` and `p` use the system clipboard (F-06: unnamedplus).
            self.share_register(&register);
            self.clipboard = Some(register.text.clone());
            tasks.push(iced::clipboard::write(register.text));
        }
        match outcome.request {
            Some(Request::CommandLine) => tasks.push(self.open_prompt(Prompt::Command)),
            Some(Request::Search { forward }) => {
                tasks.push(self.open_prompt(Prompt::Search { forward }))
            }
            None => {}
        }
        Task::batch(tasks)
    }

    /// Every open note pastes the same register.
    fn share_register(&mut self, register: &Register) {
        if let Some(w) = self.workspace_mut() {
            for doc in w.buffers.iter_mut().filter_map(|b| b.doc.as_mut()) {
                doc.vim.set_register(register.clone());
            }
        }
    }

    /// Clipboard text read when the window gained focus: text copied in
    /// another app becomes what `p` pastes (F-06.5).
    pub(super) fn clipboard_changed(&mut self, text: Option<String>) {
        let Some(text) = text else { return };
        if self.clipboard.as_deref() == Some(text.as_str()) {
            return;
        }
        self.clipboard = Some(text.clone());
        let linewise = text.ends_with('\n');
        self.share_register(&Register { text, linewise });
    }

    /// Runs a `/` or `?` search.
    pub(super) fn search(&mut self, pattern: &str, forward: bool) -> Task<Message> {
        if pattern.is_empty() {
            return Task::none();
        }
        self.record_jump();
        let Some(doc) = self.active_document_mut() else {
            return Task::none();
        };
        if !doc.vim.search(&mut doc.buffer, pattern, forward) {
            self.say(Level::Error, format!("Pattern not found: {pattern}"));
        }
        Task::none()
    }

    /// A note's bytes arrived.
    pub(super) fn note_loaded(
        &mut self,
        path: String,
        result: Result<Vec<u8>, String>,
    ) -> Task<Message> {
        let insert_escape = self.config.insert_escape.clone();
        let register = self.clipboard.clone();
        let mut error = None;
        let mut loaded = false;
        if let Some(w) = self.workspace_mut()
            && let Some(b) = w.buffers.iter_mut().find(|b| b.path == path)
        {
            match result {
                // Reopened while its closing save ran: its text is newer.
                _ if b.doc.is_some() => loaded = true,
                Ok(bytes) => {
                    let mut engine = vim::Vim::new(vim::Config { insert_escape });
                    if let Some(text) = register {
                        let linewise = text.ends_with('\n');
                        engine.set_register(Register { text, linewise });
                    }
                    b.doc = Some(Document::new(path.clone(), &bytes, engine));
                    loaded = true;
                }
                Err(e) => error = Some(format!("Can't read \"{path}\": {e}")),
            }
        }
        if let Some(e) = error {
            self.say(Level::Error, e);
        }
        if !loaded {
            return Task::none();
        }
        self.apply_pending_jump();
        self.apply_pending_cursor();
        self.load_ambiguous(path)
    }

    /// Whether the active note's Vim engine waits for a timeout (`j` of an
    /// `insert_escape` like `jk`, F-06.7).
    pub(super) fn waiting_on_vim(&self) -> bool {
        self.active_document()
            .is_some_and(|d| d.vim.deadline().is_some())
    }

    /// Lets the Vim engine act on a timeout that has passed.
    pub(super) fn vim_deadlines(&mut self, now: Instant) -> Task<Message> {
        let Some(doc) = self.active_document_mut() else {
            return Task::none();
        };
        if doc.vim.deadline().is_none_or(|d| d > now) {
            return Task::none();
        }
        let outcome = doc.vim.tick(&mut doc.buffer, now);
        if outcome.changed {
            doc.edited(now);
            doc.kinds();
        }
        self.after_outcome(outcome)
    }

    /// Whether any note has text that is not on disk yet.
    pub(super) fn unsaved(&self) -> bool {
        self.workspace().is_some_and(|w| {
            !w.closing.is_empty()
                || w.buffers
                    .iter()
                    .filter_map(|b| b.doc.as_ref())
                    .any(|d| d.dirty() && d.read_only.is_none())
        })
    }

    /// An open or closing note in the paths `within` whose text is not on
    /// disk and will not get there on its own: waiting on a conflict, or
    /// (with `failed`) after a failed save.
    pub(super) fn unsaved_stuck(
        &self,
        within: impl Fn(&str) -> bool,
        failed: bool,
    ) -> Option<String> {
        let w = self.workspace()?;
        w.buffers
            .iter()
            .filter_map(|b| b.doc.as_ref())
            .chain(&w.closing)
            .find(|d| {
                d.dirty()
                    && d.read_only.is_none()
                    && within(&d.path)
                    && match d.save {
                        Save::Changed => true,
                        Save::Failed { .. } => failed,
                        _ => false,
                    }
            })
            .map(|d| d.path.clone())
    }

    /// Saves every note that needs it (`force`: now, without waiting for
    /// the one-second pause).
    pub(super) fn autosave(&mut self, force: bool) -> Task<Message> {
        self.autosave_at(Instant::now(), force)
    }

    pub(super) fn autosave_at(&mut self, now: Instant, force: bool) -> Task<Message> {
        let Some(w) = self.workspace_mut() else {
            return Task::none();
        };
        if w.moving {
            return Task::none();
        }
        let root = w.session.paths.root.clone();
        let journal = w.journal.clone();
        let jobs: Vec<_> = w
            .buffers
            .iter_mut()
            .filter_map(|b| b.doc.as_mut())
            .filter(|d| d.should_save(now, force))
            .map(|d| d.start_save(&root, &journal))
            .collect();
        Task::batch(
            jobs.into_iter()
                .map(|job| Task::future(async move { Message::Saved(job.run()) })),
        )
    }

    /// A save finished.
    pub(super) fn saved(&mut self, done: SaveDone) -> Task<Message> {
        let reindex = if done.result.is_ok() {
            self.refresh_index(done.path.clone())
        } else {
            Task::none()
        };
        Task::batch([reindex, self.save_landed(done)])
    }

    fn save_landed(&mut self, done: SaveDone) -> Task<Message> {
        let now = Instant::now();
        let mut message = None;
        let mut reread = None;
        let mut next = Task::none();
        if let Some(w) = self.workspace_mut() {
            if let Some(i) = w.closing.iter().position(|d| d.path == done.path) {
                let mut doc = w.closing.remove(i);
                doc.finish_save(&done, now);
                if doc.dirty() && doc.save == Save::Idle {
                    // Typed into before it was closed, after this save
                    // started: save the rest, still closed.
                    let job = doc.start_save(&w.session.paths.root, &w.journal);
                    w.closing.push(doc);
                    next = Task::future(async move { Message::Saved(job.run()) });
                } else if doc.dirty() {
                    // Saving a closed note failed: bring it back, text intact.
                    message = landed_message(&doc);
                    if doc.save == Save::Changed {
                        reread = Some(doc.path.clone());
                    }
                    let id = w.buffers.iter().map(|b| b.id).max().map_or(0, |m| m + 1);
                    w.buffers.push(Buffer {
                        path: doc.path.clone(),
                        doc: Some(doc),
                        backlinks: None,
                        id,
                        used: 0,
                        read: None,
                    });
                    w.active = Some(w.buffers.len() - 1);
                }
            } else if let Some(doc) = w
                .buffers
                .iter_mut()
                .filter_map(|b| b.doc.as_mut())
                .find(|d| d.path == done.path)
            {
                doc.finish_save(&done, now);
                message = landed_message(doc);
                if doc.save == Save::Changed {
                    // The hash on disk differs (F-11.3): read it for the banner.
                    reread = Some(doc.path.clone());
                }
            }
        }
        if let Some((level, text)) = message {
            self.say(level, text);
        }
        if let Some(path) = reread {
            return self.read_disk(path);
        }
        if self.workspace().is_some_and(|w| !w.closing.is_empty()) {
            return next;
        }
        if self.quitting {
            if self.save_failed() {
                // Never quit with text that is not on disk.
                self.quitting = false;
                return Task::none();
            }
            if !self.unsaved() {
                return self.quit(false);
            }
        }
        Task::none()
    }

    fn save_failed(&self) -> bool {
        self.workspace().is_some_and(|w| {
            w.buffers
                .iter()
                .filter_map(|b| b.doc.as_ref())
                .any(|d| matches!(d.save, Save::Failed { .. } | Save::Changed))
        })
    }

    /// A click moves the cursor there; Ctrl+click also follows the link
    /// under it (F-08).
    pub(super) fn click(&mut self, line: usize, col: usize, ctrl: bool) -> Task<Message> {
        if let Some(doc) = self.active_document_mut() {
            doc.buffer.set_cursor(vim::Pos { line, col });
        }
        if ctrl {
            return self.follow_link();
        }
        Task::none()
    }

    /// After the wheel scrolled, the cursor stays inside the visible lines.
    pub(super) fn scrolled(&mut self, first: usize, last: usize) {
        if let Some(doc) = self.active_document_mut() {
            let mut pos = doc.buffer.cursor();
            if pos.line < first {
                pos.line = first;
            } else if pos.line > last {
                pos.line = last;
            } else {
                return;
            }
            doc.buffer.set_cursor(pos);
        }
    }

    /// "Open in default app" on a read-only note.
    pub(super) fn open_externally(&mut self) {
        let Some(w) = self.workspace() else { return };
        let Some(b) = w.active_buffer() else { return };
        let file = w.session.paths.root.join(&b.path);
        if let Err(e) = fonts::external_command("xdg-open").arg(file).spawn() {
            self.say(Level::Error, format!("Can't open the default app: {e}"));
        }
    }

    /// The editor for a buffer.
    pub(super) fn editor_view<'a>(&'a self, b: &'a Buffer) -> Element<'a, Message> {
        let Some(doc) = &b.doc else {
            return iced::widget::container("").into();
        };
        let mode = doc.vim.mode();
        let shape = if self.cmdline.input.is_some() || doc.read_only.is_some() {
            Shape::Hidden
        } else {
            match mode {
                vim::Mode::Insert => Shape::Line(MODE_INSERT),
                vim::Mode::Visual | vim::Mode::VisualLine => Shape::Block(EDITOR_CURSOR_VISUAL),
                vim::Mode::Normal => Shape::Block(MODE_NORMAL),
            }
        };
        let cursor = doc.buffer.cursor();
        let mut editor = Editor::new(
            b.id,
            doc.buffer.lines(),
            doc.cached_kinds(),
            (cursor.line, cursor.col),
            shape,
        )
        .on_click(|(line, col), ctrl| Message::Click(line, col, ctrl))
        .on_scroll(Message::Scrolled);
        editor.selection = doc.vim.selection(&doc.buffer).map(|(a, z)| {
            if mode == vim::Mode::VisualLine {
                ((a.line, 0), (z.line, usize::MAX))
            } else {
                ((a.line, a.col), (z.line, z.col))
            }
        });
        editor.search = doc.vim.last_search();
        editor.drawn = self.bench_clock();
        match self.layout() {
            patinote_core::config::Layout::Classic => editor.max_width = SIZE_EDITOR_MAXW_CLASSIC,
            patinote_core::config::Layout::Focus => {
                editor.max_width = SIZE_EDITOR_MAXW_FOCUS;
                editor.ty = TYPE_FOCUS;
                editor.gutter = false;
                editor.top_pad = SPACE_48 + SPACE_48;
            }
            patinote_core::config::Layout::Minimal => {}
        }
        editor.ambiguous = self.workspace().and_then(|w| w.ambiguous.get(&doc.path));
        if let Some(footer) = self.inline_backlinks() {
            editor = editor.footer(footer);
        }
        match self.popup() {
            Some((col, popup)) => editor.popup(col, popup).into(),
            None => editor.into(),
        }
    }

    /// The banner of the active note, if it needs one (0003 section 3.7).
    pub(super) fn banner(&self) -> Option<Element<'_, Message>> {
        let doc = self.active_document()?;
        let name = doc.path.rsplit('/').next().unwrap_or(&doc.path);
        let banner = if let Some(why) = doc.read_only {
            Banner {
                kind: BannerKind::Info,
                icon: Icon::Lock,
                title: why.title().into(),
                body: match why {
                    ReadOnly::TooLarge => "Patinote shows it but doesn't edit or index it.".into(),
                    ReadOnly::NotUtf8 => "Patinote shows it but doesn't edit it.".into(),
                },
                code: None,
                actions: vec![Action {
                    label: "Open in default app".into(),
                    primary: false,
                    on_press: Message::OpenExternally,
                }],
            }
        } else if let Save::Failed { reason, .. } = &doc.save {
            Banner {
                kind: BannerKind::Error,
                icon: Icon::Error,
                title: format!("Couldn't save {name}: {reason}."),
                body: "Your text is safe in the editor and the file on disk is unchanged. Patinote keeps retrying.".into(),
                code: None,
                actions: vec![Action {
                    label: "Retry now".into(),
                    primary: true,
                    on_press: Message::RetrySave,
                }],
            }
        } else {
            return None;
        };
        // The banner is built per frame; its text is owned by the element.
        Some(owned_banner(banner))
    }

    pub(super) fn status_line(&self) -> StatusLine {
        let Some(w) = self.workspace() else {
            return StatusLine::default();
        };
        let indicator = match &w.index {
            super::IndexStatus::Indexing { done, total } => Some(Indicator::Indexing {
                done: *done,
                total: *total,
            }),
            super::IndexStatus::Ready => Some(Indicator::Indexed),
            super::IndexStatus::Failed => None,
        };
        let indicator = if super::external::polling(w.watch_mode.as_ref())
            && indicator == Some(Indicator::Indexed)
        {
            Some(Indicator::Polling { seconds: 30 })
        } else {
            indicator
        };
        let mut line = StatusLine {
            mode: Mode::Normal,
            indicator,
            conflicts: w.conflicts.len(),
            ..StatusLine::default()
        };
        let Some(b) = w.active_buffer() else {
            return line;
        };
        line.path = b.path.clone();
        if let Some(doc) = &b.doc {
            if b.read.is_some() {
                // Reading mode (mockup 07): its keys, and the word count.
                line.mode = Mode::Read;
                line.notes = vec![
                    "Tab next link · Enter follow · ␣r edit".into(),
                    word_count(&doc.buffer.text()),
                ];
                line.indicator = None;
                return line;
            }
            line.mode = match doc.vim.mode() {
                _ if doc.external.is_some() || self.comparing.is_some() => Mode::Conflict,
                _ if doc.read_only.is_some() => Mode::Read,
                vim::Mode::Normal => Mode::Normal,
                vim::Mode::Insert => Mode::Insert,
                vim::Mode::Visual | vim::Mode::VisualLine => Mode::Visual,
            };
            line.modified = doc.dirty();
            if doc.read_only.is_some() {
                line.notes.push("read-only".into());
            }
            line.notes.push(word_count(&doc.buffer.text()));
            if matches!(doc.save, Save::Failed { .. }) {
                line.indicator = Some(Indicator::NotSaved);
            }
            let c = doc.buffer.cursor();
            line.position = Some(format!("{}:{}", c.line + 1, c.col + 1));
        }
        if let Some(n) = b.backlinks.as_ref().map(Vec::len) {
            line.notes.push(if n == 1 {
                "⇠ 1 backlink".into()
            } else {
                format!("⇠ {n} backlinks")
            });
        }
        if self.layout() == patinote_core::config::Layout::Classic {
            // Classic keeps its panels open; the status line names the layout.
            line.right_note = Some("layout: classic".into());
            return line;
        }
        line.buttons = vec![
            StatusButton {
                label: "␣e explorer",
                on: self.explorer_open,
            },
            StatusButton {
                label: "␣b panel",
                on: self.panel_open,
            },
            StatusButton {
                label: "␣ leader",
                on: false,
            },
        ];
        line
    }
}

/// A banner drawn from a value built for this frame.
fn owned_banner(banner: Banner<Message>) -> Element<'static, Message> {
    banner.into_element()
}

impl Screen {
    /// Whether a note is open, for tests.
    pub fn note_open(&self) -> bool {
        matches!(self, Screen::Vault(w) if w.active.is_some())
    }
}
