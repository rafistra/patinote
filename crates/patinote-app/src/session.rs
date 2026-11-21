//! An open vault: where its data lives, its index on a background thread,
//! and the socket other Patinote processes reach it on.
//!
//! The UI never waits on the index (CLAUDE.md section 5): one worker thread
//! owns the SQLite connection, runs the index update when the vault opens,
//! then answers queries sent as jobs. Answers come back as futures, which
//! the app turns into messages with `Task::perform`.

use std::path::{Path, PathBuf};
use std::sync::mpsc;

use iced::futures::Stream;
use iced::futures::channel::{mpsc as async_mpsc, oneshot};
use patinote_core::write::{Journal, recover};
use patinote_core::{IndexOptions, Progress, location};
use patinote_index::{OpenOptions, SqliteStore, update_index};

/// A unit of work for the index thread.
type Job = Box<dyn FnOnce(&mut SqliteStore) + Send>;

/// What the index thread reports while the vault opens.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IndexEvent {
    /// Notes stored so far out of those that changed (F-03).
    Progress { done: usize, total: usize },
    /// The index is up to date.
    Ready { notes: usize },
    /// The index could not be opened or updated.
    Failed(String),
    /// How the watcher detects changes from outside (F-04).
    Watching(patinote_core::WatchMode),
    /// The vault needs more inotify watches than the system allows (F-04).
    WatchLimit { command: String },
    /// Writes left half-done by a crash were undone, first thing, or why
    /// some could not be. Notes are read only after this.
    Recovered(Option<String>),
}

/// Where a vault's files and data are.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VaultPaths {
    /// Canonical vault folder.
    pub root: PathBuf,
    pub id: String,
    /// The vault's name: its folder name.
    pub name: String,
    pub index: PathBuf,
    /// `$XDG_STATE_HOME`.
    pub state_home: PathBuf,
}

impl VaultPaths {
    /// Resolves the folder `root`, with its data under `cache_home` and
    /// `state_home`. Fails when it does not exist or is not a folder.
    pub fn resolve(root: &Path, cache_home: &Path, state_home: &Path) -> std::io::Result<Self> {
        let root = std::fs::canonicalize(root)?;
        if !root.is_dir() {
            return Err(std::io::ErrorKind::NotADirectory.into());
        }
        let id = location::vault_id_of_canonical(&root);
        let cache = cache_home;
        let state_home = state_home.to_path_buf();
        let name = root.file_name().map_or_else(
            || root.display().to_string(),
            |n| n.to_string_lossy().into_owned(),
        );
        Ok(Self {
            index: location::vault_cache_dir(cache, &id).join("index.db"),
            root,
            id,
            name,
            state_home,
        })
    }
}

/// An open vault and its index thread.
#[derive(Debug, Clone)]
pub struct Session {
    pub paths: VaultPaths,
    jobs: mpsc::Sender<Job>,
}

impl Session {
    /// Starts the index thread for `paths`. The stream reports the opening
    /// index update and ends once the index is ready or has failed.
    pub fn open(paths: VaultPaths) -> (Self, impl Stream<Item = IndexEvent>) {
        let (session, events, _) = Self::open_watching(paths, false);
        (session, events)
    }

    /// Like [`Session::open`], and with `watch` also starts the file watcher
    /// once the index is ready (F-04). The second stream gives each group of
    /// changes from outside; it never ends while the session lives.
    pub fn open_watching(
        paths: VaultPaths,
        watch: bool,
    ) -> (
        Self,
        impl Stream<Item = IndexEvent>,
        impl Stream<Item = patinote_core::Change>,
    ) {
        let (jobs, rx) = mpsc::channel::<Job>();
        let (events, stream) = async_mpsc::unbounded();
        let (changes, change_stream) = async_mpsc::unbounded();
        let worker = paths.clone();
        let fallback = events.clone();
        if let Err(e) = std::thread::Builder::new()
            .name("patinote-index".into())
            .spawn(move || run(&worker, &rx, &events, watch.then_some(changes)))
        {
            not_started(&paths, &fallback, &e);
        }
        (Self { paths, jobs }, stream, change_stream)
    }

    /// Runs `f` on the index thread. `None` when the index is not available.
    pub fn query<T, F>(&self, f: F) -> impl Future<Output = Option<T>> + use<T, F>
    where
        T: Send + 'static,
        F: FnOnce(&mut SqliteStore) -> T + Send + 'static,
    {
        let (tx, rx) = oneshot::channel();
        let sent = self.jobs.send(Box::new(move |store| {
            let _ = tx.send(f(store));
        }));
        async move {
            match sent {
                Ok(()) => rx.await.ok(),
                Err(_) => None,
            }
        }
    }
}

/// The index thread could not start: writes left half-done are undone
/// here instead (quick, and rare), so notes still open, and the index is
/// reported failed.
fn not_started(
    paths: &VaultPaths,
    events: &async_mpsc::UnboundedSender<IndexEvent>,
    e: &std::io::Error,
) {
    tracing::error!("cannot start the index thread: {e}");
    let _ = events.unbounded_send(IndexEvent::Recovered(recover_writes(paths)));
    let _ = events.unbounded_send(IndexEvent::Failed(format!(
        "cannot start the index thread: {e}"
    )));
}

