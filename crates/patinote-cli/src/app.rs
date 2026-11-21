//! Running a command: parse arguments, dispatch, print, and map the outcome
//! to an exit code (A-03).

use std::ffi::OsString;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use clap::{CommandFactory, Parser};
use patinote_core::commands::{
    AgentsInitOutput, BacklinksOutput, CacheCleanOutput, ConflictsOutput, ErrorBody, ErrorCode,
    IndexOutput, LinksOutput, ListOutput, OpenOutput, OrphansOutput, OutputWarning, ReadOutput,
    ResolveOutput, SearchOutput, TagsOutput, UnresolvedOutput, VaultInfoOutput, exit,
};
use patinote_core::location;
use patinote_index::cache_clean;
use serde::Serialize;

use crate::cli::{AgentsCommand, CacheCommand, Cli, Command, Global, VaultCommand};
use crate::commands::{self, Ctx, ListArgs};
use crate::context::{
    self, index_failure, io_failure, open_store, rebuilt_label, refresh, select_vault,
};
use crate::output::{self, Failure};

/// Set by the Ctrl+C handler; indexing stops at the next batch (A-03.5).
pub static INTERRUPTED: AtomicBool = AtomicBool::new(false);

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

/// Output that can be printed as JSON or as tab-separated text.
trait Printable: Serialize {
    fn warnings_mut(&mut self) -> &mut Vec<OutputWarning>;
    fn warnings(&self) -> &[OutputWarning];
    fn text(&self) -> String;
}

/// Prints an output and returns its number of warnings.
fn print<T: Printable>(g: &Global, out: &T) -> Result<usize, Failure> {
    if g.json {
        output::json(out)?;
    } else {
        output::stdout(&out.text())?;
        output::warnings_text(out.warnings(), g.quiet);
    }
    Ok(out.warnings().len())
}

/// Selects the vault, opens and refreshes its index, runs `f`, and prints
/// its output with the warnings from this run.
fn with_vault<T: Printable>(
    g: &Global,
    f: impl FnOnce(&Ctx<'_>) -> Result<T, Failure>,
) -> Result<usize, Failure> {
    let mut vault = select_vault(g.vault.as_ref())?;
    let mut store = open_store(&vault, false)?;
    if !g.no_refresh {
        refresh(&mut vault, &mut store, g.quiet, &INTERRUPTED)?;
    }
    let mut out = f(&Ctx {
        vault: &vault,
        store: &store,
    })?;
    out.warnings_mut().extend(vault.warnings.iter().cloned());
    print(g, &out)
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
        Command::Vault(VaultCommand::Info) => with_vault(g, commands::vault_info),
        Command::List {
            tag,
            folder,
            kind,
            sort,
            limit,
        } => with_vault(g, |ctx| {
            let args = ListArgs {
                tag: tag.as_deref(),
                folder: folder.as_deref(),
                kind: *kind,
                sort: *sort,
                limit: *limit,
            };
            commands::list(ctx, &args)
        }),
        Command::Read { note, lines } => {
            let range = match lines {
                Some(spec) => commands::parse_lines(spec)?,
                None => (None, None),
            };
            with_vault(g, |ctx| {
                commands::read(ctx, &commands::resolve_note(ctx, note)?, range, None)
            })
        }
        Command::Search { query, limit } => {
            with_vault(g, |ctx| commands::search(ctx, query, *limit))
        }
        Command::Backlinks { note } => with_vault(g, |ctx| {
            commands::backlinks(ctx, &commands::resolve_note(ctx, note)?)
        }),
        Command::Links { note } => with_vault(g, |ctx| {
            commands::links(ctx, &commands::resolve_note(ctx, note)?)
        }),
        Command::Tags { prefix } => with_vault(g, |ctx| commands::tags(ctx, prefix.as_deref())),
        Command::Orphans => with_vault(g, commands::orphans),
        Command::Unresolved => with_vault(g, commands::unresolved),
        Command::Resolve { name } => with_vault(g, |ctx| commands::resolve(ctx, name)),
        Command::Conflicts => with_vault(g, commands::conflicts),
        Command::Open { note } => with_vault(g, |ctx| crate::open::open(ctx, note)),
        Command::Agents(AgentsCommand::Init { update, agents }) => {
            with_vault(g, |ctx| crate::agents::init(ctx, *update, agents))
        }
        Command::Mcp => {
            let vault = select_vault(g.vault.as_ref())?;
            let store = open_store(&vault, false)?;
            crate::mcp::serve(vault, store)?;
            Ok(0)
        }
    }
}

