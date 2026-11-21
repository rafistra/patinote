//! The Vim engine for the editor (F-06): modes, motions, operators, counts,
//! `.` repeat, and undo, as pure logic without Iced types so every command can
//! be tested without a window (CLAUDE.md, section 5).
//!
//! The reference is `nvim --clean` (Neovim 0.11), with one change:
//! `'formatoptions'` without `j`, so `J` keeps Markdown markers such as `#` and
//! `-` that Neovim would take for comment leaders. The options that matter are
//! therefore Neovim's defaults: `autoindent`, `nostartofline`, `smarttab`,
//! `tabstop=8`, `shiftwidth=8`, `noexpandtab`, `backspace=indent,eol,start`,
//! `wrapscan`, `selection=inclusive`, `whichwrap=b,s`, `nojoinspaces`, and the
//! default mappings `Y` → `y$` and insert-mode `Ctrl+w`/`Ctrl+u` with an undo break.
//! The tests in `tests/vim.rs` replay cases whose expected results were produced
//! by Neovim (`scripts/vim-fixtures.py`).
//!
//! Known differences, all outside what Phase 2 needs:
//! - Search patterns are literal, case-sensitive text, not Vim regexes.
//! - Columns are char indices and every char except a tab is one cell wide, so
//!   `j`/`k` across wide (CJK) characters may pick another column than Vim.
//!   Word motions treat CJK and emoji as word characters; Vim gives each script
//!   a class of its own.
//! - `Ctrl+d`/`Ctrl+u` move by text lines; Neovim moves by screen lines when
//!   long lines wrap.
//! - Text objects in visual mode work on a fresh selection (one character); Vim
//!   can also grow an existing selection with them.
//! - `.` after a counted `o`/`O` whose text has a backspace can differ: Neovim's
//!   redo of that combination skips the backspace.
//! - The fixtures feed keys to Neovim as fast typing, without a redraw between
//!   keys; where Neovim only updates its remembered column on redraw, the engine
//!   follows the interactive behaviour.
//! - Unsupported commands are ignored: named registers, marks, macros, `gj`/`gk`
//!   (the editor widget handles display lines), `r` with Enter, and window
//!   commands (`Ctrl+w` swallows the next key, as in Vim).

mod buffer;
mod engine;
mod motion;
mod text;

pub use buffer::Buffer;
pub use engine::Vim;

/// A position: line index and char index within the line (not bytes).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Hash)]
pub struct Pos {
    /// Line index, from 0.
    pub line: usize,
    /// Char index within the line, from 0. Equal to the line length at the line end.
    pub col: usize,
}

impl Pos {
    /// A position from a line and a char column.
    pub fn new(line: usize, col: usize) -> Self {
        Self { line, col }
    }
}

/// Editing mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    /// Normal mode.
    #[default]
    Normal,
    /// Insert mode.
    Insert,
    /// Charwise visual mode (`v`).
    Visual,
    /// Linewise visual mode (`V`).
    VisualLine,
}

/// A key press, already decoded from the window system.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    /// A printable character (Shift already applied).
    Char(char),
    /// Ctrl plus a character, lowercase for letters (`Ctrl('d')`); `Ctrl('[')` is Escape.
    Ctrl(char),
    /// Escape.
    Esc,
    /// Enter (Return).
    Enter,
    /// Backspace.
    Backspace,
    /// Delete.
    Delete,
    /// Tab.
    Tab,
    /// Arrow left.
    Left,
    /// Arrow right.
    Right,
    /// Arrow up.
    Up,
    /// Arrow down.
    Down,
    /// Home.
    Home,
    /// End.
    End,
}

/// Text yanked or deleted, for the system clipboard (F-06: `clipboard=unnamedplus`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Register {
    /// The text. Linewise text ends with `'\n'`, like Vim's `getreg()`.
    pub text: String,
    /// Whole lines (`yy`, `dd`) rather than characters.
    pub linewise: bool,
}

/// What a key did that the app must act on.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Outcome {
    /// The text changed.
    pub changed: bool,
    /// `y`, `d`, `c`, `x` and the like filled the unnamed register: the app copies
    /// it to the clipboard.
    pub yanked: Option<Register>,
    /// The app should open a prompt.
    pub request: Option<Request>,
    /// A count asked for more text than the engine inserts at once
    /// (`MAX_REPEAT_BYTES`): the count was not used.
    pub too_large: bool,
}

/// A prompt the app opens for the engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Request {
    /// `:` was pressed in normal or visual mode.
    CommandLine,
    /// `/` (`forward`) or `?` was pressed in normal or visual mode. Submit the
    /// pattern with [`Vim::search`].
    Search {
        /// `/` searches forward, `?` backward.
        forward: bool,
    },
}

/// Engine options from the user's configuration.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Config {
    /// Keys typed quickly in insert mode that act as Escape, such as `"jk"`
    /// (F-06.7). Off when `None`.
    pub insert_escape: Option<String>,
}
