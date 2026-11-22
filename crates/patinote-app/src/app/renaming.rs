//! Renaming and moving notes and folders (F-10): the dialogs' previews of
//! which links change, and committing the move together with every link
//! update through the write path, using `patinote_core::relink`.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use iced::Task;
use nucleo_matcher::pattern::{AtomKind, CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32String};
use patinote_core::model::LinkKind;
use patinote_core::relink::{self, Moved, RelinkPlan};
use patinote_core::resolve::Target;
use patinote_core::write::{Actor, Expect, Journal, commit, expect_current};
use patinote_core::{FileKind, Resolver};

use crate::editor::document::Save;
use patinote_core::IndexOptions;
use patinote_core::store::Store;
use patinote_index::{SqliteStore, update_index};

use super::files::Dialog;
use super::{App, Message};
use crate::screens::dialogs::{self, LinkUpdate};

/// What a rename or move would change, for its dialog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Preview {
    /// The note or folder being renamed or moved.
    pub path: String,
    pub updates: Vec<LinkUpdate>,
    pub notes: usize,
}

fn is_note(path: &str) -> bool {
    path.to_lowercase().ends_with(".md")
}

/// Plans the link updates for `moves`, reading every note that links to a
/// moved note (from the index) and the moved notes themselves.
fn plan_on_index(store: &SqliteStore, root: &Path, moves: &[Moved]) -> RelinkPlan {
    let mut sources: Vec<String> = Vec::new();
    let mut seen = HashSet::new();
    for m in moves {
        // A moved note's own links may change; an attachment has none.
        if is_note(&m.from) && seen.insert(m.from.clone()) {
            sources.push(m.from.clone());
        }
        if let Some(row) = store.note_by_path(&m.from).ok().flatten() {
            for b in store.backlinks(row.id).unwrap_or_default() {
                if seen.insert(b.source.clone()) {
                    sources.push(b.source);
                }
            }
        }
    }
    let files: Vec<(String, Vec<u8>)> = sources
        .into_iter()
        .filter_map(|p| {
            std::fs::read(patinote_core::names::on_disk(root, &p))
                .ok()
                .map(|b| (p, b))
        })
        .collect();
    // Every file as it will be after the move, to tell whether a link
    // target still finds exactly one.
    let moved: std::collections::HashMap<&str, &str> = moves
        .iter()
        .map(|m| (m.from.as_str(), m.to.as_str()))
        .collect();
    let aliases = store.all_aliases().unwrap_or_default();
    let after: Vec<Target> = store
        .all_notes()
        .unwrap_or_default()
        .into_iter()
        .map(|n| Target {
            path: moved
                .get(n.path.as_str())
                .map_or(n.path.clone(), |t| (*t).to_owned()),
            kind: match n.kind.as_str() {
                "note" => FileKind::Note,
                "canvas" => FileKind::Canvas,
                _ => FileKind::Attachment,
            },
            aliases: aliases.get(&n.id).cloned().unwrap_or_default(),
        })
        .collect();
    let after = Resolver::new(after);
    relink::plan(
        moves,
        &files,
        |source, link| {
            let kind = if link.kind == LinkKind::Markdown {
                LinkKind::Markdown
            } else {
                LinkKind::Wiki
            };
            let candidates = store.link_candidates(&link.target).unwrap_or_default();
            Resolver::new(candidates)
                .resolve(source, kind, &link.target)
                .map(|r| r.path.to_owned())
        },
        |target| {
            after
                .resolve("", LinkKind::Wiki, target)
                .is_some_and(|r| !r.is_ambiguous())
        },
    )
}

fn preview_of(path: String, plan: &RelinkPlan) -> Preview {
    let updates = plan
        .files
        .iter()
        .flat_map(|f| {
            f.changes.iter().map(|c| LinkUpdate {
                path: f.path.clone(),
                old: c.old.clone(),
                new: c.new.clone(),
            })
        })
        .collect();
    Preview {
        path,
        updates,
        notes: plan.note_count(),
    }
}

