//! Watching a vault for changes made outside Patinote (PRD F-04).
//!
//! Only the GUI watches; the CLI and MCP server refresh the index on every
//! call (F-03). The watcher does not index by itself: it tells its handler
//! which paths changed, and the handler runs the incremental indexer, which
//! already handles renames, folders, atomic saves and mass changes.
//!
//! - inotify through `notify`, one non-recursive watch per visible folder.
//!   Hidden folders (`.git/`, `.obsidian/`, …) are not watched, so a large
//!   `.git/` cannot exhaust the inotify limit; `.patinote/` is, because its
//!   `ignore` file changes what is excluded.
//! - Events are collected until 100 ms pass without a new one, then handed
//!   over together, so a `git checkout` of 100 files becomes one change.
//! - Watches follow folders that are created, moved or deleted.
//! - When inotify cannot be used (limit reached, or another failure), the
//!   watcher falls back to asking for a rescan every 30 seconds and says why.

use std::collections::{BTreeSet, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use notify::{RecommendedWatcher, RecursiveMode, Watcher as _};

use crate::scan::{IGNORE_FILE, ScanError, relative};

/// Watcher settings.
#[derive(Debug, Clone, Copy)]
pub struct WatchOptions {
    /// Quiet time that ends a group of events (F-04: about 100 ms).
    pub debounce: Duration,
    /// Rescan interval when inotify is not available (F-04: 30 s).
    pub poll_interval: Duration,
    /// Skip inotify and poll from the start.
    pub force_polling: bool,
}

impl Default for WatchOptions {
    fn default() -> Self {
        Self {
            debounce: Duration::from_millis(100),
            poll_interval: Duration::from_secs(30),
            force_polling: false,
        }
    }
}

/// How changes are being detected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WatchMode {
    /// inotify events.
    Events,
    /// A rescan every `poll_interval`.
    Polling(PollReason),
}

/// Why the watcher polls instead of using inotify.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PollReason {
    /// `WatchOptions::force_polling` was set.
    Requested,
    /// The inotify watch limit (`fs.inotify.max_user_watches`) was reached.
    WatchLimit,
    /// inotify failed for another reason.
    Failed(String),
}

/// A group of changes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    /// Changed paths relative to the vault root, sorted. Paths of deleted or
    /// moved files are included; folders may appear too.
    pub paths: Vec<String>,
    /// The handler should rescan the whole vault: polling, an inotify queue
    /// overflow, or a switch to polling.
    pub rescan: bool,
    pub mode: WatchMode,
}

enum Msg {
    Event(notify::Result<notify::Event>),
    Stop,
}

/// A running watcher. Dropping it stops the watcher thread.
pub struct VaultWatcher {
    tx: Sender<Msg>,
    thread: Option<JoinHandle<()>>,
    mode: Arc<Mutex<WatchMode>>,
}

impl VaultWatcher {
    /// The current detection mode.
    pub fn mode(&self) -> WatchMode {
        self.mode.lock().map_or(WatchMode::Events, |m| m.clone())
    }
}

