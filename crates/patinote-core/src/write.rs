//! The only way Patinote writes to a vault (PRD "Gerbang 2: dampak
//! arsitektur", point 1; CLAUDE.md section 5).
//!
//! A [`Transaction`] lists file changes, each with the state the file is
//! expected to have first. [`plan`] checks them and builds diffs without
//! writing anything. [`commit`]:
//! 1. checks every expectation; if any file changed, nothing is written;
//! 2. writes a journal entry with the old contents, synced to disk, before
//!    touching the vault (write-ahead);
//! 3. applies the changes: writes are atomic (temporary file in the same
//!    folder, sync, rename) and keep the file's permissions; deletes move to
//!    `.trash/`;
//! 4. if an I/O error stops it halfway, undoes what was applied, from the
//!    journal, in reverse order;
//! 5. marks the entry committed and prunes old entries.
//!
//! An entry still marked pending after a crash is undone by [`recover`], so a
//! transaction is applied completely or not at all.
//!
//! Bytes are written exactly as given: keeping line endings, a BOM and the
//! final newline is the editor's job (F-05.4).

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{self, Write as _};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::store::Hash;

/// The folder deleted files move to, relative to the vault root.
pub const TRASH: &str = ".trash";

/// Journal retention rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Retention {
    /// Every entry younger than this is kept.
    pub keep_seconds: u64,
    /// At least this many of the newest entries are kept, however old.
    pub min_entries: usize,
    /// Hard limit on the journal's size; it wins over the two rules above.
    pub max_bytes: u64,
}

impl Retention {
    /// The PRD rules: every entry of the last 7 days, at least the last 100
    /// entries, and never more than 100 MB in total.
    pub const PRD: Self = Self {
        keep_seconds: 7 * 24 * 60 * 60,
        min_entries: 100,
        max_bytes: 100 * 1024 * 1024,
    };
}

/// Who made a change; stored in the journal for undo and the agent log (A-07).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Actor {
    User,
    Cli,
    Agent(String),
}

impl Actor {
    /// `user`, `cli` or `agent:<name>`.
    pub fn as_label(&self) -> String {
        match self {
            Self::User => "user".into(),
            Self::Cli => "cli".into(),
            Self::Agent(name) => format!("agent:{name}"),
        }
    }
}

/// The state a path must have before a change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expect {
    /// Nothing exists at the path.
    Absent,
    /// A file with these contents.
    Hash(Hash),
    /// A folder with exactly these files (path relative to the folder → hash).
    Tree(BTreeMap<String, Hash>),
}

/// One change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Op {
    /// Create (`expect: Absent`) or replace a file.
    Write {
        path: String,
        content: Vec<u8>,
        expect: Expect,
    },
    /// Move or rename a file or folder; `to` must not exist. `content`
    /// replaces a file's contents in the same step (a rename that also updates
    /// links inside the note).
    Move {
        from: String,
        to: String,
        content: Option<Vec<u8>>,
        expect: Expect,
    },
    /// Move a file or folder to `.trash/`, keeping its relative path.
    Trash { path: String, expect: Expect },
}

impl Op {
    fn paths(&self) -> Vec<&str> {
        match self {
            Op::Write { path, .. } | Op::Trash { path, .. } => vec![path],
            Op::Move { from, to, .. } => vec![from, to],
        }
    }
}

/// A set of changes applied together.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transaction {
    pub actor: Actor,
    pub ops: Vec<Op>,
}

/// Why a change is not possible.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Conflict {
    /// The file or folder is not in the expected state (changed outside, F-11).
    Changed,
    /// The destination of a move already exists.
    DestinationExists,
}

/// What [`plan`] found for one operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedOp {
    /// `write`, `create`, `move` or `trash`.
    pub action: &'static str,
    pub path: String,
    /// Destination of a move, or where a deleted path goes in `.trash/`.
    pub to: Option<String>,
    /// Unified line diff for text files whose content changes.
    pub diff: Option<String>,
    pub conflict: Option<Conflict>,
}

