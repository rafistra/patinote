//! The vault a command works on: selection (F-01.9), its index, refresh
//! before the command (F-03), and the warnings to report.

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use patinote_core::commands::{ErrorCode, OutputWarning};
use patinote_core::config::{expand_home, global_config_path, load_global};
use patinote_core::index::IndexError as CoreIndexError;
use patinote_core::write::{Journal, recover};
use patinote_core::{IndexOptions, IndexReport, Progress, ScanError, location};
use patinote_index::{
    IndexError, OpenOptions, RebuildReason, SqliteStore, index_path_for, update_index,
};

use crate::output::Failure;

/// The selected vault and where its data lives.
pub struct Vault {
    /// Canonical vault path.
    pub root: PathBuf,
    pub id: String,
    pub index_path: PathBuf,
    pub journal: Journal,
    /// Warnings from configuration, journal recovery and the refresh.
    pub warnings: Vec<OutputWarning>,
}

/// A warning for the JSON contract.
pub fn warning(
    code: &str,
    path: Option<&str>,
    line: Option<usize>,
    message: impl Into<String>,
) -> OutputWarning {
    OutputWarning {
        code: code.into(),
        path: path.map(str::to_owned),
        line,
        message: message.into(),
    }
}

/// An `io_error` failure with context.
pub fn io_failure(context: &str, e: impl std::fmt::Display) -> Failure {
    Failure::new(ErrorCode::IoError, format!("{context}: {e}"))
}

/// Maps index errors to the contract (A-03).
pub fn index_failure(e: IndexError) -> Failure {
    match e {
        IndexError::Locked => Failure::new(
            ErrorCode::IndexLocked,
            "the index is locked by another Patinote process; waited 5 seconds",
        ),
        other => io_failure("index error", other),
    }
}

/// `--vault`, then `PATINOTE_VAULT`, then `[vault] default` (F-01.9).
pub fn select_vault(flag: Option<&PathBuf>) -> Result<Vault, Failure> {
    let mut warnings = Vec::new();
    let config = match location::config_home().map(|h| load_global(&global_config_path(&h))) {
        Some(Ok(c)) => c,
        Some(Err(e)) => {
            warnings.push(warning("config", None, None, e.to_string()));
            Default::default()
        }
        None => Default::default(),
    };
    warnings.extend(
        config
            .warnings
            .iter()
            .map(|w| warning("config", None, None, w.clone())),
    );

    let chosen: Option<PathBuf> = flag
        .map(|p| expand_home(&p.to_string_lossy()))
        .or_else(|| {
            std::env::var("PATINOTE_VAULT")
                .ok()
                .filter(|v| !v.is_empty())
                .map(|v| expand_home(&v))
        })
        .or(config.vault_default);
    let Some(path) = chosen else {
        return Err(Failure::new(
            ErrorCode::InvalidArgument,
            "no vault given: use --vault, PATINOTE_VAULT, or `[vault] default` in ~/.config/patinote/config.toml",
        ));
    };
    match std::fs::metadata(&path) {
        Ok(m) if m.is_dir() => {}
        Ok(_) => {
            return Err(Failure::new(
                ErrorCode::InvalidArgument,
                format!("the vault path is a file, not a folder: {}", path.display()),
            ));
        }
        Err(_) => {
            return Err(Failure::new(
                ErrorCode::VaultNotFound,
                format!("vault folder not found: {}", path.display()),
            ));
        }
    }
    let root = std::fs::canonicalize(&path).map_err(|e| io_failure("cannot open the vault", e))?;
    let id = location::vault_id_of_canonical(&root);
    let cache = location::cache_home()
        .ok_or_else(|| io_failure("cannot find a cache folder", "HOME is not set"))?;
    let state = location::state_home()
        .ok_or_else(|| io_failure("cannot find a state folder", "HOME is not set"))?;
    let index_path =
        index_path_for(&root, &cache).map_err(|e| io_failure("cannot locate the index", e))?;
    Ok(Vault {
        root,
        journal: Journal::for_vault(&state, &id),
        id,
        index_path,
        warnings,
    })
}

/// Opens the vault's index, deleting it first for `--rebuild`.
pub fn open_store(vault: &Vault, rebuild: bool) -> Result<SqliteStore, Failure> {
    if rebuild {
        for suffix in ["", "-wal", "-shm"] {
            let mut p = vault.index_path.clone().into_os_string();
            p.push(suffix);
            match std::fs::remove_file(PathBuf::from(p)) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(io_failure("cannot delete the index", e)),
            }
        }
    }
    SqliteStore::open(&vault.index_path, &OpenOptions::default()).map_err(index_failure)
}