impl Drop for VaultWatcher {
    fn drop(&mut self) {
        let _ = self.tx.send(Msg::Stop);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// Starts watching the vault at `root`. It returns once every folder is
/// watched (or polling has started), so no change made afterwards is missed.
/// `on_change` runs on the watcher's own thread, once per group of changes.
pub fn watch(
    root: &Path,
    options: WatchOptions,
    on_change: impl FnMut(Change) + Send + 'static,
) -> Result<VaultWatcher, ScanError> {
    match std::fs::metadata(root) {
        Ok(m) if m.is_dir() => {}
        Ok(_) => return Err(ScanError::NotADirectory(root.to_path_buf())),
        Err(_) => return Err(ScanError::NotFound(root.to_path_buf())),
    }
    let (tx, rx) = mpsc::channel();
    let (ready_tx, ready_rx) = mpsc::channel();
    let mode = Arc::new(Mutex::new(WatchMode::Events));
    let state = Run {
        root: root.to_path_buf(),
        options,
        tx: tx.clone(),
        mode: Arc::clone(&mode),
        watcher: None,
        watched: HashSet::new(),
    };
    let thread = std::thread::Builder::new()
        .name("patinote-watch".into())
        .spawn(move || state.run(&rx, &ready_tx, on_change))
        .map_err(|source| ScanError::Io {
            path: root.to_path_buf(),
            source,
        })?;
    // The thread signals once its watches are in place.
    let _ = ready_rx.recv();
    Ok(VaultWatcher {
        tx,
        thread: Some(thread),
        mode,
    })
}

struct Run {
    root: PathBuf,
    options: WatchOptions,
    tx: Sender<Msg>,
    mode: Arc<Mutex<WatchMode>>,
    watcher: Option<RecommendedWatcher>,
    watched: HashSet<PathBuf>,
}

impl Run {
    fn set_mode(&self, mode: WatchMode) {
        if let Ok(mut m) = self.mode.lock() {
            *m = mode;
        }
    }

    fn current_mode(&self) -> WatchMode {
        self.mode.lock().map_or(WatchMode::Events, |m| m.clone())
    }

    /// Switches to polling and asks for a rescan so nothing is missed.
    fn fall_back(&mut self, reason: PollReason, on_change: &mut impl FnMut(Change)) {
        self.watcher = None;
        self.watched.clear();
        let mode = WatchMode::Polling(reason);
        self.set_mode(mode.clone());
        on_change(Change {
            paths: Vec::new(),
            rescan: true,
            mode,
        });
    }

    fn run(
        mut self,
        rx: &mpsc::Receiver<Msg>,
        ready: &Sender<()>,
        mut on_change: impl FnMut(Change),
    ) {
        if self.options.force_polling {
            self.set_mode(WatchMode::Polling(PollReason::Requested));
        } else {
            let tx = self.tx.clone();
            match notify::recommended_watcher(move |res| {
                let _ = tx.send(Msg::Event(res));
            }) {
                Ok(w) => {
                    self.watcher = Some(w);
                    if let Err(reason) = self.sync_watches() {
                        self.fall_back(reason, &mut on_change);
                    }
                }
                Err(e) => self.fall_back(poll_reason(&e), &mut on_change),
            }
        }
        let _ = ready.send(());

        loop {
            match self.current_mode() {
                WatchMode::Polling(_) => match rx.recv_timeout(self.options.poll_interval) {
                    Ok(Msg::Stop) | Err(RecvTimeoutError::Disconnected) => return,
                    Ok(Msg::Event(_)) => {}
                    Err(RecvTimeoutError::Timeout) => {
                        on_change(Change {
                            paths: Vec::new(),
                            rescan: true,
                            mode: self.current_mode(),
                        });
                    }
                },
                WatchMode::Events => {
                    let mut group = Group::default();
                    match rx.recv() {
                        Ok(Msg::Event(res)) => group.add(res, &self.root),
                        Ok(Msg::Stop) | Err(_) => return,
                    }
                    // Collect until the vault has been quiet for `debounce`.
                    loop {
                        match rx.recv_timeout(self.options.debounce) {
                            Ok(Msg::Event(res)) => group.add(res, &self.root),
                            Ok(Msg::Stop) | Err(RecvTimeoutError::Disconnected) => return,
                            Err(RecvTimeoutError::Timeout) => break,
                        }
                    }
                    if let Some(reason) = group.failure.take() {
                        self.fall_back(reason, &mut on_change);
                        continue;
                    }
                    if group.paths.is_empty() && !group.rescan {
                        continue;
                    }
                    // Watch new folders before the handler rescans: files created
                    // after this are reported, files created before it are found
                    // by the handler's scan. The other order would miss files
                    // created in between.
                    if let Err(reason) = self.sync_watches() {
                        self.fall_back(reason, &mut on_change);
                        continue;
                    }
                    on_change(Change {
                        paths: group.paths.into_iter().collect(),
                        rescan: group.rescan,
                        mode: WatchMode::Events,
                    });
                }
            }
        }
    }

    /// Watches every visible folder (and `.patinote/`), and stops watching
    /// folders that are gone.
    fn sync_watches(&mut self) -> Result<(), PollReason> {
        let Some(watcher) = self.watcher.as_mut() else {
            return Ok(());
        };
        let wanted = watched_dirs(&self.root);
        for dir in wanted.difference(&self.watched) {
            if let Err(e) = watcher.watch(dir, RecursiveMode::NonRecursive) {
                if matches!(e.kind, notify::ErrorKind::PathNotFound) || is_not_found(&e) {
                    continue; // Removed in the meantime.
                }
                return Err(poll_reason(&e));
            }
        }
        for dir in self.watched.difference(&wanted) {
            let _ = watcher.unwatch(dir);
        }
        self.watched = wanted;
        Ok(())
    }
}

fn is_not_found(e: &notify::Error) -> bool {
    matches!(&e.kind, notify::ErrorKind::Io(io) if io.kind() == std::io::ErrorKind::NotFound)
}

fn poll_reason(e: &notify::Error) -> PollReason {
    match &e.kind {
        notify::ErrorKind::MaxFilesWatch => PollReason::WatchLimit,
        // ENOSPC from inotify_add_watch is the watch limit as well.
        notify::ErrorKind::Io(io) if io.raw_os_error() == Some(28) => PollReason::WatchLimit,
        _ => PollReason::Failed(e.to_string()),
    }
}

/// The folders to watch: the root, every visible folder below it, and
/// `.patinote/` when it exists. Symlinks are not followed (F-01).
pub fn watched_dirs(root: &Path) -> HashSet<PathBuf> {
    let mut dirs: HashSet<PathBuf> = ignore::WalkBuilder::new(root)
        .standard_filters(false)
        .hidden(true)
        .follow_links(false)
        .build()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_some_and(|t| t.is_dir()))
        .map(|e| e.into_path())
        .collect();
    let patinote = root.join(".patinote");
    if patinote.is_dir() {
        dirs.insert(patinote);
    }
    dirs
}

#[derive(Default)]
struct Group {
    paths: BTreeSet<String>,
    rescan: bool,
    failure: Option<PollReason>,
}

impl Group {
    fn add(&mut self, res: notify::Result<notify::Event>, root: &Path) {
        match res {
            Ok(event) => {
                // Opening or reading a file or folder changes nothing. Ignoring
                // it also keeps the watcher's own folder walk from triggering
                // itself in a loop.
                if matches!(event.kind, notify::EventKind::Access(_)) {
                    return;
                }
                if event.need_rescan() {
                    self.rescan = true;
                }
                for path in &event.paths {
                    if let Some(rel) = relative(path, root).filter(|r| is_relevant(r)) {
                        self.paths.insert(rel);
                    }
                }
            }
            Err(e) => match poll_reason(&e) {
                PollReason::WatchLimit => self.failure = Some(PollReason::WatchLimit),
                // An unknown problem: rescan to be safe and keep watching.
                _ => self.rescan = true,
            },
        }
    }
}

/// Whether a changed path matters to the index: visible files and folders,
/// plus `.patinote/ignore`. Editor backups (`name~`) are skipped; swap and
/// Syncthing temporary files are hidden anyway.
fn is_relevant(rel: &str) -> bool {
    if rel == IGNORE_FILE {
        return true;
    }
    if rel.is_empty() || rel.split('/').any(|part| part.starts_with('.')) {
        return false;
    }
    !rel.ends_with('~')
}

/// The inotify watch limit, from `/proc/sys/fs/inotify/max_user_watches`.
pub fn inotify_watch_limit() -> Option<u64> {
    std::fs::read_to_string("/proc/sys/fs/inotify/max_user_watches")
        .ok()
        .and_then(|s| s.trim().parse().ok())
}

/// Shown when the vault needs too many watches (F-04).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WatchLimitWarning {
    /// Folders the vault needs watched.
    pub needed: u64,
    /// The current `fs.inotify.max_user_watches`.
    pub limit: u64,
}