/// The result of [`plan`]: what [`commit`] would do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    pub ops: Vec<PlannedOp>,
}

impl Plan {
    /// Paths whose expectation fails.
    pub fn conflicts(&self) -> Vec<&str> {
        self.ops
            .iter()
            .filter(|o| o.conflict.is_some())
            .map(|o| o.path.as_str())
            .collect()
    }
}

/// Why a transaction was not applied.
#[derive(Debug, thiserror::Error)]
pub enum WriteError {
    /// The path is not allowed (outside the vault, hidden, `.obsidian/`, …).
    #[error("cannot write {path}: {reason}")]
    InvalidPath { path: String, reason: &'static str },
    /// Files changed since they were read; nothing was written.
    #[error("files changed since they were read: {}", paths.join(", "))]
    Conflict { paths: Vec<String> },
    /// I/O failed before the vault was touched.
    #[error("cannot write {}: {source}", path.display())]
    Io { path: PathBuf, source: io::Error },
    /// I/O failed halfway; everything applied was undone.
    #[error("writing {path} failed and the transaction was undone: {source}")]
    RolledBack { path: String, source: io::Error },
    /// I/O failed halfway and undoing failed too; the journal entry stays
    /// pending so [`recover`] can finish the undo.
    #[error("writing {path} failed and could not be fully undone; recovery is pending: {source}")]
    RollbackFailed { path: String, source: io::Error },
}

fn io_err(path: &Path) -> impl Fn(io::Error) -> WriteError + '_ {
    move |source| WriteError::Io {
        path: path.to_path_buf(),
        source,
    }
}

// ---------------------------------------------------------------------------
// Paths

/// Checks that a vault-relative path may be written: relative, inside the
/// vault, not hidden (no `..`, no `.obsidian/`).
pub fn check_path(path: &str) -> Result<(), WriteError> {
    let bad = |reason| {
        Err(WriteError::InvalidPath {
            path: path.to_owned(),
            reason,
        })
    };
    if path.is_empty() || path.starts_with('/') {
        return bad("the path must be relative to the vault");
    }
    if path.chars().any(char::is_control) {
        return bad("the path contains control characters");
    }
    let parts: Vec<&str> = path.split('/').collect();
    if parts
        .iter()
        .any(|p| p.is_empty() || *p == "." || *p == "..")
    {
        return bad("the path must stay inside the vault");
    }
    for (i, part) in parts.iter().enumerate() {
        if part.starts_with('.') && !(i == 0 && *part == ".patinote" && parts.len() > 1) {
            return bad("hidden files and folders (.obsidian, .git, .trash, …) are never written");
        }
    }
    Ok(())
}

/// A note that is a symlink is written through the link, to the file it
/// points to, and only when that file is itself a note the vault may write:
/// inside the vault, not hidden. Anything else is refused, so the link is
/// never replaced by a copy and nothing outside the vault is touched.
fn check_link(root: &Path, path: &str) -> Result<(), WriteError> {
    let full = root.join(path);
    let is_link = fs::symlink_metadata(&full).is_ok_and(|m| m.file_type().is_symlink());
    if !is_link {
        return Ok(());
    }
    let bad = |reason| WriteError::InvalidPath {
        path: path.to_owned(),
        reason,
    };
    let target =
        fs::canonicalize(&full).map_err(|_| bad("the note links to a file that does not exist"))?;
    let root = fs::canonicalize(root).map_err(io_err(root))?;
    let rel = target
        .strip_prefix(&root)
        .map_err(|_| bad("the note links to a file outside the vault"))?;
    check_path(&rel.to_string_lossy())
        .map_err(|_| bad("the note links to a file the vault may not write"))
}

/// The hash [`Expect::Hash`] compares against: blake3 of the file's bytes.
pub fn content_hash(bytes: &[u8]) -> Hash {
    *blake3::hash(bytes).as_bytes()
}