/// Commits `moves` with their link updates in one transaction; a folder
/// moves whole, attachments and subfolders included. Public for the rename
/// benchmark (`benches/rename.rs`).
pub fn commit_moves(
    store: &SqliteStore,
    root: &Path,
    journal: &Journal,
    moves: &[Moved],
    folder: Option<(String, String)>,
) -> Result<Vec<String>, String> {
    let plan = plan_on_index(store, root, moves);
    let changed: Vec<String> = plan.files.iter().map(|f| f.path.clone()).collect();
    match folder {
        None => {
            let tx = plan.transaction(Actor::User, |p| {
                expect_current(root, p).unwrap_or(Expect::Absent)
            });
            commit(root, journal, &tx).map_err(|e| e.to_string())?;
        }
        Some((from, to)) => {
            // One transaction: link updates at their current paths, then the
            // folder moves with everything in it.
            let expect = expect_current(root, &from).map_err(|e| e.to_string())?;
            let tx = plan.folder_transaction(Actor::User, &from, &to, expect);
            commit(root, journal, &tx).map_err(|e| e.to_string())?;
        }
    }
    Ok(changed)
}

impl App {
    /// `r` on a note: the rename dialog (mockup 09).
    pub(super) fn open_rename(&mut self, path: String) -> Task<Message> {
        let name = path
            .rsplit('/')
            .next()
            .unwrap_or(&path)
            .trim_end_matches(".md")
            .to_owned();
        self.dialog = Some(Dialog::Rename {
            path,
            value: name,
            updates: Vec::new(),
            notes: 0,
        });
        Task::batch([
            self.preview_rename(),
            iced::widget::operation::focus(dialogs::INPUT),
        ])
    }

    /// Recomputes which links the rename in the dialog changes.
    pub(super) fn preview_rename(&self) -> Task<Message> {
        let (Some(Dialog::Rename { path, value, .. }), Some(w)) = (&self.dialog, self.workspace())
        else {
            return Task::none();
        };
        let name = value.trim().trim_end_matches(".md");
        if name.is_empty() {
            return Task::none();
        }
        let folder = path.rsplit_once('/').map_or("", |(d, _)| d);
        let to = if folder.is_empty() {
            format!("{name}.md")
        } else {
            format!("{folder}/{name}.md")
        };
        let moves = vec![Moved {
            from: path.clone(),
            to,
        }];
        let root = w.session.paths.root.clone();
        let p = path.clone();
        Task::perform(
            w.session
                .query(move |store| preview_of(p.clone(), &plan_on_index(store, &root, &moves))),
            |preview| match preview {
                Some(p) => Message::RenamePreview(p),
                None => Message::Done,
            },
        )
    }

    pub(super) fn rename_preview(&mut self, preview: Preview) {
        match &mut self.dialog {
            Some(Dialog::Rename {
                path,
                updates,
                notes,
                ..
            }) if *path == preview.path => {
                *updates = preview.updates;
                *notes = preview.notes;
            }
            Some(Dialog::Move {
                path, path_links, ..
            }) if *path == preview.path => {
                *path_links = preview.updates.len();
            }
            _ => {}
        }
    }

    /// `m`: the move dialog with every folder (mockup 14).
    pub(super) fn open_move(&mut self, path: String) -> Task<Message> {
        self.dialog = Some(Dialog::Move {
            path: path.clone(),
            query: String::new(),
            folders: Vec::new(),
            selected: 0,
            path_links: 0,
        });
        self.refresh_move_folders();
        // How many links use the path: plan a move to another folder.
        let preview = match self.workspace() {
            Some(w) if path.ends_with(".md") => {
                let root = w.session.paths.root.clone();
                let name = path.rsplit('/').next().unwrap_or(&path).to_owned();
                let moves = vec![Moved {
                    from: path.clone(),
                    to: format!("{}/{name}", patinote_core::write::TRASH),
                }];
                let p = path.clone();
                Task::perform(
                    w.session.query(move |store| {
                        preview_of(p.clone(), &plan_on_index(store, &root, &moves))
                    }),
                    |preview| match preview {
                        Some(mut p) => {
                            // Links by name don't change on a move.
                            p.updates.retain(|u| u.old.contains('/'));
                            Message::RenamePreview(p)
                        }
                        None => Message::Done,
                    },
                )
            }
            _ => Task::none(),
        };
        Task::batch([preview, iced::widget::operation::focus(dialogs::INPUT)])
    }