/// Prints progress on stderr once indexing has taken longer than a second
/// (F-03). `PATINOTE_PROGRESS_AFTER_MS` changes the delay, for tests.
fn progress_printer(quiet: bool) -> impl FnMut(Progress) {
    let started = Instant::now();
    let after = std::env::var("PATINOTE_PROGRESS_AFTER_MS")
        .ok()
        .and_then(|v| v.parse().ok())
        .map_or(Duration::from_secs(1), Duration::from_millis);
    move |p| {
        if quiet || started.elapsed() < after {
            return;
        }
        match p {
            Progress::Scanned { files, changed } => {
                eprintln!("indexing: {files} files, {changed} to read")
            }
            Progress::Stored { done, changed } => eprintln!("indexing: {done}/{changed}"),
        }
    }
}

/// Undoes interrupted writes, then brings the index up to date. Scan
/// problems and an automatic rebuild become warnings.
pub fn refresh(
    vault: &mut Vault,
    store: &mut SqliteStore,
    quiet: bool,
    cancel: &'static AtomicBool,
) -> Result<IndexReport, Failure> {
    match recover(&vault.root, &vault.journal) {
        Ok(ids) if !ids.is_empty() => vault.warnings.push(warning(
            "recovered",
            None,
            None,
            format!("undid {} interrupted write(s) from the journal", ids.len()),
        )),
        Ok(_) => {}
        Err(e) => vault.warnings.push(warning(
            "journal",
            None,
            None,
            format!("cannot check the journal: {e}"),
        )),
    }
    let options = IndexOptions {
        cancel: Some(cancel),
        ..IndexOptions::default()
    };
    let mut progress = progress_printer(quiet);
    let report = match update_index(store, &vault.root, &options, &mut progress) {
        Ok(report) => report,
        Err(CoreIndexError::Cancelled) => return Err(Failure::Interrupted),
        Err(CoreIndexError::Store(e)) => return Err(index_failure(e)),
        Err(CoreIndexError::Scan(ScanError::NotFound(p))) => {
            return Err(Failure::new(
                ErrorCode::VaultNotFound,
                format!("vault folder not found: {}", p.display()),
            ));
        }
        Err(CoreIndexError::Scan(e)) => return Err(io_failure("cannot scan the vault", e)),
    };
    for issue in &report.issues {
        let code = snake(&format!("{:?}", issue.kind));
        vault.warnings.push(warning(
            &code,
            Some(&issue.path),
            None,
            warning_message(&code),
        ));
    }
    if store
        .rebuilt()
        .is_some_and(|r| r != RebuildReason::NewerSchema)
    {
        vault.warnings.push(warning(
            "index_rebuilt",
            None,
            None,
            "the index was damaged or outdated and has been rebuilt",
        ));
    }
    Ok(report)
}

/// The vault's warnings, plus the per-file warnings stored in the index
/// (for `index` and `vault info`).
pub fn with_stored_warnings(
    vault: &Vault,
    store: &SqliteStore,
) -> Result<Vec<OutputWarning>, Failure> {
    let mut out = vault.warnings.clone();
    for w in store.warnings().map_err(index_failure)? {
        out.push(warning(
            &w.code,
            Some(&w.path),
            w.line,
            warning_message(&w.code),
        ));
    }
    Ok(out)
}

/// A readable message for a warning code.
pub fn warning_message(code: &str) -> &'static str {
    match code {
        "not_utf8" => "not valid UTF-8; listed but not parsed",
        "too_large" => "larger than 5 MB; listed but not parsed or searched",
        "unreadable" => "cannot be read",
        "frontmatter_invalid" => "the frontmatter is not valid YAML; its properties are ignored",
        "symlink_loop" => "a symbolic link loops back to a parent folder",
        "invalid_name" => "the file name is not valid UTF-8",
        "invalid_ignore_file" => ".patinote/ignore has a line that cannot be read",
        _ => "warning",
    }
}

fn snake(camel: &str) -> String {
    let mut s = String::new();
    for (i, c) in camel.chars().enumerate() {
        if c.is_uppercase() && i > 0 {
            s.push('_');
        }
        s.extend(c.to_lowercase());
    }
    s
}

/// Why the index was rebuilt, for `index --json`.
pub fn rebuilt_label(store: &SqliteStore, requested: bool) -> Option<String> {
    if requested {
        return Some("rebuild_requested".into());
    }
    store.rebuilt().map(|r| match r {
        RebuildReason::Corrupt => "corrupt".into(),
        RebuildReason::NewerSchema => "newer_schema".into(),
        RebuildReason::MigrationFailed => "migration_failed".into(),
    })
}
