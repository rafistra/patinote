//! The Vim engine against Neovim (F-06). Every case in `tests/vim/cases.json` is
//! replayed through the engine and compared with what `nvim --headless --clean`
//! produced for the same keys (`tests/vim/expected.json`, written by
//! `scripts/vim-fixtures.py`). Neovim is not needed to run these tests.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use patinote_app::vim::{Buffer, Config, Key, Mode, Pos, Register, Request, Vim};
use serde::Deserialize;

/// Text lines in the Neovim window the fixtures were made with (`lines=24`).
const VIEWPORT: usize = 22;

#[derive(Debug, Deserialize)]
struct Case {
    name: String,
    group: String,
    text: Vec<String>,
    /// 1-based line, 0-based char column.
    cursor: [usize; 2],
    keys: String,
    register: Option<Reg>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
struct Reg {
    text: String,
    linewise: bool,
}

#[derive(Debug, PartialEq, Eq, Deserialize)]
struct State {
    lines: Vec<String>,
    cursor: [usize; 2],
    mode: String,
    register: Reg,
}

#[derive(Debug, Deserialize)]
struct Expected {
    name: String,
    #[serde(flatten)]
    state: State,
}

/// Parses Vim key notation: `<Esc>`, `<CR>`, `<BS>`, `<C-w>`, `<lt>`…
fn parse_keys(notation: &str) -> Vec<Key> {
    let mut keys = Vec::new();
    let mut rest = notation;
    while let Some(c) = rest.chars().next() {
        if c == '<'
            && let Some(end) = rest.find('>')
            && end > 1
        {
            let name = &rest[1..end];
            let key = match name {
                "Esc" => Some(Key::Esc),
                "CR" | "Enter" => Some(Key::Enter),
                "BS" => Some(Key::Backspace),
                "Del" => Some(Key::Delete),
                "Tab" => Some(Key::Tab),
                "Left" => Some(Key::Left),
                "Right" => Some(Key::Right),
                "Up" => Some(Key::Up),
                "Down" => Some(Key::Down),
                "Home" => Some(Key::Home),
                "End" => Some(Key::End),
                "lt" => Some(Key::Char('<')),
                "Space" => Some(Key::Char(' ')),
                _ => name
                    .strip_prefix("C-")
                    .filter(|c| c.chars().count() == 1)
                    .and_then(|c| c.chars().next())
                    .map(|c| Key::Ctrl(c.to_ascii_lowercase())),
            };
            if let Some(key) = key {
                keys.push(key);
                rest = &rest[end + 1..];
                continue;
            }
        }
        keys.push(Key::Char(c));
        rest = &rest[c.len_utf8()..];
    }
    keys
}

fn mode_name(mode: Mode) -> &'static str {
    match mode {
        Mode::Normal => "normal",
        Mode::Insert => "insert",
        Mode::Visual => "visual",
        Mode::VisualLine => "visual_line",
    }
}

/// Runs a case through the engine, answering `/` and `?` prompts with the keys
/// up to `<CR>` like Neovim's command line.
fn run(case: &Case) -> State {
    // With a final newline, `from_text` keeps every line, a last empty one included.
    let mut buf = Buffer::from_text(&format!("{}\n", case.text.join("\n")));
    // Like Neovim's `setcursorcharpos()`, keep a normal-mode cursor on a character.
    let line = case.cursor[0] - 1;
    let len = buf.lines().get(line).map_or(0, |l| l.chars().count());
    buf.set_cursor(Pos::new(line, case.cursor[1].min(len.saturating_sub(1))));
    let mut vim = Vim::new(Config::default());
    if let Some(reg) = &case.register {
        vim.set_register(Register {
            text: reg.text.clone(),
            linewise: reg.linewise,
        });
    }
    let start = Instant::now();
    let keys = parse_keys(&case.keys);
    let mut i = 0;
    while i < keys.len() {
        let now = start + Duration::from_millis(i as u64);
        let out = vim.handle(&mut buf, keys[i], now, VIEWPORT);
        i += 1;
        if let Some(Request::Search { forward }) = out.request {
            let mut pattern = String::new();
            while i < keys.len() && keys[i] != Key::Enter {
                if let Key::Char(c) = keys[i] {
                    pattern.push(c);
                }
                i += 1;
            }
            i += 1;
            vim.search(&mut buf, &pattern, forward);
        }
    }
    let cursor = buf.cursor();
    let register = vim.register().map_or(
        Reg {
            text: String::new(),
            linewise: false,
        },
        |r| Reg {
            text: r.text.clone(),
            linewise: r.linewise,
        },
    );
    State {
        lines: buf.lines().to_vec(),
        cursor: [cursor.line + 1, cursor.col],
        mode: mode_name(vim.mode()).to_owned(),
        register,
    }
}

