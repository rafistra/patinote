//! `patinote open` (A-01): talks to a running app over its socket, or starts
//! the app (PRD "Gerbang 2: dampak arsitektur", point 2).

mod common;

use std::sync::Arc;

use common::{Env, json};
use patinote_core::ipc::{Request, Response, Server, socket_path};

#[test]
fn open_sends_the_note_to_the_running_app() {
    let env = Env::new();
    let vault = env.fixture();
    let runtime = env.path("run");
    let id = patinote_core::location::vault_id(&vault).unwrap();
    let server = Arc::new(Server::bind(&socket_path(&runtime, &id)).unwrap());
    let (tx, rx) = std::sync::mpsc::channel();
    let s = Arc::clone(&server);
    std::thread::spawn(move || {
        s.serve(|r| {
            tx.send(r).unwrap();
            Response::ok()
        })
    });

    let r = env.run_with(
        &[
            "--vault",
            vault.to_str().unwrap(),
            "open",
            "second brain",
            "--json",
        ],
        &[("XDG_RUNTIME_DIR", runtime.to_str().unwrap())],
    );
    assert_eq!(r.code, 0, "{}", r.stderr);
    let out = json(&r.stdout);
    assert_eq!(out["note"]["path"], "Proyek/Patinote.md");
    assert_eq!(out["started"], false);
    assert_eq!(
        rx.recv().unwrap(),
        Request::Open {
            path: "Proyek/Patinote.md".into()
        }
    );
}

#[test]
fn open_starts_the_app_when_none_is_running() {
    let env = Env::new();
    let vault = env.fixture();
    // A stand-in app that records its arguments.
    let args_file = env.path("args.txt");
    let fake = env.path("fake-app.sh");
    std::fs::write(
        &fake,
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}'\n",
            args_file.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&fake, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();

    let r = env.run_with(
        &[
            "--vault",
            vault.to_str().unwrap(),
            "open",
            "Desain indeks Patinote",
        ],
        &[
            ("XDG_RUNTIME_DIR", env.path("run").to_str().unwrap()),
            ("PATINOTE_APP", fake.to_str().unwrap()),
        ],
    );
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert_eq!(r.stdout, "Proyek/Desain indeks Patinote.md\tstarted\n");
    let started = std::time::Instant::now();
    while !args_file.exists() && started.elapsed().as_secs() < 5 {
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let args = std::fs::read_to_string(&args_file).unwrap();
    let canonical = std::fs::canonicalize(&vault).unwrap();
    assert_eq!(
        args,
        format!(
            "--vault\n{}\n--open\nProyek/Desain indeks Patinote.md\n",
            canonical.display()
        )
    );
}

#[test]
fn open_follows_the_exit_codes_of_other_note_commands() {
    let env = Env::new();
    let vault = env.fixture();
    let v = vault.to_str().unwrap();
    let missing = env.run(&["--vault", v, "open", "Nothing here"]);
    assert_eq!(missing.code, 3);
    let ambiguous = env.run(&["--vault", v, "open", "Rapat"]);
    assert_eq!(ambiguous.code, 4);
}