fn hash_file(path: &Path) -> io::Result<Hash> {
    Ok(*blake3::hash(&fs::read(path)?).as_bytes())
}

/// Hashes of every file under `dir`, relative to it.
fn tree(dir: &Path) -> io::Result<BTreeMap<String, Hash>> {
    let mut out = BTreeMap::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for entry in fs::read_dir(&d)? {
            let entry = entry?;
            let p = entry.path();
            if entry.file_type()?.is_dir() {
                stack.push(p);
            } else {
                let rel = p
                    .strip_prefix(dir)
                    .unwrap_or(&p)
                    .to_string_lossy()
                    .replace('\\', "/");
                out.insert(rel, hash_file(&p)?);
            }
        }
    }
    Ok(out)
}

/// The current state of a vault path, to build an [`Op`]'s expectation.
pub fn expect_current(root: &Path, path: &str) -> io::Result<Expect> {
    let full = root.join(path);
    match fs::symlink_metadata(&full) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Expect::Absent),
        Err(e) => Err(e),
        Ok(m) if m.is_dir() => Ok(Expect::Tree(tree(&full)?)),
        Ok(_) => Ok(Expect::Hash(hash_file(&full)?)),
    }
}

fn matches(root: &Path, path: &str, expect: &Expect) -> io::Result<bool> {
    Ok(&expect_current(root, path)? == expect)
}

/// Where `path` goes in the trash: `.trash/<path>`, or with the deletion time
/// added before the extension when that name is taken (F-10.4).
fn trash_target(root: &Path, path: &str, now: SystemTime) -> String {
    let plain = format!("{TRASH}/{path}");
    if fs::symlink_metadata(root.join(&plain)).is_err() {
        return plain;
    }
    let stamp = timestamp(now);
    let (dir, name) = path.rsplit_once('/').map_or(("", path), |(d, n)| (d, n));
    let (stem, ext) = match name.rsplit_once('.') {
        Some((s, e)) if !s.is_empty() => (s, format!(".{e}")),
        _ => (name, String::new()),
    };
    let prefix = if dir.is_empty() {
        TRASH.to_owned()
    } else {
        format!("{TRASH}/{dir}")
    };
    let mut n = 1;
    loop {
        let suffix = if n == 1 {
            String::new()
        } else {
            format!("-{n}")
        };
        let candidate = format!("{prefix}/{stem} {stamp}{suffix}{ext}");
        if fs::symlink_metadata(root.join(&candidate)).is_err() {
            return candidate;
        }
        n += 1;
    }
}

/// `YYYYMMDD-HHMMSS` in UTC.
fn timestamp(t: SystemTime) -> String {
    let secs = t.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs()) as i64;
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    // Howard Hinnant's civil_from_days.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!(
        "{y:04}{m:02}{d:02}-{:02}{:02}{:02}",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

// ---------------------------------------------------------------------------
// Plan

fn validate(tx: &Transaction) -> Result<(), WriteError> {
    let mut seen = BTreeSet::new();
    for op in &tx.ops {
        for p in op.paths() {
            check_path(p)?;
            if !seen.insert(p.to_owned()) {
                return Err(WriteError::InvalidPath {
                    path: p.to_owned(),
                    reason: "each path may appear in only one change of a transaction",
                });
            }
        }
    }
    Ok(())
}

fn diff(path: &str, old: &[u8], new: &[u8]) -> Option<String> {
    let (old, new) = (
        std::str::from_utf8(old).ok()?,
        std::str::from_utf8(new).ok()?,
    );
    if old == new {
        return None;
    }
    Some(
        similar::TextDiff::from_lines(old, new)
            .unified_diff()
            .context_radius(3)
            .header(path, path)
            .to_string(),
    )
}

