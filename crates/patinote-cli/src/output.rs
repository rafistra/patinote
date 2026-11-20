//! Writing results. With `--json`, stdout holds exactly one JSON document and
//! everything else goes to stderr (A-02). A reader that closes the pipe early
//! (`patinote … | head`) ends the program quietly (A-03.3).

use std::io::{self, Write};

use patinote_core::commands::{ErrorBody, ErrorCode, ErrorOutput, OutputWarning};
use serde::Serialize;

/// Why a command did not finish normally.
#[derive(Debug)]
pub enum Failure {
    /// A reported error with its code (and exit code).
    Error(ErrorBody),
    /// Ctrl+C (exit code 130).
    Interrupted,
    /// stdout was closed by the reader; stop quietly.
    BrokenPipe,
}

impl Failure {
    /// An error without candidates.
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self::Error(ErrorBody {
            code,
            message: message.into(),
            candidates: None,
        })
    }
}

/// Writes text to stdout.
pub fn stdout(text: &str) -> Result<(), Failure> {
    let mut out = io::stdout().lock();
    match out.write_all(text.as_bytes()).and_then(|()| out.flush()) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::BrokenPipe => Err(Failure::BrokenPipe),
        Err(e) => Err(Failure::new(
            ErrorCode::IoError,
            format!("cannot write output: {e}"),
        )),
    }
}

/// Writes one JSON document to stdout.
pub fn json<T: Serialize>(value: &T) -> Result<(), Failure> {
    let mut text = serde_json::to_string_pretty(value)
        .map_err(|e| Failure::new(ErrorCode::IoError, format!("cannot encode output: {e}")))?;
    text.push('\n');
    stdout(&text)
}

/// Prints warnings on stderr in text mode, unless `--quiet`.
pub fn warnings_text(warnings: &[OutputWarning], quiet: bool) {
    if quiet {
        return;
    }
    let mut err = io::stderr().lock();
    for w in warnings {
        let _ = match (&w.path, w.line) {
            (Some(p), Some(l)) => writeln!(err, "warning: {p}:{l}: {}", w.message),
            (Some(p), None) => writeln!(err, "warning: {p}: {}", w.message),
            _ => writeln!(err, "warning: {}", w.message),
        };
    }
}

/// Reports an error on stderr, as JSON with `--json`, and returns its exit code.
pub fn error(body: ErrorBody, as_json: bool) -> i32 {
    let code = body.code.exit_code();
    let mut err = io::stderr().lock();
    if as_json {
        let doc = ErrorOutput { v: 1, error: body };
        if let Ok(text) = serde_json::to_string_pretty(&doc) {
            let _ = writeln!(err, "{text}");
        }
    } else {
        let _ = writeln!(err, "error: {}", body.message);
        for c in body.candidates.unwrap_or_default() {
            let _ = writeln!(err, "  {c}");
        }
    }
    code
}
