//! The local socket of the desktop app (PRD "Gerbang 2: dampak arsitektur",
//! point 2): one app instance per vault listens on
//! `$XDG_RUNTIME_DIR/patinote/<vault-id>.sock`, and `patinote open` (or a
//! second `patinote-app`) sends it a request instead of starting another.
//!
//! The protocol is one JSON request per connection, one line each way. It is
//! private to Patinote; the local API of Phase 9 (F-25) builds on it.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};

/// How long a client waits for the app before deciding it is not running.
const TIMEOUT: Duration = Duration::from_secs(2);

/// What a client asks the running app.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "request", rename_all = "snake_case")]
pub enum Request {
    /// Bring the window forward.
    Activate,
    /// Open a note, given by its path relative to the vault.
    Open { path: String },
}

/// The app's answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Response {
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl Response {
    /// A successful answer.
    pub fn ok() -> Self {
        Self {
            ok: true,
            error: None,
        }
    }
}

/// `$XDG_RUNTIME_DIR`, when set to an absolute path. Without it there is no
/// single-instance socket.
pub fn runtime_dir() -> Option<PathBuf> {
    std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
}

/// `<runtime dir>/patinote/<vault-id>.sock`.
pub fn socket_path(runtime_dir: &Path, vault_id: &str) -> PathBuf {
    runtime_dir
        .join("patinote")
        .join(format!("{vault_id}.sock"))
}

/// Why a request could not be delivered.
#[derive(Debug, thiserror::Error)]
pub enum SendError {
    /// No app is listening on the socket.
    #[error("the app is not running")]
    NotRunning,
    #[error("the app did not answer: {0}")]
    Io(#[from] std::io::Error),
    #[error("the app sent an answer that is not understood: {0}")]
    Protocol(String),
}

/// Sends one request to the app listening on `socket` and returns its answer.
pub fn send(socket: &Path, request: &Request) -> Result<Response, SendError> {
    let mut stream = match UnixStream::connect(socket) {
        Ok(s) => s,
        Err(e)
            if matches!(
                e.kind(),
                std::io::ErrorKind::NotFound | std::io::ErrorKind::ConnectionRefused
            ) =>
        {
            return Err(SendError::NotRunning);
        }
        Err(e) => return Err(e.into()),
    };
    stream.set_read_timeout(Some(TIMEOUT))?;
    stream.set_write_timeout(Some(TIMEOUT))?;
    let mut line =
        serde_json::to_string(request).map_err(|e| SendError::Protocol(e.to_string()))?;
    line.push('\n');
    stream.write_all(line.as_bytes())?;
    let mut answer = String::new();
    BufReader::new(stream).read_line(&mut answer)?;
    serde_json::from_str(answer.trim_end()).map_err(|e| SendError::Protocol(e.to_string()))
}

/// Why the app could not start listening.
#[derive(Debug, thiserror::Error)]
pub enum BindError {
    /// Another app instance already listens for this vault.
    #[error("Patinote is already running for this vault")]
    AlreadyRunning,
    #[error("cannot listen on {}: {source}", path.display())]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
}

/// The listening side, owned by the app.
#[derive(Debug)]
pub struct Server {
    listener: UnixListener,
    path: PathBuf,
}

impl Server {
    /// Listens on `socket`. A socket file left by a crashed app (nobody
    /// answers on it) is replaced; a live one gives
    /// [`BindError::AlreadyRunning`].
    pub fn bind(socket: &Path) -> Result<Self, BindError> {
        let io = |source| BindError::Io {
            path: socket.to_path_buf(),
            source,
        };
        if let Some(parent) = socket.parent() {
            std::fs::create_dir_all(parent).map_err(io)?;
        }
        match UnixListener::bind(socket) {
            Ok(listener) => Ok(Self {
                listener,
                path: socket.to_path_buf(),
            }),
            Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => {
                if UnixStream::connect(socket).is_ok() {
                    return Err(BindError::AlreadyRunning);
                }
                std::fs::remove_file(socket).map_err(io)?;
                let listener = UnixListener::bind(socket).map_err(io)?;
                Ok(Self {
                    listener,
                    path: socket.to_path_buf(),
                })
            }
            Err(e) => Err(io(e)),
        }
    }

    /// Where the socket is.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Accepts connections until the listener fails, handing each request
    /// to `handle` and writing back its answer. Blocks; run it on its own
    /// thread. A client that sends something unreadable gets an error
    /// answer and does not stop the loop.
    pub fn serve(&self, mut handle: impl FnMut(Request) -> Response) {
        for stream in self.listener.incoming() {
            let Ok(stream) = stream else { continue };
            let _ = stream.set_read_timeout(Some(TIMEOUT));
            let _ = stream.set_write_timeout(Some(TIMEOUT));
            let mut line = String::new();
            let mut reader = BufReader::new(&stream);
            if reader.read_line(&mut line).is_err() {
                continue;
            }
            let response = match serde_json::from_str::<Request>(line.trim_end()) {
                Ok(request) => handle(request),
                Err(e) => Response {
                    ok: false,
                    error: Some(format!("unknown request: {e}")),
                },
            };
            if let Ok(mut answer) = serde_json::to_string(&response) {
                answer.push('\n');
                let _ = (&stream).write_all(answer.as_bytes());
            }
        }
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn a_request_reaches_the_server_and_its_answer_comes_back() {
        let dir = tempfile::tempdir().unwrap();
        let socket = socket_path(dir.path(), "abc");
        let server = Arc::new(Server::bind(&socket).unwrap());
        let (tx, rx) = std::sync::mpsc::channel();
        let s = Arc::clone(&server);
        std::thread::spawn(move || {
            s.serve(|r| {
                tx.send(r).unwrap();
                Response::ok()
            })
        });
        let request = Request::Open {
            path: "Proyek/Patinote.md".into(),
        };
        assert_eq!(send(&socket, &request).unwrap(), Response::ok());
        assert_eq!(rx.recv().unwrap(), request);
        assert_eq!(send(&socket, &Request::Activate).unwrap(), Response::ok());
    }

    #[test]
    fn a_second_instance_sees_the_first_and_a_stale_socket_is_replaced() {
        let dir = tempfile::tempdir().unwrap();
        let socket = socket_path(dir.path(), "abc");
        let first = Server::bind(&socket).unwrap();
        assert!(matches!(
            Server::bind(&socket),
            Err(BindError::AlreadyRunning)
        ));
        // A crash leaves the file behind with nobody listening.
        std::mem::forget(first);
        let stale = UnixListener::bind(dir.path().join("other.sock")).unwrap();
        drop(stale);
        let leftover = dir.path().join("other.sock");
        assert!(leftover.exists());
        let server = Server::bind(&leftover).unwrap();
        drop(server);
        assert!(!leftover.exists(), "the socket is removed on drop");
    }

    #[test]
    fn nothing_listening_is_not_running() {
        let dir = tempfile::tempdir().unwrap();
        assert!(matches!(
            send(&dir.path().join("none.sock"), &Request::Activate),
            Err(SendError::NotRunning)
        ));
    }
}