/// Checks a transaction and describes it, without writing anything.
pub fn plan(root: &Path, tx: &Transaction) -> Result<Plan, WriteError> {
    validate(tx)?;
    let now = SystemTime::now();
    let mut ops = Vec::with_capacity(tx.ops.len());
    for op in &tx.ops {
        let planned = match op {
            Op::Write {
                path,
                content,
                expect,
            } => {
                check_link(root, path)?;
                let ok = matches(root, path, expect).map_err(io_err(&root.join(path)))?;
                let old = fs::read(root.join(path)).unwrap_or_default();
                PlannedOp {
                    action: if *expect == Expect::Absent {
                        "create"
                    } else {
                        "write"
                    },
                    path: path.clone(),
                    to: None,
                    diff: diff(path, &old, content),
                    conflict: (!ok).then_some(Conflict::Changed),
                }
            }
            Op::Move {
                from,
                to,
                content,
                expect,
            } => {
                // New bytes would land in a plain file at `to` and the link
                // would go: the note would silently become a copy.
                if content.is_some()
                    && fs::symlink_metadata(root.join(from))
                        .is_ok_and(|m| m.file_type().is_symlink())
                {
                    return Err(WriteError::InvalidPath {
                        path: from.clone(),
                        reason: "the note is a link; its own links can't change while it moves",
                    });
                }
                let ok = matches(root, from, expect).map_err(io_err(&root.join(from)))?;
                let taken = fs::symlink_metadata(root.join(to)).is_ok();
                let diff = content
                    .as_ref()
                    .and_then(|c| diff(from, &fs::read(root.join(from)).unwrap_or_default(), c));
                PlannedOp {
                    action: "move",
                    path: from.clone(),
                    to: Some(to.clone()),
                    diff,
                    conflict: if !ok {
                        Some(Conflict::Changed)
                    } else if taken {
                        Some(Conflict::DestinationExists)
                    } else {
                        None
                    },
                }
            }
            Op::Trash { path, expect } => {
                let ok = matches(root, path, expect).map_err(io_err(&root.join(path)))?;
                PlannedOp {
                    action: "trash",
                    path: path.clone(),
                    to: Some(trash_target(root, path, now)),
                    diff: None,
                    conflict: (!ok).then_some(Conflict::Changed),
                }
            }
        };
        ops.push(planned);
    }
    Ok(Plan { ops })
}

// ---------------------------------------------------------------------------
// Journal

/// The write journal of one vault.
#[derive(Debug, Clone)]
pub struct Journal {
    dir: PathBuf,
}

/// Status of a journal entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntryStatus {
    /// Written before the vault was touched; not finished.
    Pending,
    Committed,
    /// Undone after a failure or a crash.
    RolledBack,
}

/// One recorded change, enough to undo it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EntryOp {
    /// `before` names the saved old contents, or is `None` for a new file.
    Write {
        path: String,
        before: Option<String>,
        after: String,
    },
    /// `before` names the old contents when the move also rewrote the file.
    Move {
        from: String,
        to: String,
        before: Option<String>,
        after: Option<String>,
    },
    Trash {
        path: String,
        to: String,
    },
}

/// A journal entry's manifest (`manifest.json`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    pub v: u32,
    pub id: String,
    /// Unix seconds.
    pub time: u64,
    pub actor: String,
    pub status: EntryStatus,
    pub ops: Vec<EntryOp>,
    /// Folders the transaction created, removed again on undo when empty.
    pub created_dirs: Vec<String>,
}

static SEQ: AtomicU64 = AtomicU64::new(0);

fn hex(h: &Hash) -> String {
    h.iter().map(|b| format!("{b:02x}")).collect()
}

fn sync_dir(dir: &Path) -> io::Result<()> {
    fs::File::open(dir)?.sync_all()
}

impl Journal {
    /// A journal in `dir` (normally `<state home>/patinote/<vault-id>/journal`).
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    /// The journal of a vault in the standard state folder.
    pub fn for_vault(state_home: &Path, vault_id: &str) -> Self {
        Self::new(crate::location::vault_state_dir(state_home, vault_id).join("journal"))
    }