impl WatchLimitWarning {
    /// A limit that leaves room for the vault and for other programs.
    pub fn suggested_limit(&self) -> u64 {
        (self.needed * 4).max(524_288)
    }

    /// The command the warning suggests.
    pub fn sysctl_command(&self) -> String {
        format!(
            "sudo sysctl fs.inotify.max_user_watches={}",
            self.suggested_limit()
        )
    }
}

/// Checks whether the vault fits in the inotify limit, keeping half of it for
/// other programs, which share the same per-user limit.
pub fn check_watch_limit(needed: u64, limit: u64) -> Option<WatchLimitWarning> {
    (needed > limit / 2).then_some(WatchLimitWarning { needed, limit })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relevance() {
        assert!(is_relevant("a.md"));
        assert!(is_relevant("Proyek/b.md"));
        assert!(is_relevant(".patinote/ignore"));
        assert!(!is_relevant(".patinote/config.toml"));
        assert!(!is_relevant(".obsidian/app.json"));
        assert!(!is_relevant("Proyek/.note.md.swp"));
        assert!(!is_relevant("Proyek/.syncthing.note.md.tmp"));
        assert!(!is_relevant("note.md~"));
        assert!(!is_relevant(""));
    }

    #[test]
    fn limits() {
        assert_eq!(check_watch_limit(100, 65_536), None);
        let w = check_watch_limit(40_000, 65_536).unwrap();
        assert_eq!(
            w.sysctl_command(),
            "sudo sysctl fs.inotify.max_user_watches=524288"
        );
        assert_eq!(
            check_watch_limit(200_000, 65_536)
                .unwrap()
                .suggested_limit(),
            800_000
        );
    }
}
