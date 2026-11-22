//! Links in the app (F-08): following them (`gd`, `Enter`, Ctrl+click,
//! `gx`), the picker for ambiguous links, `[[` autocomplete, the ambiguous
//! link underline, and refreshing the index after a save so new links count
//! as backlinks right away.

use std::collections::{HashMap, HashSet};

use iced::keyboard::{Key, Modifiers, key::Named};
use iced::{Element, Task};
use patinote_core::model::LinkKind;
use patinote_core::resolve::missing_note_path;
use patinote_core::{IndexOptions, Resolver};
use patinote_index::update_index;

use super::{App, Message};
use crate::editor::document::Document;
use crate::fonts;
use crate::links::{Link, Suggest, link_at, link_name, suggest_at};
use crate::screens::popups::{self, Choice, Suggestion};
use crate::state;
use crate::switcher::{Candidate, Matched};
use crate::theme::styles::Level;
use crate::vim;

/// Where a followed link leads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Followed {
    /// A note, then the heading or `^block` to show.
    Note(String, Option<String>),
    /// A file that is not a note: opened with the default app.
    Attachment(String),
    /// Several notes match: (path, modified ns).
    Ambiguous {
        name: String,
        choices: Vec<(String, i64)>,
        subpath: Option<String>,
    },
    /// Nothing matches: a new note at this path.
    Missing(String),
    /// Nothing matches, and a new note would be outside the vault.
    Outside,
}

/// The open link suggestions.
#[derive(Debug, Clone, Default)]
pub struct Autocomplete {
    /// Column where the link's text starts, right after `[[`.
    pub start: usize,
    pub items: Vec<Suggestion>,
    /// What each item inserts.
    pub inserts: Vec<String>,
    pub selected: usize,
    /// The text typed after `[[`, for the "new note" line.
    pub typed: String,
}

/// The open picker for an ambiguous link (mockup 12).
#[derive(Debug, Clone)]
pub struct Picker {
    pub name: String,
    pub paths: Vec<String>,
    pub choices: Vec<Choice>,
    pub selected: usize,
    pub subpath: Option<String>,
    /// Column where the link starts, to hang the picker under it.
    pub col: usize,
}

impl App {
    /// Whether link suggestions are open, for tests.
    pub fn suggestions_open(&self) -> bool {
        self.autocomplete.is_some()
    }

    /// What each open suggestion would insert, for tests.
    pub fn suggestion_inserts(&self) -> Vec<String> {
        self.autocomplete
            .as_ref()
            .map(|a| a.inserts.clone())
            .unwrap_or_default()
    }

    /// The notes the ambiguous link picker offers, for tests.
    pub fn picker_choices(&self) -> Vec<String> {
        self.picker
            .as_ref()
            .map(|p| p.paths.clone())
            .unwrap_or_default()
    }

    /// Ambiguous link targets of the active note, sorted, for tests.
    pub fn ambiguous_targets(&self) -> Vec<String> {
        let (Some(w), Some(d)) = (self.workspace(), self.active_document()) else {
            return Vec::new();
        };
        let mut out: Vec<String> = w
            .ambiguous
            .get(&d.path)
            .map(|s| s.iter().cloned().collect())
            .unwrap_or_default();
        out.sort();
        out
    }

    /// `gd`, `Enter` on a link, Ctrl+click: opens where the link under the
    /// cursor points.
    pub(super) fn follow_link(&mut self) -> Task<Message> {
        let Some((link, col, source)) = self.link_under_cursor() else {
            return Task::none();
        };
        match link {
            Link::Url(url) => self.open_externally_path(&url),
            Link::Wiki { target, subpath } if target.is_empty() => {
                // `[[#Heading]]` points inside this note.
                self.record_jump();
                self.jump_to(subpath.as_deref());
                Task::none()
            }
            Link::Wiki { target, subpath } => {
                self.resolve_link(source, target, LinkKind::Wiki, subpath, col)
            }
            Link::Markdown { target, subpath } => {
                self.resolve_link(source, target, LinkKind::Markdown, subpath, col)
            }
        }
    }

    /// `gx`: opens a web address, or a linked file, with the default app.
    pub(super) fn open_link_externally(&mut self) -> Task<Message> {
        match self.link_under_cursor() {
            Some((Link::Url(url), _, _)) => self.open_externally_path(&url),
            Some(_) => self.follow_link(),
            None => Task::none(),
        }
    }

