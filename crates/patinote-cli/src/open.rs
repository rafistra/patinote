//! `patinote open <note>` (A-01): asks the running desktop app to open a note
//! over its socket (PRD "Gerbang 2: dampak arsitektur", point 2), and starts
//! the app when none is running.

use std::path::PathBuf;
use std::process::{Command, Stdio};

use patinote_core::commands::{ErrorCode, NoteRef, OpenOutput};
use patinote_core::ipc::{self, Request, SendError};
use patinote_core::names::stem;

use crate::commands::{Ctx, resolve_note};
use crate::context::io_failure;
use crate::output::Failure;

/// Overrides where the app binary is, for tests and unusual installs.
pub const APP_ENV: &str = "PATINOTE_APP";

/// The `patinote-app` binary: `$PATINOTE_APP`, then the one next to this
/// binary, then whatever `PATH` finds.
fn app_binary() -> PathBuf {
    if let Some(path) = std::env::var_os(APP_ENV) {
        return PathBuf::from(path);
    }
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join("patinote-app")))
        .filter(|p| p.is_file())
        .unwrap_or_else(|| PathBuf::from("patinote-app"))
}

/// Opens the note named by `arg` in the app.
pub fn open(ctx: &Ctx<'_>, arg: &str) -> Result<OpenOutput, Failure> {
    let row = resolve_note(ctx, arg)?;
    let note = NoteRef {
        title: row
            .title
            .clone()
            .unwrap_or_else(|| stem(&row.path).to_owned()),
        path: row.path,
    };
    let socket = ipc::runtime_dir().map(|dir| ipc::socket_path(&dir, &ctx.vault.id));
    let request = Request::Open {
        path: note.path.clone(),
    };
    let sent = match &socket {
        Some(socket) => ipc::send(socket, &request),
        None => Err(SendError::NotRunning),
    };
    let started = match sent {
        Ok(answer) if answer.ok => false,
        Ok(answer) => {
            return Err(Failure::new(
                ErrorCode::IoError,
                format!(
                    "the app could not open the note: {}",
                    answer.error.unwrap_or_default()
                ),
            ));
        }
        Err(SendError::NotRunning) => {
            Command::new(app_binary())
                .arg("--vault")
                .arg(&ctx.vault.root)
                .arg("--open")
                .arg(&note.path)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .map_err(|e| io_failure("cannot start patinote-app", e))?;
            true
        }
        Err(e) => return Err(io_failure("cannot reach patinote-app", e)),
    };
    Ok(OpenOutput {
        v: 1,
        note,
        started,
        warnings: Vec::new(),
    })
}
