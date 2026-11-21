//! Managing notes and folders (F-10): the explorer, new notes and folders,
//! deleting to `.trash/`, and the dialogs. Rename and move, which update
//! links, are in `app/renaming.rs`.

use std::collections::{BTreeSet, HashSet};

use iced::keyboard::{Key, Modifiers, key::Named};
use iced::widget::{container, row, stack};
use iced::{Element, Length, Padding, Task};
use patinote_core::write::{Actor, Op, Transaction, commit, expect_current};

use super::{App, Message};
use crate::explorer::{self, Kind, NameCheck, Tree};
use crate::screens::dialogs::{self, LinkUpdate};
use crate::screens::explorer::{self as view, Editing};
use crate::theme::styles::{self, Field, Level};
use crate::theme::tokens::*;

/// What the explorer's inline input is for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InlineFor {
    /// A new note in this folder (`a`).
    Note(String),
    /// A new folder in this folder (`A`).
    Folder(String),
    /// A new name for this folder (`r` on a folder).
    RenameFolder(String),
}

/// The explorer's state.
#[derive(Debug, Clone, Default)]
pub struct ExplorerState {
    pub tree: Tree,
    pub selected: Option<usize>,
    pub editing: Option<(InlineFor, Editing)>,
    /// Folders created in this session that have no notes yet.
    pub new_folders: BTreeSet<String>,
    /// (tag, uses, depth) for the tag list.
    pub tags: Vec<(String, usize, usize)>,
}

/// An open dialog.
#[derive(Debug, Clone)]
pub enum Dialog {
    /// `<leader>n` (mockup 14). `warned`: Enter was pressed once on a risky
    /// name, so the next Enter creates it anyway.
    NewNote {
        folder: String,
        value: String,
        warned: bool,
    },
    /// `r` on a note (mockup 09).
    Rename {
        path: String,
        value: String,
        updates: Vec<LinkUpdate>,
        notes: usize,
    },
    /// `d` on a note (mockup 09).
    DeleteNote { path: String, linking: usize },
    /// `d` on a folder (mockup 14).
    DeleteFolder {
        path: String,
        notes: usize,
        attachments: usize,
        outside: usize,
    },
    /// `m` (mockup 14).
    Move {
        path: String,
        query: String,
        folders: Vec<(String, Vec<usize>)>,
        selected: usize,
        path_links: usize,
    },
}

/// The problem with a typed name, for the input's look and message.
pub fn name_problem(value: &str) -> Option<(Field, String)> {
    match explorer::check_name(value) {
        NameCheck::Ok => None,
        NameCheck::Warning(c) => Some((Field::Warning, explorer::warning(c))),
        NameCheck::Rejected => Some((Field::Rejected, explorer::REJECTED.into())),
    }
}

/// Tags with a row for each parent of a nested tag (`#proyek` above
/// `/patinote`, mockup 02); a parent that is not used on its own counts the
/// uses of the tags under it.
pub fn with_parents(tags: Vec<(String, usize, usize)>) -> Vec<(String, usize, usize)> {
    let mut map: std::collections::BTreeMap<String, (String, usize)> =
        std::collections::BTreeMap::new();
    let mut own: HashSet<String> = HashSet::new();
    for (tag, uses, _) in &tags {
        own.insert(tag.to_lowercase());
        let e = map
            .entry(tag.to_lowercase())
            .or_insert_with(|| (tag.clone(), 0));
        e.1 += uses;
    }
    for (tag, uses, _) in &tags {
        let mut parts: Vec<&str> = tag.split('/').collect();
        while parts.len() > 1 {
            parts.pop();
            let parent = parts.join("/");
            if own.contains(&parent.to_lowercase()) {
                break;
            }
            let e = map
                .entry(parent.to_lowercase())
                .or_insert_with(|| (parent.clone(), 0));
            e.1 += uses;
        }
    }
    map.into_values()
        .map(|(tag, uses)| {
            let depth = tag.matches('/').count();
            (tag, uses, depth)
        })
        .collect()
}

fn join(folder: &str, name: &str) -> String {
    if folder.is_empty() {
        name.to_owned()
    } else {
        format!("{folder}/{name}")
    }
}

fn folder_of(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(d, _)| d)
}