/// Reads the cases and Neovim's results; `VIM_FIXTURES` points at another folder
/// with the same two files (handy for trying out new cases).
fn load() -> (Vec<Case>, HashMap<String, State>) {
    let dir = std::env::var("VIM_FIXTURES")
        .unwrap_or_else(|_| concat!(env!("CARGO_MANIFEST_DIR"), "/tests/vim").to_owned());
    let cases: Vec<Case> = serde_json::from_str(
        &std::fs::read_to_string(format!("{dir}/cases.json")).expect("read cases.json"),
    )
    .expect("parse cases.json");
    let expected: Vec<Expected> = serde_json::from_str(
        &std::fs::read_to_string(format!("{dir}/expected.json")).expect("read expected.json"),
    )
    .expect("parse expected.json");
    let expected = expected.into_iter().map(|e| (e.name, e.state)).collect();
    (cases, expected)
}

/// Runs every case of `group` and reports all mismatches at once.
fn check_group(group: &str) {
    let (cases, expected) = load();
    let mut ran = 0;
    let mut failures = Vec::new();
    for case in cases.iter().filter(|c| c.group == group) {
        ran += 1;
        let Some(want) = expected.get(&case.name) else {
            failures.push(format!(
                "{}: no expected result, run scripts/vim-fixtures.py",
                case.name
            ));
            continue;
        };
        let got = run(case);
        if &got != want {
            failures.push(format!(
                "{} (keys {:?}):\n  nvim:   {:?}\n  engine: {:?}",
                case.name, case.keys, want, got
            ));
        }
    }
    assert!(ran > 0, "no cases in group {group}");
    assert!(
        failures.is_empty(),
        "{} of {ran} cases differ from Neovim:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn f06_1_every_command_matches_neovim() {
    check_group("f06_1");
}

#[test]
fn f06_2_counts_work_for_every_motion_and_operator() {
    check_group("f06_2");
}

#[test]
fn f06_3_one_undo_reverts_one_whole_command() {
    check_group("f06_3");
}

/// Random key sequences over every command, kept from a fuzzing run against Neovim.
#[test]
fn f06_1_random_sequences_match_neovim() {
    check_group("random");
}

#[test]
fn every_case_has_an_expected_result() {
    let (cases, expected) = load();
    assert_eq!(cases.len(), expected.len(), "run scripts/vim-fixtures.py");
    for case in &cases {
        assert!(
            expected.contains_key(&case.name),
            "{} has no result",
            case.name
        );
    }
}

// --- Not Vim builtins: tested against the engine directly ---------------------------

fn type_keys(vim: &mut Vim, buf: &mut Buffer, keys: &str, now: Instant) {
    for key in parse_keys(keys) {
        vim.handle(buf, key, now, VIEWPORT);
    }
}

#[test]
fn f05_7_undo_redo_100_edits_restore_exactly() {
    let mut buf = Buffer::from_text("Catatan uji\n\nbaris dua — café\n");
    let mut vim = Vim::new(Config::default());
    let now = Instant::now();
    let edits = [
        "Atambah<Esc>",
        ">>",
        "x",
        "ofoo bar<Esc>",
        "cwganti<Esc>",
        "yyP",
        "rX",
        "ggJ",
        "Ixy<Esc>",
        "Gdd",
    ];
    let mut history = vec![buf.text()];
    for i in 0..100 {
        let edit = edits[i % edits.len()];
        let before = buf.text();
        type_keys(&mut vim, &mut buf, edit, now);
        assert_ne!(buf.text(), before, "edit {i} ({edit}) changed nothing");
        assert_eq!(vim.mode(), Mode::Normal);
        history.push(buf.text());
    }
    for i in (0..100).rev() {
        assert!(vim.undo(&mut buf), "undo {i}");
        assert_eq!(buf.text(), history[i], "after undoing edit {i}");
    }
    assert!(!vim.undo(&mut buf), "nothing left to undo");
    for (i, text) in history.iter().enumerate().skip(1) {
        type_keys(&mut vim, &mut buf, "<C-r>", now);
        assert_eq!(&buf.text(), text, "after redoing edit {i}");
    }
    assert!(!vim.redo(&mut buf), "nothing left to redo");
}

#[test]
fn f06_7_insert_escape_jk_and_timeout() {
    let config = Config {
        insert_escape: Some("jk".to_owned()),
    };
    let t0 = Instant::now();
    let ms = |n| t0 + Duration::from_millis(n);

    // `jk` typed quickly leaves insert mode and leaves no letter behind.
    let mut buf = Buffer::from_text("abc");
    let mut vim = Vim::new(config.clone());
    vim.handle(&mut buf, Key::Char('A'), ms(0), VIEWPORT);
    vim.handle(&mut buf, Key::Char('x'), ms(10), VIEWPORT);
    let held = vim.handle(&mut buf, Key::Char('j'), ms(20), VIEWPORT);
    assert!(!held.changed, "the j is held, not inserted");
    assert_eq!(vim.pending(), "j");
    vim.handle(&mut buf, Key::Char('k'), ms(300), VIEWPORT);
    assert_eq!(vim.mode(), Mode::Normal);
    assert_eq!(buf.text(), "abcx");
    assert_eq!(buf.cursor(), Pos::new(0, 3));
    assert_eq!(vim.pending(), "");
    // One undo removes the whole insert, the held j included.
    assert!(vim.undo(&mut buf));
    assert_eq!(buf.text(), "abc");

    // `j`, a pause over one second, then `k`: both are inserted.
    let mut buf = Buffer::from_text("abc");
    let mut vim = Vim::new(config.clone());
    vim.handle(&mut buf, Key::Char('A'), ms(0), VIEWPORT);
    vim.handle(&mut buf, Key::Char('j'), ms(10), VIEWPORT);
    vim.handle(&mut buf, Key::Char('k'), ms(1100), VIEWPORT);
    assert_eq!(vim.mode(), Mode::Insert);
    assert_eq!(buf.text(), "abcjk");

    // The held j also appears on its own once the timeout passes (`tick`).
    let mut buf = Buffer::from_text("abc");
    let mut vim = Vim::new(config.clone());
    vim.handle(&mut buf, Key::Char('A'), ms(0), VIEWPORT);
    vim.handle(&mut buf, Key::Char('j'), ms(10), VIEWPORT);
    assert_eq!(vim.deadline(), Some(ms(1010)));
    assert!(!vim.tick(&mut buf, ms(500)).changed);
    assert!(vim.tick(&mut buf, ms(1011)).changed);
    assert_eq!(buf.text(), "abcj");
    assert_eq!(vim.pending(), "");

    // `j` followed by another letter inserts both right away.
    let mut buf = Buffer::from_text("");
    let mut vim = Vim::new(config.clone());
    type_keys(&mut vim, &mut buf, "ijanji", ms(0));
    assert_eq!(buf.text(), "janji");
    assert_eq!(vim.mode(), Mode::Insert);

    // `jj` then `k`: the first j is inserted, the second pair escapes.
    let mut buf = Buffer::from_text("");
    let mut vim = Vim::new(config.clone());
    type_keys(&mut vim, &mut buf, "ijjk", ms(0));
    assert_eq!(buf.text(), "j");
    assert_eq!(vim.mode(), Mode::Normal);

    // Off by default: `jk` is plain text.
    let mut buf = Buffer::from_text("");
    let mut vim = Vim::new(Config::default());
    type_keys(&mut vim, &mut buf, "ijk", ms(0));
    assert_eq!(buf.text(), "jk");
    assert_eq!(vim.mode(), Mode::Insert);

    // `.` repeats the insert without the escape keys.
    let mut buf = Buffer::from_text("");
    let mut vim = Vim::new(config);
    type_keys(&mut vim, &mut buf, "iab", ms(0));
    vim.handle(&mut buf, Key::Char('j'), ms(10), VIEWPORT);
    vim.handle(&mut buf, Key::Char('k'), ms(20), VIEWPORT);
    vim.handle(&mut buf, Key::Char('.'), ms(30), VIEWPORT);
    assert_eq!(buf.text(), "aabb");
}

#[test]
fn requests_and_pending_keys() {
    let mut buf = Buffer::from_text("satu dua");
    let mut vim = Vim::new(Config::default());
    let now = Instant::now();
    let out = vim.handle(&mut buf, Key::Char(':'), now, VIEWPORT);
    assert_eq!(out.request, Some(Request::CommandLine));
    let out = vim.handle(&mut buf, Key::Char('?'), now, VIEWPORT);
    assert_eq!(out.request, Some(Request::Search { forward: false }));
    vim.handle(&mut buf, Key::Char('2'), now, VIEWPORT);
    vim.handle(&mut buf, Key::Char('d'), now, VIEWPORT);
    assert_eq!(vim.pending(), "2d");
    vim.handle(&mut buf, Key::Esc, now, VIEWPORT);
    assert_eq!(vim.pending(), "");
    assert_eq!(buf.text(), "satu dua");
    // `yw` reports the register for the clipboard and does not change the text.
    let out = vim.handle(&mut buf, Key::Char('y'), now, VIEWPORT);
    assert!(out.yanked.is_none());
    let out = vim.handle(&mut buf, Key::Char('w'), now, VIEWPORT);
    assert!(!out.changed);
    assert_eq!(
        out.yanked,
        Some(Register {
            text: "satu ".to_owned(),
            linewise: false
        })
    );
    // Visual mode reports its selection.
    type_keys(&mut vim, &mut buf, "wvl", now);
    assert_eq!(vim.selection(&buf), Some((Pos::new(0, 5), Pos::new(0, 6))));
    assert!(vim.search(&mut buf, "satu", true));
    assert_eq!(vim.last_search(), Some("satu"));
    assert!(!vim.search(&mut buf, "tidak ada", true));
}

#[test]
fn buffer_keeps_the_final_newline() {
    assert_eq!(Buffer::from_text("a\nb\n").lines(), ["a", "b"]);
    assert_eq!(Buffer::from_text("a\nb\n").text(), "a\nb\n");
    assert_eq!(Buffer::from_text("a\nb").text(), "a\nb");
    assert_eq!(Buffer::from_text("").lines(), [""]);
    assert_eq!(Buffer::from_text("\n").text(), "\n");
    let mut buf = Buffer::from_text("é—ü\n");
    buf.set_cursor(Pos::new(5, 9));
    assert_eq!(buf.cursor(), Pos::new(0, 3));
}