    fn open_externally_path(&mut self, target: &str) -> Task<Message> {
        if let Err(e) = fonts::external_command("xdg-open").arg(target).spawn() {
            self.say(Level::Error, format!("Can't open the default app: {e}"));
        }
        Task::none()
    }

    fn link_under_cursor(&self) -> Option<(Link, usize, String)> {
        let doc = self.active_document()?;
        let c = doc.buffer.cursor();
        let line = doc.buffer.lines().get(c.line)?;
        let (link, col) = link_at(line, c.col)?;
        Some((link, col, doc.path.clone()))
    }

    pub(super) fn resolve_link(
        &mut self,
        source: String,
        target: String,
        kind: LinkKind,
        subpath: Option<String>,
        col: usize,
    ) -> Task<Message> {
        let Some(w) = self.workspace() else {
            return Task::none();
        };
        let name = target.clone();
        Task::perform(
            w.session.query(move |store| {
                let found = store.link_candidates(&target).unwrap_or_default();
                let resolver = Resolver::new(found);
                match resolver.resolve(&source, kind, &target) {
                    None => match missing_note_path(&source, kind, &target) {
                        Ok(path) => Followed::Missing(path.unwrap_or_default()),
                        Err(_) => Followed::Outside,
                    },
                    Some(r) if r.is_ambiguous() => Followed::Ambiguous {
                        name: target.clone(),
                        choices: r
                            .candidates
                            .iter()
                            .map(|p| {
                                let mtime = store
                                    .note_by_path(p)
                                    .ok()
                                    .flatten()
                                    .map_or(0, |n| n.mtime_ns);
                                ((*p).to_owned(), mtime)
                            })
                            .collect(),
                        subpath: subpath.clone(),
                    },
                    Some(r) if r.path.to_lowercase().ends_with(".md") => {
                        Followed::Note(r.path.to_owned(), subpath.clone())
                    }
                    Some(r) => Followed::Attachment(r.path.to_owned()),
                }
            }),
            move |f| {
                Message::Followed(
                    name.clone(),
                    col,
                    f.unwrap_or(Followed::Missing(String::new())),
                )
            },
        )
    }

    /// Acts on a resolved link.
    pub(super) fn followed(
        &mut self,
        name: String,
        col: usize,
        followed: Followed,
    ) -> Task<Message> {
        match followed {
            Followed::Note(path, subpath) => {
                self.pending_jump = subpath.map(|s| (path.clone(), s));
                let open = self.open_note(path);
                self.apply_pending_jump();
                open
            }
            Followed::Attachment(path) => {
                let Some(full) = self
                    .workspace()
                    .map(|w| patinote_core::names::on_disk(&w.session.paths.root, &path))
                else {
                    return Task::none();
                };
                self.open_externally_path(&full.to_string_lossy())
            }
            Followed::Missing(path) if path.is_empty() => Task::none(),
            Followed::Outside => {
                self.say(Level::Error, format!("\"{name}\" points outside the vault"));
                Task::none()
            }
            // A note that does not exist yet opens empty and is written only
            // once it has text (F-08.6).
            Followed::Missing(path) => self.open_new_note(path, false),
            Followed::Ambiguous {
                name: _,
                choices,
                subpath,
            } => {
                let now = state::now();
                let all: Vec<String> = choices.iter().map(|(p, _)| p.clone()).collect();
                let precise = link_name(&all[0], all.iter().map(String::as_str));
                self.say(
                    Level::Warning,
                    format!(
                        "{} notes are named \"{name}\". Pick one, or write the folder in the link.",
                        choices.len()
                    ),
                );
                self.picker = Some(Picker {
                    name: name.clone(),
                    choices: choices
                        .iter()
                        .map(|(p, mtime)| {
                            let (folder, file) = p
                                .rsplit_once('/')
                                .map_or((String::new(), p.as_str()), |(d, f)| (format!("{d}/"), f));
                            Choice {
                                title: file.strip_suffix(".md").unwrap_or(file).to_owned(),
                                folder,
                                edited: popups::edited(*mtime, now),
                            }
                        })
                        .collect(),
                    paths: all,
                    selected: 0,
                    subpath,
                    col,
                });
                let _ = precise;
                Task::none()
            }
        }
    }