/// Undoes writes a crash left half-done; the error, if any, as text.
fn recover_writes(paths: &VaultPaths) -> Option<String> {
    let journal = Journal::for_vault(&paths.state_home, &paths.id);
    recover(&paths.root, &journal).err().map(|e| {
        tracing::warn!("cannot recover interrupted writes: {e}");
        e.to_string()
    })
}

fn run(
    paths: &VaultPaths,
    jobs: &mpsc::Receiver<Job>,
    events: &async_mpsc::UnboundedSender<IndexEvent>,
    changes: Option<async_mpsc::UnboundedSender<patinote_core::Change>>,
) {
    let send = |e| {
        let _ = events.unbounded_send(e);
    };
    // Writes left half-done by a crash are undone before anything else
    // (docs/design/0001; PRD "Gerbang 2", point 1).
    send(IndexEvent::Recovered(recover_writes(paths)));
    if let Some(dir) = paths.index.parent()
        && let Err(e) = std::fs::create_dir_all(dir)
    {
        send(IndexEvent::Failed(format!(
            "cannot create the index folder: {e}"
        )));
        return;
    }
    let mut store = match SqliteStore::open(&paths.index, &OpenOptions::default()) {
        Ok(s) => s,
        Err(e) => {
            send(IndexEvent::Failed(format!("cannot open the index: {e}")));
            return;
        }
    };
    let mut progress = |p: Progress| {
        if let Progress::Stored { done, changed } = p {
            send(IndexEvent::Progress {
                done,
                total: changed,
            });
        }
    };
    match update_index(
        &mut store,
        &paths.root,
        &IndexOptions::default(),
        &mut progress,
    ) {
        Ok(_) => {
            let notes = store.counts().map_or(0, |c| c.notes);
            send(IndexEvent::Ready { notes });
        }
        Err(e) => send(IndexEvent::Failed(format!("cannot index the vault: {e}"))),
    }
    // The watcher lives as long as this thread; changes go to the app, which
    // asks this thread to update the index.
    let _watcher = changes.and_then(|tx| {
        let dirs = patinote_core::watch::watched_dirs(&paths.root).len() as u64;
        if let Some(limit) = patinote_core::watch::inotify_watch_limit()
            && let Some(w) = patinote_core::watch::check_watch_limit(dirs, limit)
        {
            send(IndexEvent::WatchLimit {
                command: w.sysctl_command(),
            });
        }
        let watcher = patinote_core::watch(
            &paths.root,
            patinote_core::WatchOptions::default(),
            move |c| {
                let _ = tx.unbounded_send(c);
            },
        );
        match watcher {
            Ok(w) => {
                send(IndexEvent::Watching(w.mode()));
                Some(w)
            }
            Err(e) => {
                tracing::warn!("cannot watch the vault: {e}");
                None
            }
        }
    });
    events.close_channel();
    while let Ok(job) = jobs.recv() {
        job(&mut store);
    }
}

/// What a name typed by the user (`:e name`, `--open`) points to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Lookup {
    /// One note, by path.
    Found(String),
    /// Several notes match, the first is the one a link would choose.
    Ambiguous(Vec<String>),
    NotFound,
}

/// Finds the note `name` refers to, with the rules of `patinote read`: an
/// exact path (with or without `.md`), then the wikilink rules.
pub fn lookup(store: &SqliteStore, name: &str) -> Lookup {
    use patinote_core::model::LinkKind;
    use patinote_core::names::nfc;

    let name = nfc(name.trim());
    if name.is_empty() || name.starts_with('/') || name.split('/').any(|p| p == "..") {
        return Lookup::NotFound;
    }
    for candidate in [name.clone(), format!("{name}.md")] {
        if let Ok(Some(row)) = store.note_by_path(&candidate) {
            return Lookup::Found(row.path);
        }
    }
    let target = name
        .trim_start_matches("[[")
        .trim_end_matches("]]")
        .split(['|', '#'])
        .next()
        .unwrap_or_default();
    let Ok(candidates) = store.link_candidates(target) else {
        return Lookup::NotFound;
    };
    let resolver = patinote_core::Resolver::new(candidates);
    match resolver.resolve("", LinkKind::Wiki, target) {
        None => Lookup::NotFound,
        Some(r) if r.is_ambiguous() => {
            Lookup::Ambiguous(r.candidates.iter().map(|c| (*c).to_owned()).collect())
        }
        Some(r) => Lookup::Found(r.path.to_owned()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced::futures::StreamExt;
    use iced::futures::executor::block_on;

    /// Without the index thread, notes still open (they wait for
    /// `Recovered`) and the index shows as failed.
    #[test]
    fn a_thread_that_cannot_start_still_reports_recovery() {
        let dir = tempfile::tempdir().unwrap();
        let vault = dir.path().join("vault");
        std::fs::create_dir(&vault).unwrap();
        let paths =
            VaultPaths::resolve(&vault, &dir.path().join("cache"), &dir.path().join("state"))
                .unwrap();
        let (tx, rx) = async_mpsc::unbounded();
        not_started(&paths, &tx, &std::io::Error::other("no threads"));
        drop(tx);
        let events: Vec<IndexEvent> = block_on(rx.collect());
        assert!(
            matches!(events[0], IndexEvent::Recovered(None)),
            "{events:?}"
        );
        assert!(matches!(events[1], IndexEvent::Failed(_)), "{events:?}");
    }
}