    /// Filters the move dialog's folders by what was typed.
    pub(super) fn refresh_move_folders(&mut self) {
        let Some(w) = self.workspace() else { return };
        let all: Vec<String> = w.explorer.tree.folders().map(str::to_owned).collect();
        let Some(Dialog::Move {
            path,
            query,
            folders,
            selected,
            ..
        }) = &mut self.dialog
        else {
            return;
        };
        let own = path.clone();
        let candidates = all
            .into_iter()
            .filter(|f| *f != own && !f.starts_with(&format!("{own}/")));
        let mut out: Vec<(u32, String, Vec<usize>)> = Vec::new();
        if query.trim().is_empty() {
            out = candidates.map(|f| (0, f, Vec::new())).collect();
        } else {
            let pattern = Pattern::new(
                query,
                CaseMatching::Ignore,
                Normalization::Smart,
                AtomKind::Fuzzy,
            );
            let mut matcher = Matcher::new(Config::DEFAULT.match_paths());
            let mut idx = Vec::new();
            for f in candidates {
                let shown = format!("{f}/");
                let hay = Utf32String::from(shown.as_str());
                if let Some(score) = pattern.indices(hay.slice(..), &mut matcher, &mut idx) {
                    idx.sort_unstable();
                    idx.dedup();
                    out.push((score, f, idx.iter().map(|&i| i as usize).collect()));
                }
                idx.clear();
            }
            out.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.len().cmp(&b.1.len())));
        }
        *folders = out.into_iter().take(50).map(|(_, f, m)| (f, m)).collect();
        *selected = 0;
    }

    /// Asks for a vault change: it runs after the ones before it, once no
    /// save runs, so a save never lands at a path that moved or went to the
    /// trash meanwhile, and never races the links a rename rewrites.
    pub(super) fn queue_vault_op(&mut self, op: VaultOp) -> Task<Message> {
        if let Some(w) = self.workspace_mut() {
            w.vault_ops.push_back(op);
        }
        self.pump_vault_ops()
    }

    /// Starts the next vault change when none runs and no save runs. One
    /// that is refused at once (a stuck note, say) lets the next one go.
    pub(super) fn pump_vault_ops(&mut self) -> Task<Message> {
        let mut tasks = Vec::new();
        while let Some(w) = self.workspace_mut() {
            let saving = w
                .buffers
                .iter()
                .filter_map(|b| b.doc.as_ref())
                .chain(&w.closing)
                .any(|d| matches!(d.save, Save::Running(_)));
            if w.vault_busy {
                break;
            }
            // A rename plans its link updates from the index: while the
            // index is still being built it waits (saves go on), and changes
            // after it wait too, so the order stays. Its own job brings the
            // index up to date before it plans (`run_moves`).
            let rename_first = w.vault_ops.front().is_some_and(VaultOp::needs_index);
            if rename_first && matches!(w.index, super::IndexStatus::Indexing { .. }) {
                break;
            }
            if saving {
                break;
            }
            let Some(op) = w.vault_ops.pop_front() else {
                break;
            };
            tasks.push(match op {
                VaultOp::RenameNote { from, to } => self.rename_note(from, to),
                VaultOp::RenameFolder { from, to } => self.rename_folder(from, to),
                VaultOp::Trash { path } => self.trash(path),
                VaultOp::Sync { choice, pair } => self.resolve_sync_now(choice, *pair),
            });
        }
        Task::batch(tasks)
    }

    /// A quit that waited for saves and queued changes goes on once none
    /// is left.
    pub(super) fn quit_if_waiting(&mut self) -> Task<Message> {
        if self.quitting && !self.unsaved() && !self.save_failed() && !self.exited {
            self.quit(false)
        } else {
            Task::none()
        }
    }

    /// Whether a queued rename waits for the index to be built.
    pub(super) fn rename_waits_for_index(&self) -> bool {
        self.workspace().is_some_and(|w| {
            matches!(w.index, super::IndexStatus::Indexing { .. })
                && w.vault_ops.iter().any(VaultOp::needs_index)
        })
    }

    /// A vault change landed: notes closed meanwhile are saved, then the
    /// next change goes, and a quit that waited for it goes on.
    pub(super) fn vault_op_done(&mut self) -> Task<Message> {
        let mut tasks = Vec::new();
        if let Some(w) = self.workspace_mut() {
            w.vault_busy = false;
            let (root, journal) = (w.session.paths.root.clone(), w.journal.clone());
            // Held while it ran: also one whose last save failed or met a
            // conflict. Saved now; if that fails again, `save_landed` brings
            // it back with its message.
            for d in &mut w.closing {
                if d.dirty() && !matches!(d.save, Save::Running(_)) {
                    let job = d.start_save(&root, &journal);
                    tasks.push(Task::future(async move { Message::Saved(job.run()) }));
                }
            }
        }
        if self.quitting {
            tasks.push(self.autosave(true));
        }
        tasks.push(self.pump_vault_ops());
        tasks.push(self.quit_if_waiting());
        Task::batch(tasks)
    }

    /// Renames or moves a note, updating every link to it (F-10.6, F-10.7).
    fn rename_note(&mut self, from: String, to: String) -> Task<Message> {
        self.run_moves(
            vec![Moved {
                from: from.clone(),
                to: to.clone(),
            }],
            None,
            from,
            to,
        )
    }

    /// Renames or moves a folder with everything in it. Attachments are
    /// among the moves, so links to them follow too.
    /// The files are listed from the index when the move runs, so a note
    /// saved there or moved in by the change before is among them.
    fn rename_folder(&mut self, from: String, to: String) -> Task<Message> {
        self.run_moves(Vec::new(), Some((from.clone(), to.clone())), from, to)
    }

    fn run_moves(
        &mut self,
        moves: Vec<Moved>,
        folder: Option<(String, String)>,
        from: String,
        to: String,
    ) -> Task<Message> {
        // Text that cannot be saved would be lost when its note is read
        // again after the move: settle it first.
        if let Some(p) = self.unsaved_stuck(|_| true, true) {
            self.say(
                super::Level::Error,
                format!("Can't rename \"{from}\": \"{p}\" has unsaved changes"),
            );
            return Task::none();
        }
        // Open notes are saved first, so nothing on disk is newer than them.
        let save = self.autosave(true);
        let Some(w) = self.workspace_mut() else {
            return Task::none();
        };
        // No save starts at a path that is moving; text typed meanwhile is
        // saved at the new path once the move lands.
        w.vault_busy = true;
        let root: PathBuf = w.session.paths.root.clone();
        let journal = w.journal.clone();
        let job = Task::perform(
            w.session.query(move |store| {
                // The plan comes from the index: first brought up to date
                // (notes just saved, the change before this one), here, in
                // the same job. A rename with no up-to-date index, or no
                // list of a folder's files, would break links: refused.
                update_index(store, &root, &IndexOptions::default(), &mut |_| {})
                    .map_err(|e| format!("the index could not be updated: {e}"))?;
                let moves = match &folder {
                    Some((from, to)) => {
                        let prefix = format!("{from}/");
                        let files: Vec<String> = store
                            .file_states()
                            .map_err(|e| format!("the index could not be read: {e}"))?
                            .into_iter()
                            .map(|f| f.path)
                            .filter(|p| p.starts_with(&prefix))
                            .collect();
                        relink::folder_moves(from, to, &files)
                    }
                    None => moves,
                };
                commit_moves(store, &root, &journal, &moves, folder)
            }),
            move |result| {
                let result = result.unwrap_or_else(|| Err("the index is not available".into()));
                Message::Moved(from.clone(), to.clone(), result)
            },
        );
        save.chain(job)
    }

    /// Changes still queued follow a note or folder that moved (`to`); one
    /// about something sent to the trash (`None`) is dropped, and says so.
    pub(super) fn queued_paths_moved(&mut self, from: &str, to: Option<&str>) {
        let Some(w) = self.workspace_mut() else {
            return;
        };
        let follow = |p: &mut String| match moved_path(p, from, to) {
            None => true,
            Some(Some(new)) => {
                *p = new;
                true
            }
            Some(None) => false,
        };
        let mut dropped = Vec::new();
        let ops = std::mem::take(&mut w.vault_ops);
        for mut op in ops {
            let kept = match &mut op {
                // The destination follows too: a note renamed inside the
                // folder that moved stays in it.
                VaultOp::RenameNote { from, to } | VaultOp::RenameFolder { from, to } => {
                    follow(from) & follow(to)
                }
                VaultOp::Trash { path } => follow(path),
                VaultOp::Sync { pair, .. } => follow(&mut pair.original) & follow(&mut pair.copy),
            };
            if kept {
                w.vault_ops.push_back(op);
            } else {
                dropped.push(op.target().to_owned());
            }
        }
        // One message for all of them, naming what was deleted.
        let message = match dropped.as_slice() {
            [] => return,
            [one] if one == from => format!("Can't change \"{one}\": it was deleted"),
            [one] => format!("Can't change \"{one}\": \"{from}\" was deleted"),
            many => format!(
                "{} changes were not made: \"{from}\" was deleted",
                many.len()
            ),
        };
        self.say(super::Level::Error, message);
    }

    /// The jump list and recent notes follow a note or folder that moved
    /// (`to`); entries in one sent to the trash (`None`) are dropped.
    pub(super) fn paths_moved(&mut self, from: &str, to: Option<&str>) {
        let map = |p: &str| moved_path(p, from, to);
        let jumps = std::mem::take(&mut self.jumps);
        let mut at = self.jump_at;
        for (i, (p, c)) in jumps.into_iter().enumerate() {
            match map(&p) {
                None => self.jumps.push((p, c)),
                Some(Some(new)) => self.jumps.push((new, c)),
                Some(None) if i < self.jump_at => at -= 1,
                Some(None) => {}
            }
        }
        self.jump_at = at.min(self.jumps.len());
        if let Some(w) = self.workspace_mut() {
            w.state.recent.retain_mut(|r| match map(&r.path) {
                None => true,
                Some(Some(new)) => {
                    r.path = new;
                    true
                }
                Some(None) => false,
            });
        }
    }

    /// After a rename or move: open buffers follow the note to its new path,
    /// and notes whose links changed are read again from disk.
    pub(super) fn moved(
        &mut self,
        from: String,
        to: String,
        result: Result<Vec<String>, String>,
    ) -> Task<Message> {
        let changed = match result {
            Ok(c) => c,
            Err(e) => {
                self.say(super::Level::Error, format!("Can't rename \"{from}\": {e}"));
                return Task::none();
            }
        };
        let mut reload = Vec::new();
        let mut compare = Vec::new();
        if let Some(w) = self.workspace_mut() {
            for b in &mut w.buffers {
                let new_path = moved_path(&b.path, &from, Some(&to)).flatten();
                let rewritten = changed.contains(&b.path)
                    || new_path.as_ref().is_some_and(|p| changed.contains(p));
                let moved = new_path.is_some();
                if let Some(p) = new_path {
                    b.path = p;
                }
                match b.doc.as_mut() {
                    // Typed into while the move ran: the text stays, at the
                    // new path. When the move also rewrote its links, the
                    // disk differs and the usual banner lets the user pick.
                    Some(d) if d.dirty() => {
                        d.path = b.path.clone();
                        if rewritten {
                            compare.push(b.path.clone());
                        }
                    }
                    _ if moved || rewritten => reload.push(b.path.clone()),
                    _ => {}
                }
            }
        }
        // Notes closed while their save runs follow too: the rest of their
        // text is saved at the new path. When the move also rewrote their
        // links, that save is refused and the note comes back with the
        // banner, as an open one would.
        if let Some(w) = self.workspace_mut() {
            for d in &mut w.closing {
                if let Some(p) = moved_path(&d.path, &from, Some(&to)).flatten() {
                    d.path = p;
                }
            }
        }
        self.paths_moved(&from, Some(&to));
        self.queued_paths_moved(&from, Some(&to));
        let tasks: Vec<Task<Message>> = reload
            .into_iter()
            .filter_map(|p| {
                let root = self.workspace()?.session.paths.root.clone();
                if let Some(w) = self.workspace_mut()
                    && let Some(b) = w.buffers.iter_mut().find(|b| b.path == p)
                {
                    if let Some(d) = &b.doc {
                        let c = d.buffer.cursor();
                        self.pending_cursor.insert(p.clone(), (c.line, c.col));
                    }
                    if let Some(b) = self
                        .workspace_mut()
                        .and_then(|w| w.buffers.iter_mut().find(|b| b.path == p))
                    {
                        b.doc = None;
                    }
                }
                Some(super::read_note(root, p))
            })
            .collect();
        let compare: Vec<Task<Message>> = compare.into_iter().map(|p| self.read_disk(p)).collect();
        let refresh = self.refresh_index(to);
        Task::batch(
            tasks
                .into_iter()
                .chain(compare)
                .chain([refresh, self.save_vault_state()]),
        )
    }
}