impl App {
    /// Rebuilds the tree from the index's files and the session's new folders.
    pub(super) fn rebuild_tree(&mut self) {
        let Some(w) = self.workspace_mut() else {
            return;
        };
        let files: Vec<&str> = w
            .candidates
            .iter()
            .chain(w.attachments.iter())
            .map(|c| c.path.as_str())
            .collect();
        let open = w.explorer.tree.open_folders().clone();
        let extra: Vec<&str> = w.explorer.new_folders.iter().map(String::as_str).collect();
        w.explorer.tree = Tree::new(files, extra, open);
    }

    /// The explorer's rows, for tests.
    pub fn explorer_rows(&self) -> Vec<String> {
        self.workspace()
            .map(|w| w.explorer.tree.rows().into_iter().map(|r| r.path).collect())
            .unwrap_or_default()
    }

    /// Selects the explorer row of `path` (opening its folders), for tests.
    pub fn select_in_explorer(&mut self, path: &str) {
        let Some(w) = self.workspace_mut() else {
            return;
        };
        let mut dir = folder_of(path);
        while !dir.is_empty() {
            w.explorer.tree.set_open(dir, true);
            dir = folder_of(dir);
        }
        w.explorer.selected = w.explorer.tree.rows().iter().position(|r| r.path == path);
    }

    /// Whether the explorer is open, for tests.
    pub fn explorer_is_open(&self) -> bool {
        self.explorer_open
    }

    /// Whether a dialog is open, for tests.
    pub fn dialog_open(&self) -> bool {
        self.dialog.is_some()
    }

    /// `<leader>e`: opens the explorer with the focus in it, or closes it.
    pub(super) fn toggle_explorer(&mut self) -> Task<Message> {
        self.explorer_open = !self.explorer_open;
        self.explorer_focus = self.explorer_open;
        self.focus = None;
        self.remember_panels();
        let save = self.save_vault_state();
        if self.explorer_open {
            self.rebuild_tree();
            // Select the active note's row, opening its folders.
            let active = self.active_document().map(|d| d.path.clone());
            if let Some(w) = self.workspace_mut() {
                if let Some(path) = &active {
                    let mut dir = folder_of(path);
                    while !dir.is_empty() {
                        w.explorer.tree.set_open(dir, true);
                        dir = folder_of(dir);
                    }
                }
                w.explorer.selected = active
                    .and_then(|p| w.explorer.tree.rows().iter().position(|r| r.path == p))
                    .or(Some(0));
            }
            return Task::batch([save, self.load_tags()]);
        }
        self.release_explorer();
        save
    }

    /// A closed explorer drops its rows and tags (F-28.3); only which
    /// folders were open is kept.
    pub(super) fn release_explorer(&mut self) {
        if let Some(w) = self.workspace_mut() {
            let open = w.explorer.tree.open_folders().clone();
            w.explorer.tree = Tree::new(Vec::new(), Vec::new(), open);
            w.explorer.tags = Vec::new();
            w.explorer.selected = None;
        }
    }

    pub(super) fn load_tags(&self) -> Task<Message> {
        let Some(w) = self.workspace() else {
            return Task::none();
        };
        Task::perform(
            w.session.query(|store| {
                store
                    .tag_counts()
                    .unwrap_or_default()
                    .into_iter()
                    .map(|t| {
                        let depth = t.tag.matches('/').count();
                        (t.display, t.uses, depth)
                    })
                    .collect::<Vec<_>>()
            }),
            |t| Message::Tags(t.unwrap_or_default()),
        )
    }

    pub(super) fn tags_loaded(&mut self, tags: Vec<(String, usize, usize)>) {
        if let Some(w) = self.workspace_mut() {
            w.explorer.tags = with_parents(tags);
        }
    }

    fn selected_row(&self) -> Option<explorer::Row> {
        let w = self.workspace()?;
        let i = w.explorer.selected?;
        w.explorer.tree.rows().get(i).cloned()
    }

    /// The folder new notes go to: the selected folder, or the selected
    /// note's folder, or the vault root.
    fn target_folder(&self) -> String {
        match self.selected_row() {
            Some(r) if matches!(r.kind, Kind::Folder { .. }) => r.path,
            Some(r) => folder_of(&r.path).to_owned(),
            None => String::new(),
        }
    }