    /// The journal folder.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Entry ids, oldest first.
    pub fn ids(&self) -> io::Result<Vec<String>> {
        let mut ids: Vec<String> = match fs::read_dir(&self.dir) {
            Ok(rd) => rd
                .filter_map(Result::ok)
                .filter(|e| e.path().join("manifest.json").is_file())
                .filter_map(|e| e.file_name().into_string().ok())
                .collect(),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Vec::new(),
            Err(e) => return Err(e),
        };
        ids.sort();
        Ok(ids)
    }

    /// Reads an entry's manifest.
    pub fn entry(&self, id: &str) -> io::Result<Entry> {
        let text = fs::read_to_string(self.dir.join(id).join("manifest.json"))?;
        serde_json::from_str(&text).map_err(io::Error::other)
    }

    /// The saved old contents named in an entry.
    pub fn blob(&self, id: &str, name: &str) -> io::Result<Vec<u8>> {
        fs::read(self.dir.join(id).join(name))
    }

    fn write_manifest(&self, entry: &Entry) -> io::Result<()> {
        let dir = self.dir.join(&entry.id);
        let json = serde_json::to_vec_pretty(entry).map_err(io::Error::other)?;
        atomic_write(&dir.join("manifest.json"), &json, None)
    }

    fn total_bytes(&self, id: &str) -> u64 {
        fs::read_dir(self.dir.join(id))
            .map(|rd| {
                rd.filter_map(Result::ok)
                    .filter_map(|e| e.metadata().ok())
                    .map(|m| m.len())
                    .sum()
            })
            .unwrap_or(0)
    }

    /// Applies the PRD retention rules. Pending entries are never removed.
    pub fn prune(&self, now: SystemTime) -> io::Result<Vec<String>> {
        self.prune_with(now, &Retention::PRD)
    }

    /// Applies `rules`. Pending entries are never removed. Returns the ids removed.
    pub fn prune_with(&self, now: SystemTime, rules: &Retention) -> io::Result<Vec<String>> {
        let now_s = now.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
        let mut entries: Vec<(String, u64, u64)> = Vec::new(); // id, time, bytes
        for id in self.ids()? {
            let Ok(e) = self.entry(&id) else { continue };
            if e.status == EntryStatus::Pending {
                continue;
            }
            entries.push((id.clone(), e.time, self.total_bytes(&id)));
        }
        let n = entries.len();
        let mut remove: BTreeSet<String> = BTreeSet::new();
        for (i, (id, time, _)) in entries.iter().enumerate() {
            let recent = now_s.saturating_sub(*time) <= rules.keep_seconds;
            let among_last = i + rules.min_entries >= n;
            if !recent && !among_last {
                remove.insert(id.clone());
            }
        }
        // The hard size limit wins over both rules, oldest first.
        let mut total: u64 = entries
            .iter()
            .filter(|e| !remove.contains(&e.0))
            .map(|e| e.2)
            .sum();
        for (id, _, bytes) in &entries {
            if total <= rules.max_bytes {
                break;
            }
            if remove.insert(id.clone()) {
                total -= bytes;
            }
        }
        for id in &remove {
            fs::remove_dir_all(self.dir.join(id))?;
        }
        Ok(remove.into_iter().collect())
    }
}

// ---------------------------------------------------------------------------
// Commit