/// Where `path` is after `from` moved to `to` (`None`: to the trash):
/// `None` when the move did not touch it, `Some(None)` when it went.
fn moved_path(path: &str, from: &str, to: Option<&str>) -> Option<Option<String>> {
    if path == from {
        return Some(to.map(str::to_owned));
    }
    let rest = path.strip_prefix(from)?.strip_prefix('/')?;
    Some(to.map(|t| format!("{t}/{rest}")))
}

/// A change to the vault's files other than a note's own save (F-10, F-11),
/// queued so that it and saves never overlap (`App::queue_vault_op`).
#[derive(Debug)]
pub enum VaultOp {
    RenameNote {
        from: String,
        to: String,
    },
    /// The files under it are listed when it runs, not when asked for.
    RenameFolder {
        from: String,
        to: String,
    },
    Trash {
        path: String,
    },
    /// Use copy or Keep original for a Syncthing conflict (F-11.6).
    Sync {
        choice: super::external::Choice,
        pair: Box<super::external::SyncPair>,
    },
}

impl VaultOp {
    /// A rename plans its link updates from the index.
    fn needs_index(&self) -> bool {
        matches!(self, Self::RenameNote { .. } | Self::RenameFolder { .. })
    }

    /// The path the change is about, for messages.
    fn target(&self) -> &str {
        match self {
            Self::RenameNote { from, .. } | Self::RenameFolder { from, .. } => from,
            Self::Trash { path } => path,
            Self::Sync { pair, .. } => &pair.copy,
        }
    }
}

/// Runs a vault change that needs no index (the trash, a Syncthing choice)
/// in the background; ends with `Message::FilesChanged(gone, …)`, which
/// refreshes the index afterwards.
pub(super) fn vault_job(
    gone: Vec<String>,
    write: impl FnOnce() -> Result<(), String> + Send + 'static,
) -> Task<Message> {
    Task::future(async move { Message::FilesChanged(gone, write()) })
}