// ---------------------------------------------------------------------------
// Text output: one result per line, path first, tab-separated (A-01).

fn lines<I: IntoIterator<Item = String>>(rows: I) -> String {
    rows.into_iter().map(|r| r + "\n").collect()
}

fn clean(s: &str) -> String {
    s.replace(['\t', '\n', '\r'], " ")
}

macro_rules! printable {
    ($t:ty, |$o:ident| $body:expr) => {
        impl Printable for $t {
            fn warnings_mut(&mut self) -> &mut Vec<OutputWarning> {
                &mut self.warnings
            }
            fn warnings(&self) -> &[OutputWarning] {
                &self.warnings
            }
            fn text(&self) -> String {
                let $o = self;
                $body
            }
        }
    };
}

printable!(ListOutput, |o| lines(o.notes.iter().map(|n| format!(
    "{}\t{}",
    n.path,
    clean(&n.title)
))));
printable!(ReadOutput, |o| if o.content.is_empty() {
    String::new()
} else {
    format!("{}\n", o.content)
});
printable!(SearchOutput, |o| lines(o.results.iter().map(|r| format!(
    "{}\t{}\t{}",
    r.path,
    r.line,
    clean(&r.snippet)
))));
printable!(BacklinksOutput, |o| lines(
    o.backlinks
        .iter()
        .map(|b| format!("{}\t{}\t{}", b.path, b.line, clean(&b.context)))
));
printable!(LinksOutput, |o| lines(o.links.iter().map(|l| format!(
    "{}\t{}\t{}\t{}",
    l.path.as_deref().unwrap_or(&l.target),
    l.line,
    clean(&l.raw),
    if l.path.is_some() {
        "resolved"
    } else {
        "unresolved"
    }
))));
printable!(TagsOutput, |o| lines(
    o.tags
        .iter()
        .map(|t| format!("{}\t{}\t{}", t.tag, t.uses, t.notes))
));
printable!(OrphansOutput, |o| lines(o.notes.iter().map(|n| format!(
    "{}\t{}",
    n.path,
    clean(&n.title)
))));
printable!(UnresolvedOutput, |o| lines(o.targets.iter().map(|t| {
    format!(
        "{}\t{}\t{}",
        clean(&t.target),
        t.count,
        t.sources
            .iter()
            .map(|s| format!("{}:{}", s.path, s.line))
            .collect::<Vec<_>>()
            .join(", ")
    )
})));
printable!(OpenOutput, |o| format!(
    "{}\t{}\n",
    o.note.path,
    if o.started { "started" } else { "opened" }
));
printable!(ResolveOutput, |o| match &o.path {
    Some(p) => lines(
        std::iter::once(p.clone()).chain(
            o.candidates
                .iter()
                .skip(1)
                .map(|c| format!("{c}\talso matches"))
        )
    ),
    None => {
        eprintln!("no note matches '{}'", o.input);
        String::new()
    }
});
printable!(VaultInfoOutput, |o| lines(
    [
        ("vault", o.vault.clone()),
        ("vault_id", o.vault_id.clone()),
        ("index", o.index.clone()),
        ("notes", o.notes.to_string()),
        ("attachments", o.attachments.to_string()),
        ("canvases", o.canvases.to_string()),
        ("excluded", o.excluded.to_string()),
        ("links", o.links.to_string()),
        ("unresolved_links", o.unresolved_links.to_string()),
        ("sync_conflicts", o.sync_conflicts.to_string()),
    ]
    .into_iter()
    .map(|(k, v)| format!("{k}\t{v}"))
));
printable!(AgentsInitOutput, |o| lines(
    o.written
        .iter()
        .map(|p| format!("{p}\twritten"))
        .chain(o.unchanged.iter().map(|p| format!("{p}\tunchanged")))
));
printable!(ConflictsOutput, |o| lines(
    o.conflicts
        .iter()
        .map(|c| format!("{}\t{}", c.path, c.original))
));

// ---------------------------------------------------------------------------
// Index commands

fn index_cmd(g: &Global, rebuild: bool) -> Result<usize, Failure> {
    let mut vault = select_vault(g.vault.as_ref())?;
    let mut store = open_store(&vault, rebuild)?;
    let started = Instant::now();
    let report = refresh(&mut vault, &mut store, g.quiet, &INTERRUPTED)?;
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
        warnings: context::with_stored_warnings(&vault, &store)?,
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
        output::stdout(&lines(
            out.removed.iter().map(|id| format!("removed\t{id}")),
        ))?;
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