/// Writes `bytes` to `path` atomically: a hidden temporary file in the same
/// folder, synced, then renamed over the target. `mode` keeps permissions.
/// A symlink is written through: the file it points to is replaced, and
/// the link stays (callers check where it points, see `check_link`).
fn atomic_write(path: &Path, bytes: &[u8], mode: Option<u32>) -> io::Result<()> {
    let resolved;
    let path = if fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink()) {
        resolved = fs::canonicalize(path)?;
        resolved.as_path()
    } else {
        path
    };
    let dir = path
        .parent()
        .ok_or_else(|| io::Error::other("no parent folder"))?;
    fs::create_dir_all(dir)?;
    let name = path
        .file_name()
        .map_or_else(|| "file".into(), |n| n.to_string_lossy().into_owned());
    let tmp = dir.join(format!(
        ".{name}.patinote-{}-{}.tmp",
        std::process::id(),
        SEQ.fetch_add(1, Ordering::Relaxed)
    ));
    let result = (|| {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        if let Some(mode) = mode {
            f.set_permissions(fs::Permissions::from_mode(mode))?;
        }
        f.sync_all()?;
        fs::rename(&tmp, path)?;
        sync_dir(dir)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

fn mode_of(path: &Path) -> Option<u32> {
    fs::metadata(path).ok().map(|m| m.permissions().mode())
}

fn rename_into_place(root: &Path, from: &str, to: &str) -> io::Result<()> {
    let dest = root.join(to);
    if let Some(dir) = dest.parent() {
        fs::create_dir_all(dir)?;
    }
    fs::rename(root.join(from), &dest)?;
    if let Some(dir) = dest.parent() {
        sync_dir(dir)?;
    }
    Ok(())
}

/// Folders missing above `rel` that applying the change will create.
fn missing_dirs(root: &Path, rel: &str, out: &mut Vec<String>) {
    let mut dir = rel.rsplit_once('/').map(|(d, _)| d);
    while let Some(d) = dir {
        if root.join(d).exists() || out.iter().any(|x| x == d) {
            break;
        }
        out.push(d.to_owned());
        dir = d.rsplit_once('/').map(|(p, _)| p);
    }
}

/// What a successful commit did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Committed {
    /// The journal entry.
    pub id: String,
    /// Vault paths that changed, including move and trash destinations.
    pub changed: Vec<String>,
}

/// Holds the journal's lock file: one Patinote process writes to a vault at
/// a time (the app, the CLI, an agent). Released when dropped.
struct WriteLock(fs::File);

impl WriteLock {
    fn acquire(journal: &Journal) -> io::Result<Self> {
        fs::create_dir_all(&journal.dir)?;
        let f = fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(journal.dir.join(".lock"))?;
        f.lock()?;
        Ok(Self(f))
    }
}

impl Drop for WriteLock {
    fn drop(&mut self) {
        let _ = self.0.unlock();
    }
}

/// Applies a transaction completely or not at all (see the module docs).
pub fn commit(root: &Path, journal: &Journal, tx: &Transaction) -> Result<Committed, WriteError> {
    let _lock = WriteLock::acquire(journal).map_err(io_err(&journal.dir))?;
    let plan = plan(root, tx)?;
    let conflicts: Vec<String> = plan.conflicts().into_iter().map(str::to_owned).collect();
    if !conflicts.is_empty() {
        return Err(WriteError::Conflict { paths: conflicts });
    }

    // Write-ahead: the journal entry with every old content, before the vault.
    let now = SystemTime::now();
    let time = now.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let id = format!(
        "{:020}-{:06}-{:04}",
        now.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_nanos()),
        std::process::id(),
        SEQ.fetch_add(1, Ordering::Relaxed) % 10_000
    );
    let entry_dir = journal.dir.join(&id);
    fs::create_dir_all(&entry_dir).map_err(io_err(&entry_dir))?;
    let mut blob_no = 0;
    let mut save_blob = |path: &Path| -> Result<String, WriteError> {
        let bytes = fs::read(path).map_err(io_err(path))?;
        let name = format!("before-{blob_no}");
        blob_no += 1;
        atomic_write(&entry_dir.join(&name), &bytes, None).map_err(io_err(&entry_dir))?;
        Ok(name)
    };
    let mut ops = Vec::new();
    let mut created_dirs = Vec::new();
    for (op, planned) in tx.ops.iter().zip(&plan.ops) {
        ops.push(match op {
            Op::Write {
                path,
                content,
                expect,
            } => {
                missing_dirs(root, path, &mut created_dirs);
                let before = match expect {
                    Expect::Absent => None,
                    _ => Some(save_blob(&root.join(path))?),
                };
                EntryOp::Write {
                    path: path.clone(),
                    before,
                    after: hex(blake3::hash(content).as_bytes()),
                }
            }
            Op::Move {
                from, to, content, ..
            } => {
                missing_dirs(root, to, &mut created_dirs);
                let before = match content {
                    Some(_) => Some(save_blob(&root.join(from))?),
                    None => None,
                };
                let after = content.as_ref().map(|c| hex(blake3::hash(c).as_bytes()));
                EntryOp::Move {
                    from: from.clone(),
                    to: to.clone(),
                    before,
                    after,
                }
            }
            Op::Trash { path, .. } => {
                let to = planned.to.clone().unwrap_or_default();
                missing_dirs(root, &to, &mut created_dirs);
                EntryOp::Trash {
                    path: path.clone(),
                    to,
                }
            }
        });
    }
    let mut entry = Entry {
        v: 1,
        id: id.clone(),
        time,
        actor: tx.actor.as_label(),
        status: EntryStatus::Pending,
        ops,
        created_dirs,
    };
    journal.write_manifest(&entry).map_err(io_err(&entry_dir))?;
    sync_dir(&entry_dir).map_err(io_err(&entry_dir))?;

    // Apply.
    let mut changed = Vec::new();
    for (i, op) in tx.ops.iter().enumerate() {
        let result = match op {
            Op::Write { path, content, .. } => {
                let full = root.join(path);
                changed.push(path.clone());
                atomic_write(&full, content, mode_of(&full))
            }
            Op::Move {
                from, to, content, ..
            } => {
                changed.extend([from.clone(), to.clone()]);
                match content {
                    Some(c) => {
                        let src = root.join(from);
                        atomic_write(&root.join(to), c, mode_of(&src))
                            .and_then(|()| fs::remove_file(&src))
                    }
                    None => rename_into_place(root, from, to),
                }
            }
            Op::Trash { path, .. } => match &entry.ops[i] {
                EntryOp::Trash { to, .. } => {
                    changed.extend([path.clone(), to.clone()]);
                    rename_into_place(root, path, to)
                }
                // Entry ops are built from the same transaction in the same
                // order, so this cannot happen; fail safely if it ever does.
                _ => Err(io::Error::other(
                    "journal entry does not match the transaction",
                )),
            },
        };
        if let Err(source) = result {
            let path = op.paths()[0].to_owned();
            return match undo(root, journal, &entry) {
                Ok(()) => {
                    entry.status = EntryStatus::RolledBack;
                    let _ = journal.write_manifest(&entry);
                    Err(WriteError::RolledBack { path, source })
                }
                Err(_) => Err(WriteError::RollbackFailed { path, source }),
            };
        }
    }

    entry.status = EntryStatus::Committed;
    journal.write_manifest(&entry).map_err(io_err(&entry_dir))?;
    // Retention problems never fail a write that already happened.
    let _ = journal.prune(now);
    Ok(Committed { id, changed })
}