    /// Opens a note that is not on disk (`insert`: in insert mode).
    pub(super) fn open_new_note(&mut self, path: String, insert: bool) -> Task<Message> {
        // A note by that name already on disk (excluded ones included)
        // opens as it is; an empty buffer over it would overwrite it.
        if self
            .workspace()
            .is_some_and(|w| patinote_core::names::on_disk(&w.session.paths.root, &path).is_file())
        {
            return self.open_note(path);
        }
        if let Some(i) = self
            .workspace()
            .and_then(|w| w.buffers.iter().position(|b| b.path == path))
        {
            return self.select_buffer(i);
        }
        // Closed while its first save runs: that note comes back, not a
        // second, empty one whose save would then be refused.
        if self
            .workspace()
            .is_some_and(|w| w.closing.iter().any(|d| d.path == path))
        {
            let open = self.open_note(path);
            if insert
                && let Some(doc) = self.active_document_mut()
                && doc.vim.mode() == vim::Mode::Normal
            {
                doc.vim.handle(
                    &mut doc.buffer,
                    vim::Key::Char('i'),
                    std::time::Instant::now(),
                    1,
                );
            }
            return open;
        }
        let leave = self.autosave(true);
        let insert_escape = self.config.insert_escape.clone();
        let Some(w) = self.workspace_mut() else {
            return Task::none();
        };
        let mut doc =
            Document::new_note(path.clone(), vim::Vim::new(vim::Config { insert_escape }));
        if insert {
            doc.vim.handle(
                &mut doc.buffer,
                vim::Key::Char('i'),
                std::time::Instant::now(),
                1,
            );
        }
        let id = w.buffers.iter().map(|b| b.id).max().map_or(0, |m| m + 1);
        w.buffers.push(super::Buffer {
            path: path.clone(),
            doc: Some(doc),
            backlinks: Some(Vec::new()),
            id,
            used: 0,
            read: None,
        });
        w.active = Some(w.buffers.len() - 1);
        w.state.touch(&path, state::now());
        Task::batch([leave, self.save_vault_state()])
    }

    /// Moves the cursor of the active note to a heading or `^block`.
    pub(super) fn jump_to(&mut self, subpath: Option<&str>) {
        let Some(subpath) = subpath else { return };
        let Some(doc) = self.active_document_mut() else {
            return;
        };
        let text = doc.buffer.text();
        let parsed = patinote_core::parse(&text);
        let line = match subpath.strip_prefix('^') {
            Some(id) => parsed
                .blocks
                .iter()
                .find(|b| b.block_id.eq_ignore_ascii_case(id))
                .map(|b| b.line),
            None => {
                let want = slug(subpath);
                parsed
                    .headings
                    .iter()
                    .find(|h| slug(&h.text) == want)
                    .map(|h| h.line)
            }
        };
        if let Some(line) = line {
            doc.buffer
                .set_cursor(vim::Pos::new(line.saturating_sub(1), 0));
        }
    }

    /// A jump waiting for its note to load.
    pub(super) fn apply_pending_jump(&mut self) {
        let Some((path, subpath)) = self.pending_jump.clone() else {
            return;
        };
        if self.active_document().is_some_and(|d| d.path == path) {
            self.pending_jump = None;
            self.jump_to(Some(&subpath));
        }
    }

    /// Keys while the ambiguous link picker is open; `None` when closed.
    pub(super) fn picker_key(
        &mut self,
        key: &Key,
        text: Option<&str>,
        modifiers: Modifiers,
    ) -> Option<Task<Message>> {
        let p = self.picker.as_mut()?;
        let n = p.choices.len();
        let ctrl = |c: &str| modifiers.control() && *key == Key::Character(c.into());
        if *key == Key::Named(Named::Escape) {
            self.picker = None;
            return Some(Task::none());
        }
        if ctrl("n") || ctrl("j") || *key == Key::Named(Named::ArrowDown) || text == Some("j") {
            p.selected = (p.selected + 1) % n.max(1);
        } else if ctrl("p") || ctrl("k") || *key == Key::Named(Named::ArrowUp) || text == Some("k")
        {
            p.selected = (p.selected + n.max(1) - 1) % n.max(1);
        } else if *key == Key::Named(Named::Enter) {
            let i = p.selected;
            return Some(self.pick(i));
        }
        Some(Task::none())
    }