    /// Keys while the explorer has the focus (F-10 table).
    pub(super) fn explorer_key(
        &mut self,
        key: &Key,
        text: Option<&str>,
        modifiers: Modifiers,
    ) -> Task<Message> {
        if modifiers.control() && *key == Key::Character("w".into()) {
            self.window_prefix = true;
            return Task::none();
        }
        let rows = self
            .workspace()
            .map(|w| w.explorer.tree.rows())
            .unwrap_or_default();
        let n = rows.len();
        // Actions need a selected row; with none, only movement works, so a
        // stale selection can never rename, move or delete the wrong file.
        let picked = self
            .workspace()
            .and_then(|w| w.explorer.selected)
            .filter(|&i| i < n);
        let sel = picked.unwrap_or(0);
        let set = |app: &mut App, i: usize| {
            if let Some(w) = app.workspace_mut() {
                w.explorer.selected = Some(i);
            }
        };
        match (key, text) {
            (Key::Named(Named::Escape), _) => self.explorer_focus = false,
            (Key::Named(Named::ArrowDown), _) | (_, Some("j")) if n > 0 => {
                set(self, (sel + 1).min(n - 1))
            }
            (Key::Named(Named::ArrowUp), _) | (_, Some("k")) if n > 0 => {
                set(self, sel.saturating_sub(1))
            }
            (Key::Named(Named::Enter), _) | (_, Some("l")) => {
                let Some(r) = picked.and_then(|i| rows.get(i)).cloned() else {
                    return Task::none();
                };
                if let Kind::Folder { open } = r.kind {
                    if (!open || text != Some("l"))
                        && let Some(w) = self.workspace_mut()
                    {
                        w.explorer.tree.set_open(&r.path, !open);
                    }
                    return Task::none();
                }
                self.explorer_focus = false;
                return if r.kind == Kind::Note {
                    self.open_note(r.path)
                } else {
                    self.open_externally_file(&r.path)
                };
            }
            (_, Some("h")) => {
                let Some(r) = picked.and_then(|i| rows.get(i)).cloned() else {
                    return Task::none();
                };
                // Close the folder, or go up to the folder of this row.
                if matches!(r.kind, Kind::Folder { open: true }) {
                    if let Some(w) = self.workspace_mut() {
                        w.explorer.tree.set_open(&r.path, false);
                    }
                } else {
                    let up = folder_of(&r.path).to_owned();
                    if let Some(i) = rows.iter().position(|x| x.path == up) {
                        if let Some(w) = self.workspace_mut() {
                            w.explorer.tree.set_open(&up, false);
                        }
                        set(self, i);
                    }
                }
            }
            (_, Some("a")) => return self.start_inline(InlineFor::Note(self.target_folder())),
            (_, Some("A")) => return self.start_inline(InlineFor::Folder(self.target_folder())),
            (_, Some("r")) => {
                return match picked.and_then(|i| rows.get(i)).cloned() {
                    Some(r) if matches!(r.kind, Kind::Folder { .. }) => {
                        self.start_inline(InlineFor::RenameFolder(r.path))
                    }
                    Some(r) => self.open_rename(r.path),
                    None => Task::none(),
                };
            }
            (_, Some("m")) => {
                return match picked.and_then(|i| rows.get(i)).cloned() {
                    Some(r) => self.open_move(r.path),
                    None => Task::none(),
                };
            }
            (_, Some("d")) => {
                return match picked.and_then(|i| rows.get(i)).cloned() {
                    Some(r) => self.open_delete(r.path, matches!(r.kind, Kind::Folder { .. })),
                    None => Task::none(),
                };
            }
            _ => {}
        }
        Task::none()
    }

    fn open_externally_file(&mut self, path: &str) -> Task<Message> {
        let Some(full) = self.workspace().map(|w| w.session.paths.root.join(path)) else {
            return Task::none();
        };
        if let Err(e) = crate::fonts::external_command("xdg-open").arg(full).spawn() {
            self.say(Level::Error, format!("Can't open the default app: {e}"));
        }
        Task::none()
    }

    // --- Inline input in the tree --------------------------------------------

