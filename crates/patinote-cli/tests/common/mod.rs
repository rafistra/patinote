//! Shared helpers: every run gets its own HOME and XDG folders, so tests
//! never touch the real cache, state or configuration.

#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde_json::Value;

pub struct Env {
    pub dir: tempfile::TempDir,
}

pub struct Run {
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
}

impl Env {
    pub fn new() -> Self {
        Self {
            dir: tempfile::tempdir().unwrap(),
        }
    }
    pub fn path(&self, rel: &str) -> PathBuf {
        self.dir.path().join(rel)
    }
    pub fn cmd(&self, args: &[&str]) -> Command {
        let mut c = Command::new(env!("CARGO_BIN_EXE_patinote"));
        c.args(args)
            .env_clear()
            .env("HOME", self.path("home"))
            .env("XDG_CACHE_HOME", self.path("cache"))
            .env("XDG_STATE_HOME", self.path("state"))
            .env("XDG_CONFIG_HOME", self.path("config"))
            .stdin(Stdio::null());
        c
    }
    pub fn run(&self, args: &[&str]) -> Run {
        self.run_with(args, &[])
    }
    pub fn run_with(&self, args: &[&str], env: &[(&str, &str)]) -> Run {
        let mut c = self.cmd(args);
        for (k, v) in env {
            c.env(k, v);
        }
        let out = c.output().unwrap();
        Run {
            code: out.status.code().unwrap_or(-1),
            stdout: String::from_utf8(out.stdout).unwrap(),
            stderr: String::from_utf8(out.stderr).unwrap(),
        }
    }
    /// A copy of the fixture vault, so nothing can ever write to the fixture.
    /// Modification times are fixed (one second apart, by path), so results
    /// ordered by time, like backlinks, are the same on every machine.
    pub fn fixture(&self) -> PathBuf {
        let dest = self.path("vault");
        copy_dir(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/vault-mockup"),
            &dest,
        );
        let mut files: Vec<PathBuf> = hashes(&dest)
            .into_iter()
            .map(|(p, _)| PathBuf::from(p))
            .collect();
        files.sort();
        let base = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_795_046_400);
        for (i, f) in files.iter().enumerate() {
            let file = fs::File::options().write(true).open(f).unwrap();
            file.set_modified(base + std::time::Duration::from_secs(i as u64))
                .unwrap();
        }
        dest
    }
    pub fn write(&self, rel: &str, text: &str) {
        let p = self.path(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, text).unwrap();
    }
}

pub fn copy_dir(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for e in fs::read_dir(from).unwrap() {
        let e = e.unwrap();
        if e.file_type().unwrap().is_dir() {
            copy_dir(&e.path(), &to.join(e.file_name()));
        } else {
            fs::copy(e.path(), to.join(e.file_name())).unwrap();
        }
    }
}

pub fn json(text: &str) -> Value {
    serde_json::from_str(text).unwrap_or_else(|e| panic!("not one JSON document ({e}):\n{text}"))
}

pub fn hashes(root: &Path) -> Vec<(String, Vec<u8>)> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in fs::read_dir(&d).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                stack.push(p);
            } else {
                out.push((p.to_string_lossy().into_owned(), fs::read(&p).unwrap()));
            }
        }
    }
    out.sort();
    out
}