    /// Opens choice `i` of the picker.
    pub(super) fn pick(&mut self, i: usize) -> Task<Message> {
        let Some(p) = self.picker.take() else {
            return Task::none();
        };
        let Some(path) = p.paths.get(i).cloned() else {
            return Task::none();
        };
        self.cmdline.message = None;
        self.followed(p.name, p.col, Followed::Note(path, p.subpath))
    }

    // --- Autocomplete --------------------------------------------------------

    /// After a key in insert mode: closes `[[` with `]]`, and opens, updates
    /// or closes the suggestions (F-08).
    pub(super) fn after_insert_key(&mut self, typed: Option<char>) -> Task<Message> {
        let Some(doc) = self.active_document_mut() else {
            self.autocomplete = None;
            return Task::none();
        };
        if doc.vim.mode() != vim::Mode::Insert {
            self.autocomplete = None;
            return Task::none();
        }
        let c = doc.buffer.cursor();
        let line = doc.buffer.lines().get(c.line).cloned().unwrap_or_default();
        let chars: Vec<char> = line.chars().collect();
        // Typing `[[` adds `]]` after the cursor.
        if typed == Some('[')
            && c.col >= 2
            && chars[c.col - 2..c.col] == ['[', '[']
            && chars.get(c.col..c.col + 2) != Some(&[']', ']'][..])
        {
            doc.vim
                .insert_text(&mut doc.buffer, c.col, c.col, "]]", c.col);
            doc.edited(std::time::Instant::now());
            doc.kinds();
        }
        let line = doc.buffer.lines().get(c.line).cloned().unwrap_or_default();
        let Some((suggest, start)) = suggest_at(&line, c.col) else {
            self.autocomplete = None;
            return Task::none();
        };
        self.update_suggestions(suggest, start)
    }

    fn update_suggestions(&mut self, suggest: Suggest, start: usize) -> Task<Message> {
        let dismissed = self.dismissed;
        let recent: HashMap<String, usize> = self
            .workspace()
            .map(|w| {
                w.state
                    .recent
                    .iter()
                    .enumerate()
                    .map(|(i, r)| (r.path.clone(), i))
                    .collect()
            })
            .unwrap_or_default();
        let Some(w) = self.workspace_mut() else {
            return Task::none();
        };
        match suggest {
            Suggest::Notes { query, embed } => {
                let pool: Vec<Candidate> = if embed {
                    w.candidates
                        .iter()
                        .chain(w.attachments.iter())
                        .cloned()
                        .collect()
                } else {
                    w.candidates.clone()
                };
                let results = w.finder.find(&pool, &query, &recent);
                let all_paths: Vec<&str> = w.candidates.iter().map(|c| c.path.as_str()).collect();
                let mut items = Vec::new();
                let mut inserts = Vec::new();
                for hit in results.hits.iter().take(SUGGESTIONS_SHOWN) {
                    let c = &pool[hit.candidate];
                    let name = if c.path.ends_with(".md") {
                        link_name(&c.path, all_paths.iter().copied())
                    } else {
                        c.path.rsplit('/').next().unwrap_or(&c.path).to_owned()
                    };
                    match &hit.matched {
                        Matched::Title(idx) => {
                            items.push(Suggestion::Note {
                                text: c.title.clone(),
                                matched: idx.clone(),
                                alias_of: None,
                                folder: c.folder.clone(),
                            });
                            inserts.push(name);
                        }
                        Matched::Alias(n, idx) => {
                            items.push(Suggestion::Note {
                                text: c.aliases[*n].clone(),
                                matched: idx.clone(),
                                alias_of: Some(c.title.clone()),
                                folder: c.folder.clone(),
                            });
                            // An alias becomes the display text (F-08.2).
                            inserts.push(format!("{name}|{}", c.aliases[*n]));
                        }
                    }
                }
                let typed = query;
                self.autocomplete = Some(Autocomplete {
                    start,
                    items,
                    inserts,
                    selected: 0,
                    typed,
                });
                Task::none()
            }
            Suggest::Headings { note, query } | Suggest::Blocks { note, query }
                if note.is_empty() =>
            {
                // `[[#` points inside this note: use its own text.
                let _ = query;
                self.autocomplete = None;
                Task::none()
            }
            Suggest::Headings { note, query } => {
                let typed = format!("{note}#{query}");
                Task::perform(
                    w.session.query(move |store| {
                        let found = store.link_candidates(&note).unwrap_or_default();
                        let path = Resolver::new(found)
                            .resolve("", LinkKind::Wiki, &note)
                            .map(|r| r.path.to_owned());
                        path.and_then(|p| store.note_by_path(&p).ok().flatten())
                            .and_then(|row| store.headings(row.id).ok())
                            .unwrap_or_default()
                            .into_iter()
                            .filter(|h| h.text.to_lowercase().contains(&query.to_lowercase()))
                            .map(|h| (h.level, h.text))
                            .collect::<Vec<_>>()
                    }),
                    move |h| {
                        Message::Headings(dismissed, start, typed.clone(), h.unwrap_or_default())
                    },
                )
            }
            Suggest::Blocks { note, query } => {
                let typed = format!("{note}#^{query}");
                Task::perform(
                    w.session.query(move |store| {
                        let found = store.link_candidates(&note).unwrap_or_default();
                        let path = Resolver::new(found)
                            .resolve("", LinkKind::Wiki, &note)
                            .map(|r| r.path.to_owned());
                        path.and_then(|p| store.note_by_path(&p).ok().flatten())
                            .and_then(|row| store.blocks(row.id).ok())
                            .unwrap_or_default()
                            .into_iter()
                            .filter(|(id, _)| id.to_lowercase().starts_with(&query.to_lowercase()))
                            .map(|(id, _)| (0, id))
                            .collect::<Vec<_>>()
                    }),
                    move |b| {
                        Message::Headings(dismissed, start, typed.clone(), b.unwrap_or_default())
                    },
                )
            }
        }
    }

