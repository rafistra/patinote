//! Running a command: vault selection, index refresh, output and exit codes.

use std::ffi::OsString;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use clap::{CommandFactory, Parser};
use patinote_core::commands::{
    CacheCleanOutput, ErrorBody, ErrorCode, IndexOutput, OutputWarning, VaultInfoOutput, exit,
};
use patinote_core::config::{expand_home, global_config_path, load_global};
use patinote_core::index::IndexError as CoreIndexError;
use patinote_core::write::{Journal, recover};
use patinote_core::{IndexOptions, IndexReport, Progress, ScanError, location};
use patinote_index::{
    IndexError, OpenOptions, RebuildReason, SqliteStore, cache_clean, index_path_for, update_index,
};

use crate::cli::{CacheCommand, Cli, Command, Global, VaultCommand};
use crate::output::{self, Failure};

/// Set by the Ctrl+C handler; indexing stops at the next batch (A-03.5).
static INTERRUPTED: AtomicBool = AtomicBool::new(false);

/// Runs the program and returns its exit code.
pub fn main() -> i32 {
    // Without a handler, Ctrl+C would kill the process before it could exit
    // with code 130 (Q-008).
    let _ = ctrlc::set_handler(|| INTERRUPTED.store(true, Ordering::SeqCst));
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_max_level(tracing::Level::WARN)
        .without_time()
        .init();

    let args: Vec<OsString> = std::env::args_os().collect();
    let wants_json = args.iter().any(|a| a == "--json");
    let cli = match Cli::try_parse_from(&args) {
        Ok(cli) => cli,
        Err(e) => {
            use clap::error::ErrorKind;
            if matches!(e.kind(), ErrorKind::DisplayHelp | ErrorKind::DisplayVersion) {
                let _ = output::stdout(&e.render().to_string());
                return exit::OK;
            }
            if wants_json {
                return output::error(
                    ErrorBody {
                        code: ErrorCode::InvalidArgument,
                        message: first_line(&e.to_string()),
                        candidates: None,
                    },
                    true,
                );
            }
            let _ = e.print();
            return exit::INVALID_ARGUMENT;
        }
    };

    let code = match run(&cli) {
        Ok(warnings) if warnings > 0 && cli.global.strict => exit::ERROR,
        Ok(_) => exit::OK,
        Err(Failure::Error(body)) => output::error(body, cli.global.json),
        Err(Failure::Interrupted) => exit::INTERRUPTED,
        Err(Failure::BrokenPipe) => exit::OK,
    };
    if INTERRUPTED.load(Ordering::SeqCst) {
        exit::INTERRUPTED
    } else {
        code
    }
}

fn first_line(s: &str) -> String {
    s.lines()
        .next()
        .unwrap_or(s)
        .trim_start_matches("error: ")
        .to_owned()
}

/// Runs the command; returns the number of warnings it reported.
fn run(cli: &Cli) -> Result<usize, Failure> {
    let g = &cli.global;
    match &cli.command {
        Command::Completions { shell } => {
            let mut buf = Vec::new();
            clap_complete::generate(*shell, &mut Cli::command(), "patinote", &mut buf);
            output::stdout(&String::from_utf8_lossy(&buf))?;
            Ok(0)
        }
        Command::Cache(CacheCommand::Clean) => cache_clean_cmd(g),
        Command::Index { rebuild } => index_cmd(g, *rebuild),
        Command::Vault(VaultCommand::Info) => vault_info_cmd(g),
    }
}

fn io_failure(context: &str, e: impl std::fmt::Display) -> Failure {
    Failure::new(ErrorCode::IoError, format!("{context}: {e}"))
}

// ---------------------------------------------------------------------------
// Vault selection (F-01) and index refresh (F-03)

/// The selected vault and where its data lives.
struct Vault {
    root: PathBuf,
    id: String,
    index_path: PathBuf,
    journal: Journal,
    warnings: Vec<OutputWarning>,
}