/// Undoes whatever of `entry` is applied, in reverse order. It checks the
/// state of each path, so it is safe to run on a partly applied entry and
/// to run twice.
fn undo(root: &Path, journal: &Journal, entry: &Entry) -> io::Result<()> {
    let exists = |rel: &str| fs::symlink_metadata(root.join(rel)).is_ok();
    let hash_is = |rel: &str, h: &str| {
        hash_file(&root.join(rel))
            .map(|x| hex(&x) == h)
            .unwrap_or(false)
    };
    for op in entry.ops.iter().rev() {
        match op {
            EntryOp::Write {
                path,
                before,
                after,
            } => {
                if !hash_is(path, after) {
                    continue; // Not applied.
                }
                match before {
                    Some(blob) => {
                        let full = root.join(path);
                        atomic_write(&full, &journal.blob(&entry.id, blob)?, mode_of(&full))?;
                    }
                    None => fs::remove_file(root.join(path))?,
                }
            }
            EntryOp::Move {
                from,
                to,
                before,
                after,
            } => match (before, after) {
                // A move that rewrote the file: the new file is written first,
                // then the old one removed; a crash can leave both.
                (Some(blob), Some(after)) => {
                    if hash_is(to, after) {
                        if !exists(from) {
                            let mode = mode_of(&root.join(to));
                            atomic_write(&root.join(from), &journal.blob(&entry.id, blob)?, mode)?;
                        }
                        fs::remove_file(root.join(to))?;
                    }
                }
                _ => {
                    if !exists(from) && exists(to) {
                        rename_into_place(root, to, from)?;
                    }
                }
            },
            EntryOp::Trash { path, to } => {
                if !exists(path) && exists(to) {
                    rename_into_place(root, to, path)?;
                }
            }
        }
    }
    // Remove folders the transaction created, deepest first, if still empty.
    let mut dirs = entry.created_dirs.clone();
    dirs.sort_by_key(|d| std::cmp::Reverse(d.matches('/').count()));
    for d in dirs {
        let _ = fs::remove_dir(root.join(d));
    }
    Ok(())
}