    fn start_inline(&mut self, what: InlineFor) -> Task<Message> {
        let rows = self
            .workspace()
            .map(|w| w.explorer.tree.rows())
            .unwrap_or_default();
        let (folder, replaces, value) = match &what {
            InlineFor::Note(f) | InlineFor::Folder(f) => (f.clone(), false, String::new()),
            InlineFor::RenameFolder(p) => (
                p.clone(),
                true,
                p.rsplit('/').next().unwrap_or(p).to_owned(),
            ),
        };
        let at = rows.iter().position(|r| r.path == folder);
        let depth = match (&what, at) {
            (InlineFor::RenameFolder(_), Some(i)) => rows[i].depth,
            (_, Some(i)) => rows[i].depth + 1,
            _ => 0,
        };
        if let Some(w) = self.workspace_mut() {
            if !folder.is_empty() && !replaces {
                w.explorer.tree.set_open(&folder, true);
            }
            let at = w.explorer.tree.rows().iter().position(|r| r.path == folder);
            w.explorer.editing = Some((
                what,
                Editing {
                    at,
                    depth,
                    value,
                    problem: None,
                    replaces,
                },
            ));
        }
        iced::widget::operation::focus(view::INPUT)
    }

    pub(super) fn inline_input(&mut self, value: String) {
        if let Some(w) = self.workspace_mut()
            && let Some((_, e)) = &mut w.explorer.editing
        {
            e.problem = name_problem(&value);
            e.value = value;
        }
    }

    /// Enter in the tree's input.
    pub(super) fn inline_submit(&mut self) -> Task<Message> {
        let Some((what, e)) = self.workspace_mut().and_then(|w| w.explorer.editing.take()) else {
            return Task::none();
        };
        if let Some((Field::Rejected, _)) = name_problem(&e.value) {
            // Keep the input open; the reason stays under it (F-10.2).
            if let Some(w) = self.workspace_mut() {
                w.explorer.editing = Some((
                    what,
                    Editing {
                        problem: name_problem(&e.value),
                        ..e
                    },
                ));
            }
            return iced::widget::operation::focus(view::INPUT);
        }
        match what {
            InlineFor::Note(folder) => {
                let taken = self.taken_names(&folder);
                let name = explorer::new_note_name(&e.value, &taken);
                self.explorer_focus = false;
                self.create_in(&folder, &name)
            }
            InlineFor::Folder(parent) => {
                if e.value.trim().is_empty() {
                    return Task::none();
                }
                let path = join(&parent, e.value.trim());
                if let Some(w) = self.workspace_mut() {
                    w.explorer.new_folders.insert(path.clone());
                }
                self.rebuild_tree();
                if let Some(w) = self.workspace_mut() {
                    w.explorer.selected =
                        w.explorer.tree.rows().iter().position(|r| r.path == path);
                }
                Task::none()
            }
            InlineFor::RenameFolder(path) => {
                let name = e.value.trim();
                if name.is_empty() || Some(name) == path.rsplit('/').next() {
                    return Task::none();
                }
                let to = join(folder_of(&path), name);
                self.rename_folder(path, to)
            }
        }
    }

    pub(super) fn inline_cancel(&mut self) {
        if let Some(w) = self.workspace_mut() {
            w.explorer.editing = None;
        }
    }

