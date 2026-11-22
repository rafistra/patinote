//! Changes from outside (F-04, F-11): the watcher's changes reach the index
//! and the open notes; a note that changed or was deleted on disk while it
//! had unsaved text gets a banner and nothing is overwritten; versions are
//! compared side by side; and Syncthing conflicts are listed and resolved.

use iced::keyboard::{Key, Modifiers, key::Named};
use iced::widget::{container, stack};
use iced::{Element, Length, Padding, Task};
use patinote_core::store::Hash;
use patinote_core::write::{Actor, Expect, Op, Transaction, commit, content_hash};
use patinote_core::{Change, SyncConflict, WatchMode};

use super::{App, Message};
use crate::diff;
use crate::editor::document::{Document, External, Save};
use crate::editor::file;
use crate::screens::compare::{self, Side};
use crate::screens::conflicts::{self, Item, copy_info};
use crate::theme::styles::{self, BannerKind, Level};
use crate::theme::tokens::*;
use crate::widgets::banner::{Action, Banner, Icon};

/// What is being compared side by side.
#[derive(Debug, Clone)]
pub enum Comparing {
    /// The open note against its version on disk (mockup 08).
    Disk(String),
    /// A Syncthing conflict: the original and the copy (mockup 16).
    Sync(Box<SyncPair>),
}

/// A Syncthing conflict as read for the comparison. Picking a version acts
/// on exactly these bytes: a file that changed since is not overwritten.
#[derive(Debug, Clone)]
pub struct SyncPair {
    pub original: String,
    pub copy: String,
    pub sides: diff::Sides,
    /// The original's hash when read; `None` when it was missing.
    pub original_hash: Option<Hash>,
    pub copy_bytes: Vec<u8>,
}

/// An action on a banner or in a comparison.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Choice {
    UseDisk,
    KeepMine,
    Compare,
    CloseDeleted,
    SaveAgain,
    UseCopy,
    KeepOriginal,
    OpenBoth,
}