fn warning(
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

/// `--vault`, then `PATINOTE_VAULT`, then `[vault] default` in the global
/// configuration (F-01.9).
fn select_vault(g: &Global) -> Result<Vault, Failure> {
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

    let chosen: Option<PathBuf> = g
        .vault
        .as_ref()
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

fn open_store(vault: &Vault, rebuild: bool) -> Result<SqliteStore, Failure> {
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

fn index_failure(e: IndexError) -> Failure {
    match e {
        IndexError::Locked => Failure::new(
            ErrorCode::IndexLocked,
            "the index is locked by another Patinote process; waited 5 seconds",
        ),
        other => io_failure("index error", other),
    }
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

/// Undoes interrupted writes, then brings the index up to date.
fn refresh(
    vault: &mut Vault,
    store: &mut SqliteStore,
    quiet: bool,
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
        cancel: Some(&INTERRUPTED),
        ..IndexOptions::default()
    };
    let mut progress = progress_printer(quiet);
    match update_index(store, &vault.root, &options, &mut progress) {
        Ok(report) => Ok(report),
        Err(CoreIndexError::Cancelled) => Err(Failure::Interrupted),
        Err(CoreIndexError::Store(e)) => Err(index_failure(e)),
        Err(CoreIndexError::Scan(ScanError::NotFound(p))) => Err(Failure::new(
            ErrorCode::VaultNotFound,
            format!("vault folder not found: {}", p.display()),
        )),
        Err(CoreIndexError::Scan(e)) => Err(io_failure("cannot scan the vault", e)),
    }
}

fn warning_message(code: &str) -> &'static str {
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

/// Every warning to report: configuration, scan problems, stored per-file
/// warnings, and an automatic rebuild.
fn collect_warnings(
    vault: &Vault,
    store: &SqliteStore,
    report: Option<&IndexReport>,
) -> Result<Vec<OutputWarning>, Failure> {
    let mut out = vault.warnings.clone();
    if store
        .rebuilt()
        .is_some_and(|r| r != RebuildReason::NewerSchema)
    {
        out.push(warning(
            "index_rebuilt",
            None,
            None,
            "the index was damaged or outdated and has been rebuilt",
        ));
    }
    if let Some(r) = report {
        for issue in &r.issues {
            let code = format!("{:?}", issue.kind);
            let code = snake(&code);
            out.push(warning(
                &code,
                Some(&issue.path),
                None,
                warning_message(&code),
            ));
        }
    }
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

fn rebuilt_label(store: &SqliteStore, requested: bool) -> Option<String> {
    if requested {
        return Some("rebuild_requested".into());
    }
    store.rebuilt().map(|r| match r {
        RebuildReason::Corrupt => "corrupt".into(),
        RebuildReason::NewerSchema => "newer_schema".into(),
        RebuildReason::MigrationFailed => "migration_failed".into(),
    })
}

// ---------------------------------------------------------------------------
// Commands

fn index_cmd(g: &Global, rebuild: bool) -> Result<usize, Failure> {
    let mut vault = select_vault(g)?;
    let mut store = open_store(&vault, rebuild)?;
    let started = Instant::now();
    let report = refresh(&mut vault, &mut store, g.quiet)?;
    let warnings = collect_warnings(&vault, &store, Some(&report))?;
    let out = IndexOutput {
        v: 1,
        notes: report.notes,
        attachments: report.attachments,
        canvases: report.canvases,
        excluded: report.excluded,
        sync_conflicts: report.sync_conflicts,
        reprocessed: report.reprocessed(),
        added: report.added,
        updated: report.updated,
        touched: report.touched,
        removed: report.removed,
        unchanged: report.unchanged,
        links_updated: report.links_updated,
        rebuilt: rebuilt_label(&store, rebuild),
        duration_ms: started.elapsed().as_millis() as u64,
        warnings,
    };
    if g.json {
        output::json(&out)?;
    } else {
        output::stdout(&format!(
            "{}, {}: {} added, {} updated, {} removed in {:.2} s\n",
            plural(out.notes, "note"),
            plural(out.attachments, "attachment"),
            out.added,
            out.updated,
            out.removed,
            out.duration_ms as f64 / 1000.0
        ))?;
        output::warnings_text(&out.warnings, g.quiet);
    }
    Ok(out.warnings.len())
}

fn vault_info_cmd(g: &Global) -> Result<usize, Failure> {
    let mut vault = select_vault(g)?;
    let mut store = open_store(&vault, false)?;
    let report = if g.no_refresh {
        None
    } else {
        Some(refresh(&mut vault, &mut store, g.quiet)?)
    };
    let counts = store.counts().map_err(index_failure)?;
    let sync_conflicts = match &report {
        Some(r) => r.sync_conflicts,
        None => {
            patinote_core::scan(&vault.root, &Default::default()).map_or(0, |s| s.conflicts.len())
        }
    };
    let out = VaultInfoOutput {
        v: 1,
        vault: vault.root.to_string_lossy().into_owned(),
        vault_id: vault.id.clone(),
        index: vault.index_path.to_string_lossy().into_owned(),
        notes: counts.notes,
        attachments: counts.attachments,
        canvases: counts.canvases,
        excluded: counts.excluded,
        links: counts.links,
        unresolved_links: counts.unresolved_links,
        sync_conflicts,
        warnings: collect_warnings(&vault, &store, report.as_ref())?,
    };
    if g.json {
        output::json(&out)?;
    } else {
        let rows = [
            ("vault", out.vault.clone()),
            ("vault_id", out.vault_id.clone()),
            ("index", out.index.clone()),
            ("notes", out.notes.to_string()),
            ("attachments", out.attachments.to_string()),
            ("canvases", out.canvases.to_string()),
            ("excluded", out.excluded.to_string()),
            ("links", out.links.to_string()),
            ("unresolved_links", out.unresolved_links.to_string()),
            ("sync_conflicts", out.sync_conflicts.to_string()),
        ];
        let text: String = rows.iter().map(|(k, v)| format!("{k}\t{v}\n")).collect();
        output::stdout(&text)?;
        output::warnings_text(&out.warnings, g.quiet);
    }
    Ok(out.warnings.len())
}

fn cache_clean_cmd(g: &Global) -> Result<usize, Failure> {
    let cache = location::cache_home()
        .ok_or_else(|| io_failure("cannot find a cache folder", "HOME is not set"))?;
    let report = cache_clean(&cache).map_err(index_failure)?;
    let out = CacheCleanOutput {
        v: 1,
        removed: report.removed,
        kept: report.kept,
        unknown: report.unknown,
        warnings: Vec::new(),
    };
    if g.json {
        output::json(&out)?;
    } else {
        let text: String = out
            .removed
            .iter()
            .map(|id| format!("removed\t{id}\n"))
            .collect();
        output::stdout(&text)?;
        if !g.quiet {
            eprintln!(
                "removed {} index(es), kept {}",
                out.removed.len(),
                out.kept + out.unknown.len()
            );
        }
    }
    Ok(0)
}

/// `1 note`, `2 notes`.
fn plural(n: usize, word: &str) -> String {
    if n == 1 {
        format!("1 {word}")
    } else {
        format!("{n} {word}s")
    }
}