    /// Note names (lowercase, without `.md`) in `folder`, for "Untitled N".
    fn taken_names(&self, folder: &str) -> HashSet<String> {
        self.workspace()
            .map(|w| {
                w.candidates
                    .iter()
                    .filter(|c| folder_of(&c.path) == folder)
                    .map(|c| c.title.to_lowercase())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// A new note `name` in `folder`, open in insert mode and written once
    /// it has text (F-10.1).
    fn create_in(&mut self, folder: &str, name: &str) -> Task<Message> {
        let path = format!("{}.md", join(folder, name));
        if let Some(w) = self.workspace_mut()
            && !w
                .candidates
                .iter()
                .any(|c| c.path.eq_ignore_ascii_case(&path))
        {
            w.candidates
                .push(crate::switcher::Candidate::new(&path, Vec::new()));
        }
        self.rebuild_tree();
        self.open_new_note(path, true)
    }

    // --- Dialogs ------------------------------------------------------------------

    /// `<leader>n`: the new note dialog (mockup 14).
    pub(super) fn open_new_note_dialog(&mut self) -> Task<Message> {
        let folder = if self.explorer_open {
            self.target_folder()
        } else {
            String::new()
        };
        self.dialog = Some(Dialog::NewNote {
            folder,
            value: String::new(),
            warned: false,
        });
        iced::widget::operation::focus(dialogs::INPUT)
    }

    /// Typing in a dialog's input.
    pub(super) fn dialog_input(&mut self, value: String) -> Task<Message> {
        match &mut self.dialog {
            Some(Dialog::NewNote {
                value: v, warned, ..
            }) => {
                *v = value;
                *warned = false;
                Task::none()
            }
            Some(Dialog::Rename { value: v, .. }) => {
                *v = value;
                self.preview_rename()
            }
            Some(Dialog::Move { query, .. }) => {
                *query = value;
                self.refresh_move_folders();
                Task::none()
            }
            _ => Task::none(),
        }
    }

    /// Enter in a dialog.
    pub(super) fn dialog_submit(&mut self) -> Task<Message> {
        match self.dialog.clone() {
            Some(Dialog::NewNote {
                folder,
                value,
                warned,
            }) => match name_problem(&value) {
                Some((Field::Rejected, _)) => iced::widget::operation::focus(dialogs::INPUT),
                Some((Field::Warning, _)) if !warned => {
                    self.dialog = Some(Dialog::NewNote {
                        folder,
                        value,
                        warned: true,
                    });
                    iced::widget::operation::focus(dialogs::INPUT)
                }
                _ => {
                    self.dialog = None;
                    let taken = self.taken_names(&folder);
                    let name = explorer::new_note_name(&value, &taken);
                    self.create_in(&folder, &name)
                }
            },
            Some(Dialog::Rename { path, value, .. }) => match name_problem(&value) {
                Some((Field::Rejected, _)) => iced::widget::operation::focus(dialogs::INPUT),
                _ => {
                    self.dialog = None;
                    let name = value.trim().trim_end_matches(".md");
                    if name.is_empty() {
                        return Task::none();
                    }
                    let to = format!("{}.md", join(folder_of(&path), name));
                    if to == path {
                        return Task::none();
                    }
                    self.rename_note(path, to)
                }
            },
            Some(Dialog::DeleteNote { path, .. }) | Some(Dialog::DeleteFolder { path, .. }) => {
                self.dialog = None;
                self.trash(path)
            }
            Some(Dialog::Move {
                path,
                folders,
                selected,
                ..
            }) => {
                self.dialog = None;
                let Some((folder, _)) = folders.get(selected) else {
                    return Task::none();
                };
                let name = path.rsplit('/').next().unwrap_or(&path);
                let to = join(folder, name);
                if to == path {
                    return Task::none();
                }
                if self
                    .workspace()
                    .is_some_and(|w| w.explorer.tree.is_folder(&path))
                {
                    self.rename_folder(path, to)
                } else {
                    self.rename_note(path, to)
                }
            }
            None => Task::none(),
        }
    }

    /// Keys while a dialog is open: Esc cancels, Enter confirms (dialogs
    /// without an input), and the move picker's list moves.
    pub(super) fn dialog_key(
        &mut self,
        key: &Key,
        modifiers: Modifiers,
        captured: bool,
    ) -> Task<Message> {
        if *key == Key::Named(Named::Escape) {
            self.dialog = None;
            return Task::none();
        }
        if captured {
            return Task::none();
        }
        let ctrl = |c: &str| modifiers.control() && *key == Key::Character(c.into());
        if let Some(Dialog::Move {
            folders, selected, ..
        }) = &mut self.dialog
        {
            let n = folders.len().max(1);
            if ctrl("n") || *key == Key::Named(Named::ArrowDown) {
                *selected = (*selected + 1) % n;
            } else if ctrl("p") || *key == Key::Named(Named::ArrowUp) {
                *selected = (*selected + n - 1) % n;
            }
            return Task::none();
        }
        if *key == Key::Named(Named::Enter)
            && matches!(
                self.dialog,
                Some(Dialog::DeleteNote { .. } | Dialog::DeleteFolder { .. })
            )
        {
            return self.dialog_submit();
        }
        Task::none()
    }

    pub(super) fn pick_folder(&mut self, i: usize) -> Task<Message> {
        if let Some(Dialog::Move { selected, .. }) = &mut self.dialog {
            *selected = i;
        }
        self.dialog_submit()
    }

    /// `d`: the delete dialog with how many notes link here (F-10).
    fn open_delete(&mut self, path: String, folder: bool) -> Task<Message> {
        let Some(w) = self.workspace() else {
            return Task::none();
        };
        let inside: Vec<String> = if folder {
            w.explorer.tree.files_under(&path)
        } else {
            vec![path.clone()]
        };
        let notes = inside.iter().filter(|p| p.ends_with(".md")).count();
        let attachments = inside.len() - notes;
        let inside_set: HashSet<String> = inside.iter().cloned().collect();
        let p = path.clone();
        Task::perform(
            w.session.query(move |store| {
                let mut sources = HashSet::new();
                for f in &inside {
                    if let Some(row) = store.note_by_path(f).ok().flatten() {
                        for b in store.backlinks(row.id).unwrap_or_default() {
                            if !inside_set.contains(&b.source) {
                                sources.insert(b.source);
                            }
                        }
                    }
                }
                sources.len()
            }),
            move |linking| {
                Message::DeleteCounted(p.clone(), folder, notes, attachments, linking.unwrap_or(0))
            },
        )
    }

    pub(super) fn delete_counted(
        &mut self,
        path: String,
        folder: bool,
        notes: usize,
        attachments: usize,
        linking: usize,
    ) {
        self.dialog = Some(if folder {
            Dialog::DeleteFolder {
                path,
                notes,
                attachments,
                outside: linking,
            }
        } else {
            Dialog::DeleteNote { path, linking }
        });
    }

    /// Moves a note or folder to `.trash/` through the write path (F-10.3).
    fn trash(&mut self, path: String) -> Task<Message> {
        // Text that cannot be saved would be closed with the note: the
        // trash only gets what is on disk.
        let prefix = format!("{path}/");
        if let Some(p) = self.unsaved_stuck(|p| p == path || p.starts_with(&prefix), true) {
            self.say(
                Level::Error,
                format!("Can't delete \"{path}\": \"{p}\" has unsaved changes"),
            );
            return Task::none();
        }
        // Open notes inside are saved first, then closed.
        let save = self.autosave(true);
        let Some(w) = self.workspace() else {
            return Task::none();
        };
        let root = w.session.paths.root.clone();
        let journal = w.journal.clone();
        let p = path.clone();
        let job = Task::future(async move {
            let result = expect_current(&root, &p)
                .map_err(|e| e.to_string())
                .and_then(|expect| {
                    let tx = Transaction {
                        actor: Actor::User,
                        ops: vec![Op::Trash {
                            path: p.clone(),
                            expect,
                        }],
                    };
                    commit(&root, &journal, &tx)
                        .map(|_| ())
                        .map_err(|e| e.to_string())
                });
            Message::FilesChanged(vec![p], result)
        });
        save.chain(job)
    }

    /// After a trash, rename or move: closes or renames affected buffers,
    /// refreshes the index, and reports failures.
    pub(super) fn files_changed(
        &mut self,
        gone: Vec<String>,
        result: Result<(), String>,
    ) -> Task<Message> {
        if let Err(e) = result {
            self.say(Level::Error, format!("Can't change the vault: {e}"));
            return Task::none();
        }
        if let Some(w) = self.workspace_mut() {
            let (shown, was) = (w.active_buffer().map(|b| b.id), w.active);
            for g in &gone {
                let prefix = format!("{g}/");
                w.buffers
                    .retain(|b| b.path != *g && !b.path.starts_with(&prefix));
                w.new_folders_remove(g);
            }
            // The note shown stays shown; when it went, its neighbour.
            w.active = w
                .buffers
                .iter()
                .position(|b| Some(b.id) == shown)
                .or_else(|| {
                    let last = w.buffers.len().checked_sub(1)?;
                    was.map(|a| a.min(last))
                });
        }
        let path = gone.first().cloned().unwrap_or_default();
        Task::batch([self.refresh_index(path), self.save_vault_state()])
    }

    // --- Views ----------------------------------------------------------------------

    /// The body with the explorer at its left, when open.
    pub(super) fn with_explorer<'a>(&'a self, body: Element<'a, Message>) -> Element<'a, Message> {
        let (true, Some(w)) = (self.explorer_open, self.workspace()) else {
            return body;
        };
        let classic = self.layout() == patinote_core::config::Layout::Classic;
        if self.layout() == patinote_core::config::Layout::Focus {
            return body;
        }
        let rows = w.explorer.tree.rows();
        let editing = w.explorer.editing.as_ref().map(|(_, e)| e.clone());
        let active = self.active_document().map(|d| d.path.clone());
        let selected = if self.explorer_focus {
            w.explorer.selected
        } else {
            None
        };
        let explorer = view::View {
            vault: w.session.paths.name.clone(),
            rows,
            selected,
            active,
            tags: w.explorer.tags.clone(),
            editing,
            on_select: Message::ExplorerClick,
            on_input: Message::InlineInput,
            on_submit: Message::InlineSubmit,
            width: self.explorer_width(),
            find: classic.then_some(Message::FindNote),
        }
        .view();
        row![explorer, self.edge(super::layout::Edge::Explorer), body].into()
    }

    /// The open dialog over everything, on the scrim, 72 px from the top.
    pub(super) fn with_dialog<'a>(&'a self, body: Element<'a, Message>) -> Element<'a, Message> {
        let Some(d) = &self.dialog else { return body };
        let content: Element<'a, Message> = match d {
            Dialog::NewNote {
                folder,
                value,
                warned,
            } => {
                let mut problem = name_problem(value);
                if *warned && let Some((Field::Warning, _)) = problem {
                    problem = name_problem(value);
                }
                dialogs::new_note(
                    value,
                    problem.as_ref(),
                    folder,
                    Message::DialogInput,
                    Message::DialogSubmit,
                )
            }
            Dialog::Rename {
                path,
                value,
                updates,
                notes,
            } => dialogs::rename(
                path,
                value,
                name_problem(value).as_ref(),
                updates,
                *notes,
                Message::DialogInput,
                Message::DialogSubmit,
                Message::DialogCancel,
            ),
            Dialog::DeleteNote { path, linking } => {
                let name = path
                    .rsplit('/')
                    .next()
                    .unwrap_or(path)
                    .trim_end_matches(".md");
                dialogs::delete_note(
                    name,
                    path,
                    *linking,
                    &format!(".trash/{path}"),
                    Message::DialogSubmit,
                    Message::DialogCancel,
                )
            }
            Dialog::DeleteFolder {
                path,
                notes,
                attachments,
                outside,
            } => {
                let name = path.rsplit('/').next().unwrap_or(path);
                dialogs::delete_folder(
                    name,
                    *notes,
                    *attachments,
                    *outside,
                    &format!(".trash/{path}/"),
                    Message::DialogSubmit,
                    Message::DialogCancel,
                )
            }
            Dialog::Move {
                path,
                query,
                folders,
                selected,
                path_links,
            } => {
                let name = path
                    .rsplit('/')
                    .next()
                    .unwrap_or(path)
                    .trim_end_matches(".md");
                dialogs::move_picker(
                    name,
                    folder_of(path),
                    query,
                    folders,
                    *selected,
                    *path_links,
                    Message::DialogInput,
                    Message::DialogSubmit,
                    Message::PickFolder,
                )
            }
        };
        stack![
            body,
            container("")
                .width(Length::Fill)
                .height(Length::Fill)
                .style(styles::scrim),
            container(content)
                .width(Length::Fill)
                .height(Length::Fill)
                .align_x(iced::Alignment::Center)
                .padding(Padding {
                    top: SIZE_SWITCHER_TOP - SIZE_BUFFERLINE_H,
                    ..Padding::ZERO
                }),
        ]
        .into()
    }
}

impl super::Workspace {
    fn new_folders_remove(&mut self, path: &str) {
        let prefix = format!("{path}/");
        self.explorer
            .new_folders
            .retain(|f| f != path && !f.starts_with(&prefix));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nested_tags_get_their_parent_row() {
        let tags = vec![
            ("harian".into(), 1, 0),
            ("proyek/patinote".into(), 2, 1),
            ("rust".into(), 2, 0),
        ];
        let got: Vec<(String, usize, usize)> = with_parents(tags);
        assert_eq!(
            got,
            [
                ("harian".to_owned(), 1, 0),
                ("proyek".to_owned(), 2, 0),
                ("proyek/patinote".to_owned(), 2, 1),
                ("rust".to_owned(), 2, 0),
            ]
        );
    }
}