fn name_of(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

impl App {
    /// A group of changes from the watcher: the index catches up, then the
    /// open notes are checked against the disk.
    pub(super) fn external_change(&mut self, change: Change) -> Task<Message> {
        if let Some(w) = self.workspace_mut() {
            w.watch_mode = Some(change.mode.clone());
        }
        let Some(w) = self.workspace() else {
            return Task::none();
        };
        let root = w.session.paths.root.clone();
        let paths = change.paths.clone();
        let rescan = change.rescan;
        let open: Vec<String> = w
            .buffers
            .iter()
            .map(|b| b.path.clone())
            .filter(|p| {
                rescan
                    || paths
                        .iter()
                        .any(|c| c == p || p.starts_with(&format!("{c}/")))
            })
            .collect();
        let index = Task::perform(
            w.session.query(move |store| {
                let _ =
                    patinote_index::update_index(store, &root, &Default::default(), &mut |_| {});
            }),
            |_| Message::Reindexed(String::new()),
        );
        Task::batch(std::iter::once(index).chain(open.into_iter().map(|p| self.read_disk(p))))
    }

    /// Reads an open note from disk to compare it with what was loaded.
    pub(super) fn read_disk(&self, path: String) -> Task<Message> {
        let Some(w) = self.workspace() else {
            return Task::none();
        };
        let root = w.session.paths.root.clone();
        Task::future(async move {
            let bytes = std::fs::read(patinote_core::names::on_disk(&root, &path)).ok();
            Message::DiskState(path, bytes)
        })
    }

    /// What the disk holds for an open note (F-11): unchanged, reloaded, or
    /// a banner when the note has unsaved text or was deleted.
    pub(super) fn disk_state(&mut self, path: String, bytes: Option<Vec<u8>>) {
        let mut info = None;
        let Some(doc) = self
            .workspace_mut()
            .and_then(|w| w.buffers.iter_mut().find(|b| b.path == path))
            .and_then(|b| b.doc.as_mut())
        else {
            return;
        };
        // While a save runs, the disk may already hold what it writes: the
        // disk is read again once the save has landed. (A change from
        // outside that lands after the save's hash check lets it succeed.)
        if matches!(doc.save, Save::Running(_)) {
            doc.recheck = true;
            return;
        }
        match bytes {
            None if doc.disk_hash.is_none() => {}
            None => {
                doc.external = Some(External::Deleted);
                doc.save = Save::Changed;
            }
            Some(bytes) => {
                let hash = content_hash(&bytes);
                if doc.disk_hash == Some(hash) {
                    // Our own save, or nothing new.
                    return;
                }
                if doc.dirty() {
                    doc.external = Some(External::Changed {
                        text: file::open(&bytes).text,
                        bytes,
                        hash,
                    });
                    doc.save = Save::Changed;
                } else {
                    reload(doc, &bytes);
                    info = Some(format!("\"{}\" updated from disk", name_of(&path)));
                }
            }
        }
        if let Some(text) = info {
            self.say(Level::Info, text);
        }
        self.refresh_reading();
    }

    /// A banner or comparison action.
    pub(super) fn choose(&mut self, choice: Choice) -> Task<Message> {
        match choice {
            Choice::UseCopy | Choice::KeepOriginal | Choice::OpenBoth => {
                return self.resolve_sync(choice);
            }
            Choice::Compare => {
                let path = self.active_document().map(|d| d.path.clone());
                self.comparing = path.map(Comparing::Disk);
                return Task::none();
            }
            _ => {}
        }
        self.comparing = None;
        let Some(doc) = self.active_document_mut() else {
            return Task::none();
        };
        match (choice, doc.external.take()) {
            (Choice::UseDisk, Some(External::Changed { bytes, .. })) => {
                // The bytes as they are on disk: their hash and format.
                reload(doc, &bytes);
                self.refresh_reading();
                Task::none()
            }
            (Choice::KeepMine, Some(External::Changed { hash, .. })) => {
                // My text replaces the version now on disk.
                doc.disk_hash = Some(hash);
                doc.save = Save::Idle;
                self.autosave(true)
            }
            (Choice::SaveAgain, Some(External::Deleted)) => {
                doc.disk_hash = None;
                doc.save = Save::Idle;
                // Not on disk any more: the whole text is unsaved.
                doc.saved_rev = u64::MAX;
                self.autosave(true)
            }
            (Choice::CloseDeleted, Some(External::Deleted)) => self.close_or_quit(true),
            (_, other) => {
                doc.external = other;
                Task::none()
            }
        }
    }

    /// The banner for a note changed or deleted outside (mockups 08, 17).
    pub(super) fn external_banner(&self) -> Option<Element<'_, Message>> {
        if let Some(Comparing::Sync(pair)) = &self.comparing {
            let original = &pair.original;
            return Some(
                Banner {
                    kind: BannerKind::Warning,
                    icon: Icon::Warning,
                    title: format!("Syncthing kept two versions of {}.", name_of(original)),
                    body: "Neither file has been changed. Pick one, or open both to merge by hand."
                        .into(),
                    code: None,
                    actions: vec![
                        action("Use copy", false, Choice::UseCopy),
                        action("Keep original", false, Choice::KeepOriginal),
                        action("Open both", true, Choice::OpenBoth),
                    ],
                }
                .into_element(),
            );
        }
        let doc = self.active_document()?;
        let name = name_of(&doc.path);
        let banner = match doc.external.as_ref()? {
            External::Changed { .. } => Banner {
                kind: BannerKind::Warning,
                icon: Icon::Warning,
                title: format!("{name} changed on disk while you were editing."),
                body: "Autosave is paused for this note. Nothing has been overwritten.".into(),
                code: None,
                actions: vec![
                    action("Use disk version", false, Choice::UseDisk),
                    action("Keep mine", false, Choice::KeepMine),
                    action("Compare", true, Choice::Compare),
                ],
            },
            External::Deleted => Banner {
                kind: BannerKind::Warning,
                icon: Icon::Warning,
                title: format!("{name} was deleted outside Patinote."),
                body: "Your text is still here in the editor. Autosave is paused for this note."
                    .into(),
                code: None,
                actions: vec![
                    action("Close", false, Choice::CloseDeleted),
                    action("Save again", true, Choice::SaveAgain),
                ],
            },
        };
        Some(banner.into_element())
    }

    /// The comparison in place of the editor, if one is open.
    pub(super) fn comparison(&self) -> Option<Element<'_, Message>> {
        match self.comparing.as_ref()? {
            Comparing::Disk(path) => {
                let doc = self.active_document().filter(|d| d.path == *path)?;
                let Some(External::Changed { text, .. }) = &doc.external else {
                    return None;
                };
                let sides = diff::sides(text, &doc.buffer.text());
                Some(compare::view(
                    &sides,
                    ("On disk · changed by another app".into(), Side::Disk),
                    ("Your version · unsaved".into(), Side::Mine),
                    ("only on disk", "only in your version"),
                    "Pick a version in the banner, or open both side by side to merge by hand. Per-hunk merge arrives in Phase 3.",
                ))
            }
            Comparing::Sync(pair) => {
                let SyncPair {
                    original,
                    copy,
                    sides,
                    ..
                } = pair.as_ref();
                let from = copy_info(copy).map_or_else(String::new, |c| {
                    format!(" · from device {}, {}", c.device, c.when)
                });
                Some(compare::view(
                    sides,
                    (format!("Original · {original}"), Side::Mine),
                    (format!("Copy{from}"), Side::Disk),
                    ("only in the original", "only in the copy"),
                    "After you pick, the copy moves to .trash/. Per-hunk merge arrives in Phase 3.",
                ))
            }
        }
    }

    /// The command line under a comparison: how many lines differ.
    pub(super) fn comparison_summary(&self) -> Option<String> {
        let plural = |n: usize, one: &str| {
            if n == 1 {
                format!("1 {one}")
            } else {
                format!("{n} {one}s")
            }
        };
        match self.comparing.as_ref()? {
            Comparing::Disk(_) => {
                let doc = self.active_document()?;
                let Some(External::Changed { text, .. }) = &doc.external else {
                    return None;
                };
                let s = diff::sides(text, &doc.buffer.text());
                Some(format!(
                    "{} on disk, {} in your version",
                    plural(s.left_only(), "change"),
                    s.right_only()
                ))
            }
            Comparing::Sync(pair) => Some(format!(
                "{} only in the original, {} only in the copy",
                plural(pair.sides.left_only(), "line"),
                pair.sides.right_only()
            )),
        }
    }

    // --- Syncthing conflicts ----------------------------------------------------

    /// Loads the Syncthing conflicts (F-11): a scan, off the UI thread.
    pub(super) fn load_conflicts(&self) -> Task<Message> {
        let Some(w) = self.workspace() else {
            return Task::none();
        };
        let root = w.session.paths.root.clone();
        Task::perform(
            w.session.query(move |_| {
                patinote_core::scan(&root, &Default::default())
                    .map_or_else(|_| Vec::new(), |s| s.conflicts)
            }),
            |c| Message::Conflicts(c.unwrap_or_default()),
        )
    }

    pub(super) fn conflicts_loaded(&mut self, conflicts: Vec<SyncConflict>) {
        if let Some(w) = self.workspace_mut() {
            w.conflicts = conflicts;
        }
    }

    /// The banner shown above the active note, for tests: its title.
    pub fn external_title(&self) -> Option<String> {
        let doc = self.active_document()?;
        match doc.external.as_ref()? {
            External::Changed { .. } => Some(format!(
                "{} changed on disk while you were editing.",
                name_of(&doc.path)
            )),
            External::Deleted => Some(format!(
                "{} was deleted outside Patinote.",
                name_of(&doc.path)
            )),
        }
    }

    /// The comparison's summary line, for tests.
    pub fn compare_summary(&self) -> Option<String> {
        self.comparison_summary()
    }

    /// Opens the conflict of the note at `original`, for tests.
    pub fn conflict_index(&self, original: &str) -> Option<usize> {
        self.workspace()?
            .conflicts
            .iter()
            .position(|c| c.original == original)
    }

    /// The number of Syncthing conflicts, for the status line and tests.
    pub fn conflict_count(&self) -> usize {
        self.workspace().map_or(0, |w| w.conflicts.len())
    }

    /// Whether the conflict list is open, for tests.
    pub fn conflict_list_open(&self) -> bool {
        self.conflict_list.is_some()
    }

    /// `<leader>c` / `:conflicts`.
    pub(super) fn open_conflicts(&mut self) -> Task<Message> {
        self.conflict_list = Some(0);
        self.load_conflicts()
    }

    pub(super) fn conflicts_key(
        &mut self,
        key: &Key,
        text: Option<&str>,
        modifiers: Modifiers,
    ) -> Option<Task<Message>> {
        let sel = self.conflict_list?;
        let n = self.conflict_count().max(1);
        let ctrl = |c: &str| modifiers.control() && *key == Key::Character(c.into());
        if *key == Key::Named(Named::Escape) {
            self.conflict_list = None;
        } else if ctrl("n") || *key == Key::Named(Named::ArrowDown) || text == Some("j") {
            self.conflict_list = Some((sel + 1) % n);
        } else if ctrl("p") || *key == Key::Named(Named::ArrowUp) || text == Some("k") {
            self.conflict_list = Some((sel + n - 1) % n);
        } else if *key == Key::Named(Named::Enter) {
            return Some(self.open_conflict(sel));
        }
        Some(Task::none())
    }

    /// Opens conflict `i` as a comparison (mockup 16).
    pub(super) fn open_conflict(&mut self, i: usize) -> Task<Message> {
        self.conflict_list = None;
        let Some(w) = self.workspace() else {
            return Task::none();
        };
        let Some(c) = w.conflicts.get(i).cloned() else {
            return Task::none();
        };
        let root = w.session.paths.root.clone();
        Task::future(async move {
            let original = std::fs::read(patinote_core::names::on_disk(&root, &c.original)).ok();
            let copy_bytes =
                std::fs::read(patinote_core::names::on_disk(&root, &c.path)).unwrap_or_default();
            let text = |b: &[u8]| file::open(b).text;
            let sides = diff::sides(
                &original.as_deref().map(text).unwrap_or_default(),
                &text(&copy_bytes),
            );
            Message::SyncLoaded(Box::new(SyncPair {
                original: c.original,
                copy: c.path,
                sides,
                original_hash: original.as_deref().map(content_hash),
                copy_bytes,
            }))
        })
    }

    pub(super) fn sync_loaded(&mut self, pair: Box<SyncPair>) {
        self.comparing = Some(Comparing::Sync(pair));
    }

    /// Use copy, Keep original, Open both (F-11.6). The copy moves to
    /// `.trash/` once a version is picked; nothing is deleted.
    fn resolve_sync(&mut self, choice: Choice) -> Task<Message> {
        let Some(Comparing::Sync(pair)) = self.comparing.take() else {
            return Task::none();
        };
        if choice == Choice::KeepOriginal && pair.original_hash.is_none() {
            // Trashing the copy would leave no version at all.
            let msg = format!(
                "Can't keep the original: \"{}\" no longer exists. Use the copy instead.",
                pair.original
            );
            self.comparing = Some(Comparing::Sync(pair));
            self.say(Level::Error, msg);
            return Task::none();
        }
        if choice == Choice::OpenBoth {
            let SyncPair { original, copy, .. } = *pair;
            let first = self.open_note(original);
            return Task::batch([first, self.open_note(copy)]);
        }
        self.queue_vault_op(super::renaming::VaultOp::Sync { choice, pair })
    }

    /// Writes the choice for a Syncthing conflict, once no save runs.
    pub(super) fn resolve_sync_now(&mut self, choice: Choice, pair: SyncPair) -> Task<Message> {
        let SyncPair {
            original,
            copy,
            original_hash,
            copy_bytes,
            ..
        } = pair;
        let save = self.autosave(true);
        let Some(w) = self.workspace_mut() else {
            return Task::none();
        };
        // No save starts until the choice lands (`vault_op_done`).
        w.vault_busy = true;
        let root = w.session.paths.root.clone();
        let journal = w.journal.clone();
        let use_copy = choice == Choice::UseCopy;
        let (o, c) = (original.clone(), copy.clone());
        let job = self.vault_job(vec![c.clone()], move || {
            let mut ops = Vec::new();
            // Both files must still be what the comparison showed.
            let copy_expect = Expect::Hash(content_hash(&copy_bytes));
            if use_copy {
                ops.push(Op::Write {
                    path: o.clone(),
                    content: copy_bytes,
                    expect: original_hash.map_or(Expect::Absent, Expect::Hash),
                });
            }
            ops.push(Op::Trash {
                path: c.clone(),
                expect: copy_expect,
            });
            let tx = Transaction {
                actor: Actor::User,
                ops,
            };
            commit(&root, &journal, &tx)
                .map(|_| ())
                .map_err(|e| e.to_string())
        });
        let reread = self.read_disk(original);
        save.chain(job).chain(reread)
    }

    /// The conflict list over the screen (mockup 15).
    pub(super) fn with_conflicts<'a>(&'a self, body: Element<'a, Message>) -> Element<'a, Message> {
        let (Some(sel), Some(w)) = (self.conflict_list, self.workspace()) else {
            return body;
        };
        let items: Vec<Item> = w
            .conflicts
            .iter()
            .map(|c| {
                let (folder, file) = c
                    .original
                    .rsplit_once('/')
                    .map_or((String::new(), c.original.as_str()), |(d, f)| {
                        (format!("{d}/"), f)
                    });
                Item {
                    title: file.trim_end_matches(".md").to_owned(),
                    folder,
                    copy: c.path.clone(),
                }
            })
            .collect();
        stack![
            body,
            container("")
                .width(Length::Fill)
                .height(Length::Fill)
                .style(styles::scrim),
            container(conflicts::view(&items, sel, Message::OpenConflict))
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

fn action(label: &str, primary: bool, choice: Choice) -> Action<Message> {
    Action {
        label: label.into(),
        primary,
        on_press: Message::Choose(choice),
    }
}

/// Replaces a document's text with `bytes` from disk, keeping the cursor's
/// line where possible (F-11.1).
fn reload(doc: &mut Document, bytes: &[u8]) {
    let cursor = doc.buffer.cursor();
    let opened = file::open(bytes);
    doc.buffer.replace_text(&opened.text);
    doc.buffer.set_cursor(cursor);
    doc.format = opened.format;
    // Outside the vault stays so: `file::open` only knows about the bytes.
    if doc.read_only != Some(file::ReadOnly::Outside) {
        doc.read_only = opened.read_only;
    }
    doc.disk_hash = Some(opened.hash);
    doc.saved_rev = doc.buffer.revision();
    doc.save = Save::Idle;
    doc.external = None;
    doc.kinds();
}

/// Whether the watcher polls instead of getting events (F-04).
pub fn polling(mode: Option<&WatchMode>) -> bool {
    matches!(mode, Some(WatchMode::Polling(_)))
}