    /// Heading (level 1–6) or block (level 0) suggestions arrived.
    pub(super) fn headings_loaded(
        &mut self,
        start: usize,
        typed: String,
        found: Vec<(u8, String)>,
    ) {
        // Only if the user is still typing that link.
        let still = self.active_document().is_some_and(|d| {
            let c = d.buffer.cursor();
            d.vim.mode() == vim::Mode::Insert
                && d.buffer
                    .lines()
                    .get(c.line)
                    .and_then(|l| suggest_at(l, c.col))
                    .is_some_and(|(suggest, s)| {
                        // The same link: another one at the same column
                        // (on another line) asked for other headings.
                        let now = match suggest {
                            Suggest::Headings { note, query } => format!("{note}#{query}"),
                            Suggest::Blocks { note, query } => format!("{note}#^{query}"),
                            Suggest::Notes { .. } => return false,
                        };
                        s == start && now == typed
                    })
        });
        if !still {
            return;
        }
        let (note, _) = typed.split_once('#').unwrap_or((&typed, ""));
        let mut items = Vec::new();
        let mut inserts = Vec::new();
        for (level, text) in found.into_iter().take(SUGGESTIONS_SHOWN) {
            if level == 0 {
                inserts.push(format!("{note}#^{text}"));
                items.push(Suggestion::Block(text));
            } else {
                inserts.push(format!("{note}#{text}"));
                items.push(Suggestion::Heading { level, text });
            }
        }
        self.autocomplete = Some(Autocomplete {
            start,
            items,
            inserts,
            selected: 0,
            typed,
        });
    }

    /// The user closed the suggestions: answers still on their way are
    /// dropped, so they do not open them again.
    pub(super) fn dismiss_suggestions(&mut self) {
        self.autocomplete = None;
        self.dismissed += 1;
    }

    /// Keys while suggestions are open; `None` lets the key go to Vim.
    pub(super) fn autocomplete_key(
        &mut self,
        key: &Key,
        modifiers: Modifiers,
    ) -> Option<Task<Message>> {
        let a = self.autocomplete.as_mut()?;
        let n = a.items.len();
        let ctrl = |c: &str| modifiers.control() && *key == Key::Character(c.into());
        if *key == Key::Named(Named::Escape) {
            // The first Esc closes the suggestions, the second leaves insert.
            self.dismiss_suggestions();
            return Some(Task::none());
        }
        if ctrl("n") || *key == Key::Named(Named::ArrowDown) {
            if n > 0 {
                a.selected = (a.selected + 1) % n;
            }
            return Some(Task::none());
        }
        if ctrl("p") || *key == Key::Named(Named::ArrowUp) {
            if n > 0 {
                a.selected = (a.selected + n - 1) % n;
            }
            return Some(Task::none());
        }
        if matches!(key, Key::Named(Named::Enter | Named::Tab)) {
            let i = a.selected;
            return Some(self.accept_suggestion(i));
        }
        None
    }