/// Undoes journal entries left pending by a crash. Run it when a vault is
/// opened for writing. Returns the ids it rolled back. An entry that cannot
/// be read or undone does not stop the others; the error names it. A
/// manifest that is read but cannot be parsed is renamed
/// `manifest.json.damaged`, so it is reported once rather than on every
/// start; one that cannot be read is tried again next time.
pub fn recover(root: &Path, journal: &Journal) -> io::Result<Vec<String>> {
    let _lock = WriteLock::acquire(journal)?;
    let mut done = Vec::new();
    let mut failed = Vec::new();
    // Each entry on its own: one damaged entry must not leave the ones
    // after it half-applied.
    for id in journal.ids()? {
        let undone = (|| {
            let manifest = journal.dir().join(&id).join("manifest.json");
            // Not read (permissions, an I/O error): tried again next time.
            let text = fs::read_to_string(&manifest)?;
            let mut entry: Entry = match serde_json::from_str(&text) {
                Ok(entry) => entry,
                Err(e) => {
                    // Read but damaged: nothing can ever be undone from it.
                    // It is set aside (blobs kept, for a person to look at)
                    // and reported this once.
                    let _ = fs::rename(&manifest, manifest.with_extension("json.damaged"));
                    return Err(io::Error::other(format!("damaged manifest: {e}")));
                }
            };
            if entry.status != EntryStatus::Pending {
                return Ok(false);
            }
            undo(root, journal, &entry)?;
            entry.status = EntryStatus::RolledBack;
            journal.write_manifest(&entry)?;
            Ok::<_, io::Error>(true)
        })();
        match undone {
            Ok(true) => done.push(id),
            Ok(false) => {}
            Err(e) => failed.push(format!("{id}: {e}")),
        }
    }
    if failed.is_empty() {
        Ok(done)
    } else {
        Err(io::Error::other(format!(
            "cannot recover journal entries {}",
            failed.join("; ")
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_rules() {
        for ok in [
            "a.md",
            "Proyek/Rapat 19 Nov.md",
            ".patinote/config.toml",
            "Café.md",
        ] {
            assert!(check_path(ok).is_ok(), "{ok}");
        }
        for bad in [
            "",
            "/etc/passwd",
            "../outside.md",
            "a/../../b.md",
            "a//b.md",
            ".obsidian/app.json",
            ".git/HEAD",
            ".trash/x.md",
            "Proyek/.draft/x.md",
            ".patinote",
            "a\nb.md",
        ] {
            assert!(check_path(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn timestamps() {
        assert_eq!(timestamp(UNIX_EPOCH), "19700101-000000");
        let t = UNIX_EPOCH + std::time::Duration::from_secs(1_795_102_530);
        assert_eq!(timestamp(t), "20261119-153530");
    }

    #[test]
    fn actors() {
        assert_eq!(Actor::Agent("claude".into()).as_label(), "agent:claude");
        assert_eq!(Actor::User.as_label(), "user");
    }
}