    /// Inserts suggestion `i`, or with no suggestions the typed text as a
    /// link to a new note; the cursor goes after `]]`.
    pub(super) fn accept_suggestion(&mut self, i: usize) -> Task<Message> {
        let Some(a) = self.autocomplete.take() else {
            return Task::none();
        };
        let Some(doc) = self.active_document_mut() else {
            return Task::none();
        };
        let c = doc.buffer.cursor();
        let insert = a.inserts.get(i).cloned().unwrap_or_else(|| a.typed.clone());
        let line = doc.buffer.lines().get(c.line).cloned().unwrap_or_default();
        let chars: Vec<char> = line.chars().collect();
        // Replace what was typed after `[[`, up to the closing `]]`.
        let end = (c.col..chars.len().saturating_sub(1))
            .find(|&j| chars[j] == ']' && chars[j + 1] == ']')
            .unwrap_or(c.col);
        let after = a.start + insert.chars().count();
        let closed = chars.get(end..end + 2) == Some(&[']', ']'][..]);
        let text = if closed {
            insert
        } else {
            format!("{insert}]]")
        };
        doc.vim
            .insert_text(&mut doc.buffer, a.start, end, &text, after + 2);
        doc.edited(std::time::Instant::now());
        doc.kinds();
        Task::none()
    }

    /// The popup under the cursor, if any.
    pub(super) fn popup(&self) -> Option<(usize, Element<'_, Message>)> {
        if let Some(a) = &self.autocomplete {
            return Some((
                a.start.saturating_sub(2),
                popups::suggestions(&a.items, a.selected, &a.typed, Message::Suggest),
            ));
        }
        let p = self.picker.as_ref()?;
        let precise = link_name(
            &p.paths[p.selected.min(p.paths.len() - 1)],
            p.paths.iter().map(String::as_str),
        );
        Some((
            p.col,
            popups::picker(&p.name, &p.choices, p.selected, &precise, Message::Pick),
        ))
    }

    // --- Ambiguous links and index refresh ------------------------------------

    /// Which link targets in the note at `path` match more than one note.
    pub(super) fn load_ambiguous(&self, path: String) -> Task<Message> {
        let Some(w) = self.workspace() else {
            return Task::none();
        };
        let p = path.clone();
        Task::perform(
            w.session.query(move |store| {
                let Some(row) = store.note_by_path(&p).ok().flatten() else {
                    return HashSet::new();
                };
                let mut out = HashSet::new();
                let mut seen = HashSet::new();
                for link in store.outgoing(row.id).unwrap_or_default() {
                    if link.kind != "wiki" && link.kind != "embed" {
                        continue;
                    }
                    let key = link.target.trim().to_lowercase();
                    if !seen.insert(key.clone()) {
                        continue;
                    }
                    let found = store.link_candidates(&link.target).unwrap_or_default();
                    if Resolver::new(found)
                        .resolve(&p, LinkKind::Wiki, &link.target)
                        .is_some_and(|r| r.is_ambiguous())
                    {
                        out.insert(key);
                    }
                }
                out
            }),
            move |set| Message::Ambiguous(path.clone(), set.unwrap_or_default()),
        )
    }

    /// After a save: brings the index up to date (a quick check, F-03.8), so
    /// a link written now counts in backlinks at once (F-08.7); then refreshes
    /// what depends on it.
    pub(super) fn refresh_index(&self, path: String) -> Task<Message> {
        let Some(w) = self.workspace() else {
            return Task::none();
        };
        let root = w.session.paths.root.clone();
        Task::perform(
            w.session.query(move |store| {
                let _ = update_index(store, &root, &IndexOptions::default(), &mut |_| {});
            }),
            move |_| Message::Reindexed(path.clone()),
        )
    }
}

/// How many suggestions show at once.
const SUGGESTIONS_SHOWN: usize = 8;

/// Obsidian matches headings ignoring case and runs of spaces.
fn slug(s: &str) -> String {
    s.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}
