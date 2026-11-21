//! The Vim state machine: parses keys into commands and runs them on a [`Buffer`].

use std::iter::repeat_n;
use std::time::{Duration, Instant};

use super::buffer::UndoCursor;
use super::motion::{
    ObjectRange, bck_word, current_block, current_quote, current_word, end_word, find_char,
    fwd_word,
};
use super::text::{
    SHIFTWIDTH, Walker, class, coladvance, cursor_vcol, first_non_blank, in_indent, indent_of,
    indent_width, is_blank, make_indent,
};
use super::{Buffer, Config, Key, Mode, Outcome, Pos, Register, Request};

/// How long `insert_escape` waits for its next key, like Vim's `'timeoutlen'`.
const ESCAPE_TIMEOUT: Duration = Duration::from_secs(1);

/// Counts are capped so a stray `99999999` cannot stall the editor.
const MAX_COUNT: usize = 100_000;

/// `curswant` value for "end of line" (`$`).
const END: usize = usize::MAX;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Op {
    Delete,
    Change,
    Yank,
    ShiftRight,
    ShiftLeft,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Motion {
    Left,
    Right,
    /// Backspace: `h` that wraps to the end of the previous line.
    WrapLeft,
    /// Space: `l` that wraps to the start of the next line.
    WrapRight,
    Down,
    Up,
    NextLine,
    WordFwd(bool),
    WordBack(bool),
    WordEnd(bool),
    LineStart,
    /// `<Home>`: Vim's `|` with a count of 1, which also sets `curswant`.
    Home,
    FirstNonBlank,
    LineEnd,
    FirstLine,
    LastLine,
    HalfDown,
    HalfUp,
    Find {
        forward: bool,
        till: bool,
        ch: char,
    },
    RepeatFind {
        reverse: bool,
    },
    SearchNext {
        reverse: bool,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Object {
    Word {
        big: bool,
        include: bool,
    },
    Quote {
        ch: char,
        include: bool,
    },
    Block {
        open: char,
        close: char,
        include: bool,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Target {
    Motion(Motion),
    Lines,
    Object(Object),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InsertCmd {
    Before,
    After,
    LineStart,
    LineEnd,
    OpenBelow,
    OpenAbove,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Cmd {
    Cancel,
    CommandLine,
    Search(bool),
    Motion(Motion),
    /// An operator, its target, and a forced motion type (`dvj`, `dVw`).
    Operator(Op, Target, Option<char>),
    Insert(InsertCmd),
    Replace(char),
    Join,
    Put {
        before: bool,
    },
    Undo,
    Redo,
    Repeat,
    Visual(Mode),
    VisualOp(Op),
    VisualJoin,
    VisualPut {
        before: bool,
    },
    VisualSwap,
    VisualObject(Object),
}

enum Parse {
    Incomplete,
    Invalid,
    Done {
        count: Option<usize>,
        cmd: Cmd,
        body: Vec<Key>,
    },
}

/// How a motion includes its end, for operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Exclusive,
    Inclusive,
    Linewise,
}

/// What a motion does to the remembered column.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Want {
    Update,
    Keep,
    End,
}

struct MotionResult {
    pos: Pos,
    kind: Kind,
    want: Want,
    /// Skip the exclusive-to-linewise adjustment (Vim's `CA_NO_ADJ_OP_END`).
    no_adjust: bool,
}

/// How insert mode repeats its text for a count (`3ihi<Esc>`, `3ohi<Esc>`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Repeat {
    Text,
    Lines,
    None,
}

#[derive(Debug, Clone)]
struct InsertState {
    /// Vim's `Insstart`: where the text being inserted starts.
    start: Pos,
    /// Vim's `Insstart_orig`: `Ctrl+w` and `Ctrl+u` stop here first. It follows
    /// `start` after every key until `update_orig` is cleared.
    orig: Pos,
    update_orig: bool,
    /// Vim's `ins_need_undo`: nothing changed yet since the insert or the last
    /// undo break.
    need_undo: bool,
    count: usize,
    repeat: Repeat,
    /// The line's indent was inserted automatically and nothing typed yet.
    did_ai: bool,
    /// An arrow key ended the insert for undo and `.`.
    arrow_used: bool,
    /// Keys typed since the insert (re)started, for the count repeat.
    typed: Vec<Key>,
}

/// What `.` repeats: a count and the keys of the last change.
#[derive(Debug, Clone)]
struct Change {
    count: Option<usize>,
    keys: Vec<Key>,
    visual: Option<VisualRedo>,
    /// Leaves the register alone (the delete of a visual `P`, Vim's `"_d`).
    keep_register: bool,
}

/// The size of the visual area a repeated visual operator works on.
#[derive(Debug, Clone, Copy)]
struct VisualRedo {
    mode: Mode,
    lines: usize,
    cols: usize,
    /// The selection ran to the line ends (`$`).
    to_end: bool,
}

/// The Vim engine: mode, pending keys, registers, and repeat state.
///
/// One engine serves every note; undo history lives in each [`Buffer`].
#[derive(Debug, Clone)]
pub struct Vim {
    config: Config,
    mode: Mode,
    keys: Vec<Key>,
    pending: String,
    anchor: Pos,
    curswant: usize,
    /// `curswant` when the current operator started.
    old_col: usize,
    /// The last command failed (Vim beeps).
    failed: bool,
    /// Forces `old_col` for the delete of a visual put.
    curswant_override: Option<usize>,
    /// The operator whose motion is running.
    pending_op: Option<Op>,
    /// `v` or `V` typed after the operator.
    force: Option<char>,
    /// The area of the last visual operator, for `{count}v` (Vim's `resel_VIsual`).
    resel: Option<VisualRedo>,
    keep_curswant: bool,
    last_cursor: Option<Pos>,
    register: Option<Register>,
    search: Option<String>,
    search_forward: bool,
    find: Option<(bool, bool, char)>,
    scroll: Option<usize>,
    insert: Option<InsertState>,
    recording: Option<Change>,
    last_change: Option<Change>,
    replaying: bool,
    held: Vec<char>,
    held_at: Option<Instant>,
}

impl Vim {
    /// A new engine in normal mode.
    pub fn new(config: Config) -> Self {
        Self {
            config,
            mode: Mode::Normal,
            keys: Vec::new(),
            pending: String::new(),
            anchor: Pos::default(),
            curswant: 0,
            old_col: 0,
            failed: false,
            curswant_override: None,
            pending_op: None,
            force: None,
            resel: None,
            keep_curswant: false,
            last_cursor: None,
            register: None,
            search: None,
            search_forward: true,
            find: None,
            scroll: None,
            insert: None,
            recording: None,
            last_change: None,
            replaying: false,
            held: Vec::new(),
            held_at: None,
        }
    }

    /// The current mode.
    pub fn mode(&self) -> Mode {
        self.mode
    }

    /// The unnamed register, if anything was yanked, deleted, or set.
    pub fn register(&self) -> Option<&Register> {
        self.register.as_ref()
    }

    /// Puts clipboard text into the unnamed register so `p`/`P` paste it.
    pub fn set_register(&mut self, register: Register) {
        self.register = Some(register);
    }

    /// The last search pattern, for highlighting all matches.
    pub fn last_search(&self) -> Option<&str> {
        self.search.as_deref()
    }

    /// Keys typed but not yet a full command (count, operator, `g`, `f`…), or the
    /// first keys of `insert_escape` held in insert mode, for display.
    pub fn pending(&self) -> &str {
        &self.pending
    }

    /// When held `insert_escape` keys time out and must be inserted; call
    /// [`Vim::tick`] then.
    pub fn deadline(&self) -> Option<Instant> {
        self.held_at.map(|at| at + ESCAPE_TIMEOUT)
    }

    /// Inserts held `insert_escape` keys once their timeout has passed (F-06.7).
    pub fn tick(&mut self, buf: &mut Buffer, now: Instant) -> Outcome {
        let rev = buf.revision();
        if self.mode == Mode::Insert
            && let Some(at) = self.held_at
            && now.duration_since(at) > ESCAPE_TIMEOUT
        {
            self.flush_held(buf);
        }
        self.update_pending();
        self.last_cursor = Some(buf.cursor());
        Outcome {
            changed: buf.revision() != rev,
            ..Outcome::default()
        }
    }

    /// The visual selection, inclusive, `start <= end`; `None` outside visual modes.
    /// In charwise visual mode `end.col` equals the line length when the line break
    /// is selected. In linewise mode the columns span whole lines.
    pub fn selection(&self, buf: &Buffer) -> Option<(Pos, Pos)> {
        let cursor = buf.cursor();
        let (start, end) = if self.anchor <= cursor {
            (self.anchor, cursor)
        } else {
            (cursor, self.anchor)
        };
        match self.mode {
            Mode::Visual => Some((start, end)),
            Mode::VisualLine => Some((
                Pos::new(start.line, 0),
                Pos::new(end.line, buf.line_len(end.line)),
            )),
            Mode::Normal | Mode::Insert => None,
        }
    }

    /// Handles one key. `now` drives the `insert_escape` timeout (F-06.7).
    /// `viewport_lines` is how many text lines fit on screen (`Ctrl+d`/`Ctrl+u`
    /// move half of it).
    pub fn handle(
        &mut self,
        buf: &mut Buffer,
        key: Key,
        now: Instant,
        viewport_lines: usize,
    ) -> Outcome {
        self.sync(buf);
        let rev = buf.revision();
        let mut out = Outcome::default();
        self.feed(buf, key, now, viewport_lines, &mut out);
        if self.mode != Mode::Insert && self.keys.is_empty() {
            buf.end_step();
        }
        self.update_pending();
        out.changed = buf.revision() != rev;
        self.last_cursor = Some(buf.cursor());
        out
    }

    /// Jumps to the next match of `pattern` after the app's `/` or `?` prompt,
    /// wrapping around the end like Vim's `'wrapscan'`, and remembers it for
    /// `n`/`N`. Matching is literal and case-sensitive. An empty pattern repeats
    /// the last one. Returns false when there is no match (cursor unchanged).
    pub fn search(&mut self, buf: &mut Buffer, pattern: &str, forward: bool) -> bool {
        self.sync(buf);
        let pattern = if pattern.is_empty() {
            match &self.search {
                Some(p) => p.clone(),
                None => return false,
            }
        } else {
            pattern.to_owned()
        };
        self.search = Some(pattern.clone());
        self.search_forward = forward;
        let found = find_pattern(buf, buf.cursor(), &pattern, forward);
        if let Some(pos) = found {
            buf.set_cursor_raw(pos);
            self.update_curswant(buf);
        }
        self.last_cursor = Some(buf.cursor());
        found.is_some()
    }

    /// Undoes the last whole command (also `u`).
    pub fn undo(&mut self, buf: &mut Buffer) -> bool {
        self.sync(buf);
        self.leave_to_normal(buf);
        let done = match buf.undo() {
            Some(c) => {
                self.place_after_undo(buf, c);
                true
            }
            None => false,
        };
        self.last_cursor = Some(buf.cursor());
        done
    }

    /// Redoes the last undone command (also `Ctrl+r`).
    pub fn redo(&mut self, buf: &mut Buffer) -> bool {
        self.sync(buf);
        self.leave_to_normal(buf);
        let done = match buf.redo() {
            Some(c) => {
                self.place_after_undo(buf, c);
                true
            }
            None => false,
        };
        self.last_cursor = Some(buf.cursor());
        done
    }

    // --- Plumbing --------------------------------------------------------------------

    /// Picks up a cursor the app moved since the last key.
    fn sync(&mut self, buf: &Buffer) {
        if self.last_cursor != Some(buf.cursor()) {
            self.update_curswant(buf);
        }
    }

    fn update_curswant(&mut self, buf: &Buffer) {
        let pos = buf.cursor();
        let chars = buf.chars(pos.line);
        let first_cell = self.mode == Mode::Insert || (self.is_visual() && pos <= self.anchor);
        self.curswant = cursor_vcol(&chars, pos.col, first_cell);
    }

    fn update_pending(&mut self) {
        self.pending.clear();
        if self.mode == Mode::Insert {
            self.pending.extend(self.held.iter());
            return;
        }
        for key in &self.keys {
            match key {
                Key::Char(c) => self.pending.push(*c),
                Key::Ctrl(c) => {
                    self.pending.push('^');
                    self.pending.push(c.to_ascii_uppercase());
                }
                _ => {}
            }
        }
    }

    fn fill_register(&mut self, register: Register, out: &mut Outcome) {
        self.register = Some(register.clone());
        out.yanked = Some(register);
    }

    fn is_visual(&self) -> bool {
        matches!(self.mode, Mode::Visual | Mode::VisualLine)
    }

    /// Keeps the normal-mode cursor on a character.
    fn clamp_normal(&self, buf: &mut Buffer) {
        if self.mode == Mode::Normal {
            let pos = buf.cursor();
            let len = buf.line_len(pos.line);
            buf.set_cursor_raw(Pos::new(pos.line, pos.col.min(len.saturating_sub(1))));
        }
    }

    /// Leaves insert or visual mode without side effects, for the public undo/redo.
    fn leave_to_normal(&mut self, buf: &mut Buffer) {
        if self.mode == Mode::Insert {
            self.insert = None;
            self.held.clear();
            self.held_at = None;
            if let Some(mut change) = self.recording.take() {
                change.keys.push(Key::Esc);
                self.last_change = Some(change);
            }
        }
        self.keys.clear();
        self.mode = Mode::Normal;
        self.clamp_normal(buf);
        buf.end_step();
        self.update_pending();
    }

    fn place_after_undo(&mut self, buf: &mut Buffer, cursor: UndoCursor) {
        let pos = match cursor {
            UndoCursor::At(pos) => pos,
            UndoCursor::Line(line) => {
                let chars = buf.chars(line);
                Pos::new(line, coladvance(&chars, self.curswant, false))
            }
        };
        buf.set_cursor(pos);
        self.clamp_normal(buf);
        self.update_curswant(buf);
    }

    fn feed(&mut self, buf: &mut Buffer, key: Key, now: Instant, vp: usize, out: &mut Outcome) {
        if self.mode == Mode::Insert {
            self.insert_key(buf, key, now);
            return;
        }
        self.keys.push(key);
        match parse(&self.keys, self.is_visual()) {
            Parse::Incomplete => {}
            Parse::Invalid => self.keys.clear(),
            Parse::Done { count, cmd, body } => {
                self.keys.clear();
                self.keep_curswant = false;
                self.execute(buf, count, cmd, body, now, vp, out);
                if !self.keep_curswant {
                    self.update_curswant(buf);
                }
                self.keep_curswant = false;
            }
        }
    }

    fn start_recording(
        &mut self,
        count: Option<usize>,
        body: Vec<Key>,
        visual: Option<VisualRedo>,
    ) {
        if !self.replaying {
            self.recording = Some(Change {
                count,
                keys: body,
                visual,
                keep_register: false,
            });
        }
    }

    /// Ends the recording of a change that did not enter insert mode.
    fn finish_recording(&mut self, done: bool) {
        if !done {
            // A command that failed leaves the remembered column alone.
            self.keep_curswant = true;
            self.failed = true;
        }
        if self.mode == Mode::Insert {
            return;
        }
        if let Some(change) = self.recording.take()
            && done
        {
            self.last_change = Some(change);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn execute(
        &mut self,
        buf: &mut Buffer,
        count: Option<usize>,
        cmd: Cmd,
        body: Vec<Key>,
        now: Instant,
        vp: usize,
        out: &mut Outcome,
    ) {
        match cmd {
            Cmd::Cancel => {
                if self.is_visual() {
                    self.mode = Mode::Normal;
                    self.clamp_normal(buf);
                }
            }
            Cmd::CommandLine => out.request = Some(Request::CommandLine),
            Cmd::Search(forward) => out.request = Some(Request::Search { forward }),
            Cmd::Motion(m) => match self.motion(buf, m, count, false, vp) {
                Some(r) => {
                    buf.set_cursor_raw(r.pos);
                    self.apply_want(buf, r.want);
                }
                None => self.keep_curswant = true,
            },
            Cmd::Operator(op, target, force) => {
                if op != Op::Yank {
                    self.start_recording(count, body, None);
                }
                self.force = force;
                let done = self.operator(buf, op, target, count, vp, out);
                self.force = None;
                self.finish_recording(done);
            }
            Cmd::Insert(ic) => {
                self.start_recording(count, body, None);
                self.begin_insert(buf, ic, count.unwrap_or(1));
            }
            Cmd::Replace(c) => {
                self.start_recording(count, body, None);
                let done = self.replace(buf, c, count.unwrap_or(1));
                self.finish_recording(done);
            }
            Cmd::Join => {
                self.start_recording(count, body, None);
                let n = count.unwrap_or(2).max(2);
                let cur = buf.cursor();
                let last = buf.line_count() - 1;
                let done = if cur.line + n - 1 > last {
                    if n <= 2 {
                        false
                    } else {
                        join_lines(buf, cur.line, last - cur.line + 1);
                        true
                    }
                } else {
                    join_lines(buf, cur.line, n);
                    true
                };
                self.clamp_normal(buf);
                self.finish_recording(done);
            }
            Cmd::Put { before } => {
                self.start_recording(count, body, None);
                let done = self.put(buf, before, count.unwrap_or(1));
                self.finish_recording(done);
            }
            Cmd::Undo | Cmd::Redo => {
                buf.end_step();
                for _ in repeat_n((), count.unwrap_or(1)) {
                    let c = if cmd == Cmd::Undo {
                        buf.undo()
                    } else {
                        buf.redo()
                    };
                    match c {
                        Some(c) => self.place_after_undo(buf, c),
                        None => break,
                    }
                }
            }
            Cmd::Repeat => self.repeat(buf, count, now, vp, out),
            Cmd::Visual(mode) => {
                if self.mode == mode {
                    self.mode = Mode::Normal;
                    self.clamp_normal(buf);
                } else if self.is_visual() {
                    self.mode = mode;
                } else {
                    self.anchor = buf.cursor();
                    self.mode = mode;
                    if let Some(c) = count {
                        self.visual_count(buf, c);
                        return;
                    }
                }
                self.keep_curswant = true;
            }
            Cmd::VisualSwap => {
                let cursor = buf.cursor();
                buf.set_cursor_raw(self.anchor);
                self.anchor = cursor;
            }
            Cmd::VisualObject(obj) => self.visual_object(buf, obj, count),
            Cmd::VisualOp(op) => {
                let redo = self.visual_redo(buf);
                self.resel = Some(redo);
                if op != Op::Yank {
                    self.start_recording(count, body, Some(redo));
                }
                let done = self.visual_operator_into(buf, op, count, out);
                self.finish_recording(done);
            }
            Cmd::VisualJoin => {
                let redo = self.visual_redo(buf);
                self.resel = Some(redo);
                self.start_recording(count, body, Some(redo));
                let (start, end) = self.visual_bounds(buf);
                let op_start = self.visual_op_start(buf);
                self.mode = Mode::Normal;
                let n = (end.line - start.line + 1).max(2);
                let done = start.line + n - 1 < buf.line_count();
                if done {
                    join_lines(buf, start.line, n);
                } else {
                    buf.set_cursor_raw(op_start);
                }
                self.clamp_normal(buf);
                self.finish_recording(done);
                // Even when it cannot join, the cursor moved to the start.
                self.keep_curswant = false;
            }
            Cmd::VisualPut { before } => {
                let redo = self.visual_redo(buf);
                self.resel = Some(redo);
                // Vim's redo buffer only holds the delete of a visual put.
                self.start_recording(count, vec![Key::Char('d')], Some(redo));
                if before && let Some(change) = &mut self.recording {
                    change.keep_register = true;
                }
                let done = self.visual_put(buf, before, count.unwrap_or(1), out);
                self.finish_recording(done);
            }
        }
    }

    fn apply_want(&mut self, buf: &Buffer, want: Want) {
        match want {
            Want::Update => self.update_curswant(buf),
            Want::Keep => {}
            Want::End => self.curswant = END,
        }
        self.keep_curswant = true;
    }

    // --- Motions ---------------------------------------------------------------------

    fn motion(
        &mut self,
        buf: &Buffer,
        m: Motion,
        count: Option<usize>,
        op: bool,
        vp: usize,
    ) -> Option<MotionResult> {
        let visual = self.is_visual();
        let cur = buf.cursor();
        let n = count.unwrap_or(1);
        let len = buf.line_len(cur.line);
        let last = buf.line_count() - 1;
        let excl = |pos| {
            Some(MotionResult {
                pos,
                kind: Kind::Exclusive,
                want: Want::Update,
                no_adjust: false,
            })
        };
        match m {
            Motion::Left => {
                if cur.col == 0 {
                    return if op { excl(cur) } else { None };
                }
                excl(Pos::new(cur.line, cur.col.saturating_sub(n)))
            }
            Motion::Right => {
                let limit = if op || visual {
                    len
                } else {
                    len.saturating_sub(1)
                };
                if cur.col >= limit {
                    return if op { excl(cur) } else { None };
                }
                excl(Pos::new(cur.line, (cur.col + n).min(limit)))
            }
            Motion::WrapLeft | Motion::WrapRight => {
                let mut w = Walker::new(buf, cur, false);
                let forward = m == Motion::WrapRight;
                let mut moved = false;
                let mut no_adjust = false;
                // `d` and `c` take the line break when Backspace wraps; other
                // operators stop on the last char.
                let keep_eol = matches!(self.pending_op, Some(Op::Delete | Op::Change));
                for _ in repeat_n((), n) {
                    let before = w.pos;
                    let r = if forward { w.inc() } else { w.dec() };
                    if r == -1 {
                        w.set(before);
                        break;
                    }
                    let at_eol = w.get().is_none() && !w.line_empty();
                    if at_eol && (!(op || visual) || (op && !forward && !keep_eol)) {
                        // Step over the line end.
                        let r = if forward { w.inc() } else { w.dec() };
                        if r == -1 {
                            w.set(before);
                            break;
                        }
                    }
                    // Only a wrap that ends the motion on the line end counts.
                    no_adjust = at_eol && op && !forward && keep_eol;
                    moved = true;
                }
                if !moved && !op {
                    return None;
                }
                Some(MotionResult {
                    pos: w.pos,
                    kind: Kind::Exclusive,
                    want: Want::Update,
                    no_adjust,
                })
            }
            Motion::Down | Motion::Up | Motion::NextLine => {
                let line = if m == Motion::Up {
                    if cur.line == 0 {
                        return None;
                    }
                    cur.line.saturating_sub(n)
                } else {
                    if cur.line >= last {
                        return None;
                    }
                    (cur.line + n).min(last)
                };
                let chars = buf.chars(line);
                if m == Motion::NextLine {
                    return Some(MotionResult {
                        pos: Pos::new(line, first_non_blank(&chars)),
                        kind: Kind::Linewise,
                        want: Want::Update,
                        no_adjust: false,
                    });
                }
                Some(MotionResult {
                    pos: Pos::new(line, coladvance(&chars, self.curswant, visual)),
                    kind: Kind::Linewise,
                    want: Want::Keep,
                    no_adjust: false,
                })
            }
            Motion::WordFwd(big) => {
                let mut w = Walker::new(buf, cur, big);
                fwd_word(&mut w, n, op);
                Some(self.adjust_cursor(buf, cur, w.pos, op, Kind::Exclusive))
            }
            Motion::WordBack(big) => {
                let mut w = Walker::new(buf, cur, big);
                if !bck_word(&mut w, n, false) && op {
                    return None;
                }
                excl(w.pos)
            }
            Motion::WordEnd(big) => {
                let mut w = Walker::new(buf, cur, big);
                end_word(&mut w, n, false, false);
                Some(self.adjust_cursor(buf, cur, w.pos, op, Kind::Inclusive))
            }
            Motion::LineStart => excl(Pos::new(cur.line, 0)),
            Motion::Home => {
                self.curswant = 0;
                Some(MotionResult {
                    pos: Pos::new(cur.line, 0),
                    kind: Kind::Exclusive,
                    want: Want::Keep,
                    no_adjust: false,
                })
            }
            Motion::FirstNonBlank => {
                excl(Pos::new(cur.line, first_non_blank(&buf.chars(cur.line))))
            }
            Motion::LineEnd => {
                // Vim sets the column before it tries to move down.
                self.curswant = END;
                let line = if n > 1 {
                    if cur.line >= last {
                        return None;
                    }
                    (cur.line + n - 1).min(last)
                } else {
                    cur.line
                };
                let len = buf.line_len(line);
                let col = if visual { len } else { len.saturating_sub(1) };
                Some(MotionResult {
                    pos: Pos::new(line, col),
                    kind: Kind::Inclusive,
                    want: Want::End,
                    no_adjust: false,
                })
            }
            Motion::FirstLine | Motion::LastLine => {
                let line = match count {
                    Some(c) => (c - 1).min(last),
                    None if m == Motion::FirstLine => 0,
                    None => last,
                };
                let chars = buf.chars(line);
                Some(MotionResult {
                    pos: Pos::new(line, coladvance(&chars, self.curswant, visual)),
                    kind: Kind::Linewise,
                    want: Want::Keep,
                    no_adjust: false,
                })
            }
            Motion::HalfDown | Motion::HalfUp => {
                if op {
                    return None;
                }
                if let Some(c) = count {
                    self.scroll = Some(c.min(vp.max(1)));
                }
                let amount = self.scroll.unwrap_or(vp / 2).max(1);
                let line = if m == Motion::HalfDown {
                    (cur.line + amount).min(last)
                } else {
                    cur.line.saturating_sub(amount)
                };
                let chars = buf.chars(line);
                let col = coladvance(&chars, self.curswant, visual);
                // Neovim fails only when neither the line nor the column changes.
                if line == cur.line && col == cur.col {
                    return None;
                }
                Some(MotionResult {
                    pos: Pos::new(line, col),
                    kind: Kind::Linewise,
                    want: Want::Keep,
                    no_adjust: false,
                })
            }
            Motion::Find { forward, till, ch } => {
                if !self.replaying {
                    self.find = Some((forward, till, ch));
                }
                self.find_motion(buf, forward, till, ch, n, false)
            }
            Motion::RepeatFind { reverse } => {
                let (forward, till, ch) = self.find?;
                let forward = forward != reverse;
                self.find_motion(buf, forward, till, ch, n, till && n == 1)
            }
            Motion::SearchNext { reverse } => {
                let pattern = self.search.clone()?;
                let forward = self.search_forward != reverse;
                let mut pos = cur;
                for _ in repeat_n((), n) {
                    pos = find_pattern(buf, pos, &pattern, forward)?;
                }
                excl(pos)
            }
        }
    }

    fn find_motion(
        &self,
        buf: &Buffer,
        forward: bool,
        till: bool,
        ch: char,
        n: usize,
        skip_adjacent: bool,
    ) -> Option<MotionResult> {
        let cur = buf.cursor();
        let chars = buf.chars(cur.line);
        let col = find_char(&chars, cur.col, ch, forward, till, n, skip_adjacent)?;
        Some(MotionResult {
            pos: Pos::new(cur.line, col),
            kind: if forward {
                Kind::Inclusive
            } else {
                Kind::Exclusive
            },
            want: Want::Update,
            no_adjust: false,
        })
    }

    /// Vim's `adjust_cursor()` after a forward word motion: off the line end,
    /// making an operator inclusive.
    fn adjust_cursor(
        &self,
        buf: &Buffer,
        start: Pos,
        pos: Pos,
        op: bool,
        kind: Kind,
    ) -> MotionResult {
        let mut pos = pos;
        let mut kind = kind;
        if start < pos && pos.col > 0 && pos.col >= buf.line_len(pos.line) && !self.is_visual() {
            pos.col -= 1;
            if op {
                kind = Kind::Inclusive;
            }
        }
        MotionResult {
            pos,
            kind,
            want: Want::Update,
            no_adjust: false,
        }
    }

    // --- Operators -------------------------------------------------------------------

    fn operator(
        &mut self,
        buf: &mut Buffer,
        op: Op,
        target: Target,
        count: Option<usize>,
        vp: usize,
        out: &mut Outcome,
    ) -> bool {
        let cur = buf.cursor();
        let n = count.unwrap_or(1);
        // Vim's `old_col`: the remembered column before the motion moved anything.
        self.old_col = self.curswant;
        self.pending_op = Some(op);
        match target {
            Target::Lines => {
                let last = buf.line_count() - 1;
                if n > 1 && cur.line >= last {
                    return false;
                }
                let end = (cur.line + n - 1).min(last);
                self.linewise(buf, op, cur.line, end, false, cur, 1, out);
                true
            }
            Target::Object(obj) => {
                let range = match obj {
                    Object::Word { big, include } => {
                        match current_word(buf, cur, n, include, big) {
                            Ok(range) => Some(range),
                            Err(pos) => {
                                buf.set_cursor_raw(pos);
                                self.clamp_normal(buf);
                                return false;
                            }
                        }
                    }
                    Object::Quote { ch, include } => current_quote(buf, cur, n, include, ch),
                    Object::Block {
                        open,
                        close,
                        include,
                    } => current_block(buf, cur, n, include, open, close),
                };
                let Some(ObjectRange {
                    start,
                    end,
                    inclusive,
                }) = range
                else {
                    return false;
                };
                self.charwise_motion(buf, op, (start, end), inclusive, false, out);
                true
            }
            Target::Motion(m) => {
                let on_word = buf.char_at(cur).is_some_and(|c| !is_blank(c));
                let r = match m {
                    Motion::WordFwd(big) if op == Op::Change && on_word => {
                        // `cw` works like `ce`, but stays on the current word's end.
                        let mut w = Walker::new(buf, cur, big);
                        end_word(&mut w, n, true, false);
                        let mut r = self.adjust_cursor(buf, cur, w.pos, true, Kind::Inclusive);
                        r.kind = Kind::Inclusive;
                        r
                    }
                    _ => match self.motion(buf, m, count, true, vp) {
                        Some(r) => r,
                        None => return false,
                    },
                };
                let mut r = r;
                match self.force {
                    Some('V') => r.kind = Kind::Linewise,
                    Some(_) => {
                        r.kind = match r.kind {
                            Kind::Linewise | Kind::Inclusive => Kind::Exclusive,
                            Kind::Exclusive => Kind::Inclusive,
                        };
                    }
                    None => {}
                }
                let (start, end) = if r.pos < cur {
                    (r.pos, cur)
                } else {
                    (cur, r.pos)
                };
                if r.kind == Kind::Linewise {
                    self.linewise(buf, op, start.line, end.line, false, start, 1, out);
                } else {
                    self.charwise_motion(
                        buf,
                        op,
                        (start, end),
                        r.kind == Kind::Inclusive,
                        r.no_adjust,
                        out,
                    );
                }
                true
            }
        }
    }

    /// A charwise operator range from a motion or text object (not visual mode),
    /// with Vim's exclusive-to-linewise rules (`:help exclusive-linewise`).
    fn charwise_motion(
        &mut self,
        buf: &mut Buffer,
        op: Op,
        (start, end): (Pos, Pos),
        inclusive: bool,
        no_adjust: bool,
        out: &mut Outcome,
    ) {
        let mut end = end;
        let mut inclusive = inclusive;
        let start_chars = buf.chars(start.line);
        if !inclusive && !no_adjust && end.col == 0 && end.line > start.line {
            end.line -= 1;
            if in_indent(&start_chars, start.col) {
                self.linewise(buf, op, start.line, end.line, true, start, 1, out);
                return;
            }
            end.col = buf.line_len(end.line);
            if end.col > 0 {
                end.col -= 1;
                inclusive = true;
            }
        }
        if matches!(op, Op::ShiftLeft | Op::ShiftRight) {
            // A charwise motion: Vim puts the cursor at the column the motion left
            // remembered (only `$` changes it), not at the old one.
            self.old_col = self.curswant;
            self.linewise(buf, op, start.line, end.line, false, start, 1, out);
            return;
        }
        if op == Op::Delete && end.line > start.line && self.force.is_none() {
            let chars = buf.chars(end.line);
            let mut after = end.col;
            if after < chars.len() && inclusive {
                after += 1;
            }
            let rest_blank = chars
                .get(after..)
                .unwrap_or_default()
                .iter()
                .all(|&c| is_blank(c));
            if rest_blank && in_indent(&start_chars, start.col) {
                self.linewise(buf, op, start.line, end.line, false, start, 1, out);
                return;
            }
        }
        let len = buf.line_len(end.line);
        let end = if inclusive && end.col < len {
            Pos::new(end.line, end.col + 1)
        } else {
            end
        };
        // Vim's `oap->empty` is false for an inclusive range, even on an empty
        // line, and `c` then yanks the empty text.
        if op == Op::Change && inclusive && start == end && !is_empty(buf) {
            self.fill_register(
                Register {
                    text: String::new(),
                    linewise: false,
                },
                out,
            );
        }
        self.charwise(buf, op, start, end, out);
    }

    /// Runs `op` on the text in `start..end` (`end` excluded).
    fn charwise(&mut self, buf: &mut Buffer, op: Op, start: Pos, end: Pos, out: &mut Outcome) {
        match op {
            Op::Yank => {
                let text = buf.slice(start, end);
                self.fill_register(
                    Register {
                        text,
                        linewise: false,
                    },
                    out,
                );
                buf.set_cursor_raw(start);
                self.clamp_normal(buf);
            }
            Op::Delete | Op::Change => {
                if start < end && !is_empty(buf) {
                    let text = buf.slice(start, end);
                    self.fill_register(
                        Register {
                            text,
                            linewise: false,
                        },
                        out,
                    );
                    buf.set_cursor_raw(start);
                    replace(buf, start, end, "");
                }
                buf.set_cursor_raw(start);
                if start >= end && !is_empty(buf) {
                    // Vim saves the line for undo even when there is nothing to delete.
                    buf.save_lines(start.line..start.line + 1);
                }
                if op == Op::Change {
                    self.start_insert(buf, Repeat::None, 1, false);
                } else {
                    self.clamp_normal(buf);
                }
            }
            Op::ShiftLeft | Op::ShiftRight => {
                let end_line = if end.col == 0 && end.line > start.line {
                    end.line - 1
                } else {
                    end.line
                };
                self.linewise(buf, op, start.line, end_line, false, start, 1, out);
            }
        }
    }

    /// Runs `op` on lines `first..=last`. `end_adjusted` is set when an exclusive
    /// motion became linewise; `yank_at` is where a yank leaves the cursor.
    #[allow(clippy::too_many_arguments)]
    fn linewise(
        &mut self,
        buf: &mut Buffer,
        op: Op,
        first: usize,
        last: usize,
        end_adjusted: bool,
        yank_at: Pos,
        amount: usize,
        out: &mut Outcome,
    ) {
        let old_col = self.old_col;
        let mut text = String::new();
        for line in first..=last {
            text.push_str(buf.line(line));
            text.push('\n');
        }
        let register = Register {
            text,
            linewise: true,
        };
        if op != Op::Yank {
            buf.set_cursor_raw(Pos::new(
                yank_at.line,
                yank_at.col.min(buf.line_len(yank_at.line)),
            ));
        }
        match op {
            Op::Yank => {
                self.fill_register(register, out);
                buf.set_cursor_raw(yank_at);
                self.clamp_normal(buf);
            }
            Op::Delete => {
                if !is_empty(buf) {
                    self.fill_register(register, out);
                }
                buf.splice_lines(first..last + 1, Vec::new());
                let line = first.min(buf.line_count() - 1);
                let chars = buf.chars(line);
                if end_adjusted {
                    buf.set_cursor_raw(Pos::new(line, first_non_blank(&chars)));
                } else {
                    buf.set_cursor_raw(Pos::new(line, coladvance(&chars, old_col, false)));
                }
            }
            Op::Change => {
                if !is_empty(buf) {
                    self.fill_register(register, out);
                }
                let indent = indent_of(buf.line(first));
                let col = indent.chars().count();
                // Where undo puts the cursor back (found against Neovim): the line
                // below when lines are deleted first, otherwise no further right
                // than the indent.
                let at = if last > first {
                    Pos::new(first + 1, buf.cursor().col)
                } else {
                    Pos::new(first, buf.cursor().col.min(col))
                };
                buf.set_cursor_raw(at);
                buf.splice_lines(first..last + 1, vec![indent]);
                buf.set_cursor_raw(Pos::new(first, col));
                self.start_insert(buf, Repeat::None, 1, true);
            }
            Op::ShiftLeft | Op::ShiftRight => {
                let right = op == Op::ShiftRight;
                buf.save_lines(first..last + 1);
                let lines: Vec<String> = (first..=last)
                    .map(|l| shift_line(buf.line(l), right, amount))
                    .collect();
                if (first..=last)
                    .zip(&lines)
                    .any(|(l, new)| buf.line(l) != new)
                {
                    buf.splice_lines(first..last + 1, lines);
                }
                let chars = buf.chars(first);
                buf.set_cursor_raw(Pos::new(first, coladvance(&chars, old_col, false)));
            }
        }
    }

    fn replace(&mut self, buf: &mut Buffer, c: char, n: usize) -> bool {
        let cur = buf.cursor();
        let mut chars = buf.chars(cur.line);
        if cur.col + n > chars.len() {
            return false;
        }
        for ch in &mut chars[cur.col..cur.col + n] {
            *ch = c;
        }
        buf.splice_lines(cur.line..cur.line + 1, vec![chars.into_iter().collect()]);
        buf.set_cursor_raw(Pos::new(cur.line, cur.col + n - 1));
        true
    }

    fn put(&mut self, buf: &mut Buffer, before: bool, n: usize) -> bool {
        let Some(reg) = self.register.clone() else {
            return false;
        };
        let cur = buf.cursor();
        if reg.linewise {
            let at = if before { cur.line } else { cur.line + 1 };
            put_lines(buf, at, &reg.text, n);
            return true;
        }
        if reg.text.is_empty() {
            return false;
        }
        let len = buf.line_len(cur.line);
        let col = if before || len == 0 {
            cur.col
        } else {
            cur.col + 1
        };
        let text = reg.text.repeat(n);
        let at = Pos::new(cur.line, col);
        replace(buf, at, at, &text);
        if text.contains('\n') {
            buf.set_cursor_raw(at);
        } else {
            buf.set_cursor_raw(Pos::new(cur.line, col + text.chars().count() - 1));
        }
        self.clamp_normal(buf);
        true
    }

    // --- Visual mode -----------------------------------------------------------------

    fn visual_bounds(&self, buf: &Buffer) -> (Pos, Pos) {
        let cursor = buf.cursor();
        if self.anchor <= cursor {
            (self.anchor, cursor)
        } else {
            (cursor, self.anchor)
        }
    }

    /// `{count}v` and `{count}V`: the size of the last visual operation times the
    /// count, or `count` chars or lines when there was none (Vim's `nv_visual()`).
    fn visual_count(&mut self, buf: &mut Buffer, count: usize) {
        let cur = buf.cursor();
        let last = buf.line_count() - 1;
        let Some(r) = self.resel else {
            let pos = if self.mode == Mode::Visual {
                Pos::new(cur.line, (cur.col + count - 1).min(buf.line_len(cur.line)))
            } else {
                Pos::new((cur.line + count - 1).min(last), cur.col)
            };
            buf.set_cursor_raw(pos);
            return;
        };
        self.mode = r.mode;
        let mut line = cur.line;
        if r.mode != Mode::Visual || r.lines > 1 {
            line = (cur.line + r.lines * count - 1).min(last);
        }
        let len = buf.line_len(line);
        let col = if r.to_end {
            len
        } else if r.mode == Mode::Visual {
            if r.lines <= 1 {
                cur.col + r.cols * count - 1
            } else {
                r.cols
            }
        } else {
            cur.col
        };
        buf.set_cursor_raw(Pos::new(line, col.min(len)));
        if r.to_end {
            self.curswant = END;
            self.keep_curswant = true;
        }
    }

    /// Where an operator on the visual area starts: also where a yank leaves the
    /// cursor and where undo puts it back. In linewise mode that is column 0 unless
    /// the cursor is above the line where the selection was started.
    fn visual_op_start(&self, buf: &Buffer) -> Pos {
        self.visual_op_start_in(buf, self.mode == Mode::VisualLine)
    }

    fn visual_op_start_in(&self, buf: &Buffer, linewise: bool) -> Pos {
        let (start, _) = self.visual_bounds(buf);
        if linewise && buf.cursor().line >= self.anchor.line {
            Pos::new(start.line, 0)
        } else {
            start
        }
    }

    fn visual_redo(&self, buf: &Buffer) -> VisualRedo {
        let (start, end) = self.visual_bounds(buf);
        let lines = end.line - start.line + 1;
        VisualRedo {
            mode: self.mode,
            lines,
            cols: if lines == 1 {
                end.col - start.col + 1
            } else {
                end.col
            },
            to_end: self.curswant == END,
        }
    }

    fn visual_operator_into(
        &mut self,
        buf: &mut Buffer,
        op: Op,
        count: Option<usize>,
        out: &mut Outcome,
    ) -> bool {
        let (_, end) = self.visual_bounds(buf);
        let linewise = self.mode == Mode::VisualLine;
        self.mode = Mode::Normal;
        self.old_col = self.curswant_override.unwrap_or(self.curswant);
        let amount = count.unwrap_or(1);
        let start = self.visual_op_start_in(buf, linewise);
        buf.set_cursor_raw(start);
        if linewise || matches!(op, Op::ShiftLeft | Op::ShiftRight) {
            self.linewise(buf, op, start.line, end.line, false, start, amount, out);
            return true;
        }
        let len = buf.line_len(end.line);
        let end = if end.col >= len {
            if end.line + 1 < buf.line_count() {
                Pos::new(end.line + 1, 0)
            } else {
                Pos::new(end.line, len)
            }
        } else {
            Pos::new(end.line, end.col + 1)
        };
        self.charwise(buf, op, start, end, out);
        true
    }

    /// The delete before a visual put; Vim runs it with an old column of 0.
    fn visual_delete_for_put(&mut self, buf: &mut Buffer, out: &mut Outcome) {
        self.curswant_override = Some(0);
        self.visual_operator_into(buf, Op::Delete, None, out);
        self.curswant_override = None;
    }

    fn visual_put(&mut self, buf: &mut Buffer, before: bool, n: usize, out: &mut Outcome) -> bool {
        let Some(reg) = self.register.clone() else {
            // Vim deletes the selection before it finds the register empty.
            let mut deleted = Outcome::default();
            self.visual_delete_for_put(buf, &mut deleted);
            if !before && let Some(d) = deleted.yanked {
                self.fill_register(d, out);
            } else {
                self.register = None;
            }
            return true;
        };
        let vmode = self.mode;
        let (start, _) = self.visual_bounds(buf);
        let mut deleted = Outcome::default();
        self.visual_delete_for_put(buf, &mut deleted);
        let whole = buf.no_lines();
        let cur = buf.cursor();
        let forward = if vmode == Mode::VisualLine {
            cur.line < start.line
        } else {
            cur.col < start.col
        };
        if vmode == Mode::VisualLine {
            let at = if forward { cur.line + 1 } else { cur.line };
            // A charwise register goes in as lines, one per line of its text.
            if reg.linewise {
                put_lines(buf, at, &reg.text, n);
            } else {
                put_lines(buf, at, &format!("{}\n", reg.text), n);
            }
        } else if reg.linewise {
            // Split the line at the cursor and put the lines in between.
            let chars = buf.chars(cur.line);
            let mut split = cur.col;
            if forward && split < chars.len() {
                split += 1;
            }
            let head: String = chars[..split.min(chars.len())].iter().collect();
            let tail: String = chars[split.min(chars.len())..].iter().collect();
            buf.splice_lines(cur.line..cur.line + 1, vec![head, tail]);
            put_lines(buf, cur.line + 1, &reg.text, n);
        } else {
            self.register = Some(reg.clone());
            self.put(buf, !forward, n);
        }
        if whole {
            let last = buf.line_count() - 1;
            if last > 0 && buf.line(last).is_empty() {
                buf.splice_lines(last..last + 1, Vec::new());
            }
        }
        if before {
            // `P` keeps the register.
            self.register = Some(reg);
        } else if let Some(d) = deleted.yanked {
            self.fill_register(d, out);
        }
        true
    }

    fn visual_object(&mut self, buf: &mut Buffer, obj: Object, count: Option<usize>) {
        let cur = buf.cursor();
        // A failed (or unsupported) text object leaves the remembered column alone.
        self.keep_curswant = true;
        if self.anchor != cur {
            return;
        }
        let n = count.unwrap_or(1);
        let range = match obj {
            Object::Word { big, include } => current_word(buf, cur, n, include, big).ok(),
            Object::Quote { ch, include } => current_quote(buf, cur, n, include, ch),
            Object::Block {
                open,
                close,
                include,
            } => current_block(buf, cur, n, include, open, close),
        };
        let Some(range) = range else { return };
        self.keep_curswant = false;
        let mut end = range.end;
        if !range.inclusive {
            let mut w = Walker::new(buf, end, false);
            w.dec();
            end = w.pos;
        }
        self.anchor = range.start;
        buf.set_cursor_raw(end);
        if self.mode == Mode::VisualLine {
            self.mode = Mode::Visual;
        }
    }

    // --- Repeat ----------------------------------------------------------------------

    fn repeat(
        &mut self,
        buf: &mut Buffer,
        count: Option<usize>,
        now: Instant,
        vp: usize,
        out: &mut Outcome,
    ) {
        let Some(mut change) = self.last_change.clone() else {
            return;
        };
        if count.is_some() && change.visual.is_none() {
            change.count = count;
            if let Some(last) = &mut self.last_change {
                last.count = count;
            }
        }
        self.replaying = true;
        if let Some(v) = change.visual {
            let cur = buf.cursor();
            self.anchor = cur;
            let line = (cur.line + v.lines - 1).min(buf.line_count() - 1);
            let col = if v.to_end {
                buf.line_len(line)
            } else if v.lines == 1 {
                cur.col + v.cols - 1
            } else {
                v.cols
            };
            let col = col.min(buf.line_len(line));
            buf.set_cursor_raw(Pos::new(line, col));
            self.mode = v.mode;
            if v.to_end {
                self.curswant = END;
            }
        }
        let mut keys = Vec::new();
        if change.visual.is_none()
            && let Some(c) = change.count
        {
            keys.extend(c.to_string().chars().map(Key::Char));
        }
        keys.extend(change.keys.iter().copied());
        self.failed = false;
        let register = self.register.clone();
        let mut inner = Outcome::default();
        for key in keys {
            self.feed(buf, key, now, vp, &mut inner);
            if self.failed {
                break;
            }
        }
        if change.keep_register {
            self.register = register;
        } else if inner.yanked.is_some() {
            out.yanked = inner.yanked;
        }
        out.request = out.request.or(inner.request);
        self.keys.clear();
        self.replaying = false;
    }

    // --- Insert mode -----------------------------------------------------------------

    fn begin_insert(&mut self, buf: &mut Buffer, ic: InsertCmd, count: usize) {
        let cur = buf.cursor();
        let len = buf.line_len(cur.line);
        let mut repeat = Repeat::Text;
        let mut did_ai = false;
        match ic {
            InsertCmd::Before => {}
            InsertCmd::After => {
                if len > 0 {
                    buf.set_cursor_raw(Pos::new(cur.line, cur.col + 1));
                }
            }
            InsertCmd::LineStart => {
                let chars = buf.chars(cur.line);
                let col = chars.iter().take_while(|&&c| is_blank(c)).count();
                buf.set_cursor_raw(Pos::new(cur.line, col));
            }
            InsertCmd::LineEnd => buf.set_cursor_raw(Pos::new(cur.line, len)),
            InsertCmd::OpenBelow | InsertCmd::OpenAbove => {
                let indent = make_indent(indent_width(&indent_of(buf.line(cur.line))));
                let col = indent.chars().count();
                let at = if ic == InsertCmd::OpenBelow {
                    cur.line + 1
                } else {
                    cur.line
                };
                buf.splice_lines(at..at, vec![indent]);
                buf.set_cursor_raw(Pos::new(at, col));
                repeat = Repeat::Lines;
                did_ai = true;
            }
        }
        self.start_insert(buf, repeat, count, did_ai);
        // `o` and `O` start the insert at the line start (Vim's `startln`).
        if repeat == Repeat::Lines
            && let Some(ins) = &mut self.insert
        {
            ins.start.col = 0;
            ins.orig.col = 0;
        }
    }

    fn start_insert(&mut self, buf: &Buffer, repeat: Repeat, count: usize, did_ai: bool) {
        self.mode = Mode::Insert;
        self.insert = Some(InsertState {
            start: buf.cursor(),
            orig: buf.cursor(),
            update_orig: true,
            need_undo: true,
            count: count.max(1),
            repeat,
            did_ai,
            arrow_used: false,
            typed: Vec::new(),
        });
    }

    fn insert_key(&mut self, buf: &mut Buffer, key: Key, now: Instant) {
        if !self.replaying && self.escape_filter(buf, key, now) {
            return;
        }
        self.insert_key_raw(buf, key, true);
    }

    /// Holds the first keys of `insert_escape`; returns true when `key` was consumed.
    fn escape_filter(&mut self, buf: &mut Buffer, key: Key, now: Instant) -> bool {
        let Some(seq) = self.config.insert_escape.as_ref() else {
            return false;
        };
        let seq: Vec<char> = seq.chars().collect();
        if seq.is_empty() {
            return false;
        }
        if let Some(at) = self.held_at
            && now.duration_since(at) > ESCAPE_TIMEOUT
        {
            self.flush_held(buf);
        }
        let Key::Char(c) = key else {
            if !self.held.is_empty() {
                self.flush_held(buf);
            }
            return false;
        };
        if seq.get(self.held.len()) != Some(&c) {
            if self.held.is_empty() {
                return false;
            }
            self.flush_held(buf);
            if seq[0] != c {
                return false;
            }
        }
        self.held.push(c);
        self.held_at = Some(now);
        if self.held.len() == seq.len() {
            self.held.clear();
            self.held_at = None;
            self.insert_key_raw(buf, Key::Esc, true);
        }
        true
    }

    fn flush_held(&mut self, buf: &mut Buffer) {
        let held = std::mem::take(&mut self.held);
        self.held_at = None;
        for c in held {
            self.insert_key_raw(buf, Key::Char(c), true);
        }
    }

    fn insert_key_raw(&mut self, buf: &mut Buffer, key: Key, record: bool) {
        let insert_only = !matches!(key, Key::Up | Key::Down);
        let rev = buf.revision();
        match key {
            Key::Esc | Key::Ctrl('[') | Key::Ctrl('c') => {
                self.leave_insert(buf);
                return;
            }
            Key::Char(c) => {
                self.before_edit(buf);
                self.type_char(buf, c);
            }
            Key::Tab | Key::Ctrl('i') => {
                self.before_edit(buf);
                self.type_char(buf, '\t');
            }
            Key::Enter | Key::Ctrl('j') | Key::Ctrl('m') => {
                self.before_edit(buf);
                self.newline(buf);
            }
            Key::Backspace | Key::Ctrl('h') => {
                self.before_edit(buf);
                self.backspace(buf, Bs::Char);
            }
            Key::Delete => {
                self.before_edit(buf);
                self.delete_forward(buf);
            }
            Key::Ctrl('w') | Key::Ctrl('u') => {
                // Neovim maps both to `<C-G>u` first: a new undo step, and the
                // inserted text starts again here.
                self.before_edit(buf);
                if !self.replaying {
                    buf.end_step();
                }
                if let Some(ins) = &mut self.insert {
                    ins.start = buf.cursor();
                    ins.update_orig = false;
                    ins.need_undo = true;
                }
                let mode = if key == Key::Ctrl('w') {
                    Bs::Word
                } else {
                    Bs::Line
                };
                self.backspace(buf, mode);
            }
            Key::Left | Key::Right | Key::Up | Key::Down | Key::Home | Key::End => {
                self.insert_arrow(buf, key);
                self.follow_start();
                return;
            }
            _ => return,
        }
        let deletes = matches!(key, Key::Backspace | Key::Ctrl('h' | 'w' | 'u'));
        if deletes
            && buf.revision() != rev
            && let Some(ins) = &mut self.insert
        {
            // Vim's ins_bs(): the line is no longer just an autoindent.
            ins.did_ai = false;
        }
        // Like Vim's redo buffer, a backspace that deleted nothing is not kept.
        let record = record && !(deletes && buf.revision() == rev);
        if record {
            if let Some(ins) = &mut self.insert {
                ins.typed.push(key);
            }
            if let Some(change) = &mut self.recording {
                change.keys.push(key);
            }
        }
        if insert_only {
            self.update_curswant(buf);
        }
        if buf.revision() != rev
            && let Some(ins) = &mut self.insert
        {
            ins.need_undo = false;
        }
        self.follow_start();
    }

    /// Vim's loop in `edit()`: `Insstart_orig` follows `Insstart` after each key.
    fn follow_start(&mut self) {
        if let Some(ins) = &mut self.insert
            && ins.update_orig
        {
            ins.orig = ins.start;
        }
    }

    /// Vim's `stop_arrow()`: typing after an arrow key starts a new insert.
    fn before_edit(&mut self, buf: &Buffer) {
        let Some(ins) = &mut self.insert else { return };
        if ins.arrow_used {
            ins.arrow_used = false;
            // A new insert starts here; the original start only follows it to the left.
            ins.start = buf.cursor();
            if ins.start > ins.orig && !ins.need_undo {
                ins.update_orig = false;
            }
            ins.typed.clear();
            ins.repeat = Repeat::Text;
            if !self.replaying {
                self.recording = Some(Change {
                    count: None,
                    keys: vec![Key::Char('i')],
                    visual: None,
                    keep_register: false,
                });
            }
        }
    }

    fn type_char(&mut self, buf: &mut Buffer, c: char) {
        let cur = buf.cursor();
        replace(buf, cur, cur, &c.to_string());
        buf.set_cursor_raw(Pos::new(cur.line, cur.col + 1));
        if let Some(ins) = &mut self.insert {
            ins.did_ai = false;
        }
    }

    /// Enter in insert mode, with `autoindent` (Vim's `open_line()`).
    fn newline(&mut self, buf: &mut Buffer) {
        let cur = buf.cursor();
        let chars = buf.chars(cur.line);
        let split = cur.col.min(chars.len());
        let mut head: String = chars[..split].iter().collect();
        let tail: String = chars[split..].iter().collect();
        let width = indent_width(&indent_of(&head));
        let did_ai = self.insert.as_ref().is_some_and(|i| i.did_ai);
        if did_ai {
            head.truncate(head.trim_end_matches(is_blank).len());
        }
        let tail = tail.trim_start_matches(is_blank).to_owned();
        let indent = make_indent(width);
        let col = indent.chars().count();
        buf.splice_lines(cur.line..cur.line + 1, vec![head, indent + &tail]);
        buf.set_cursor_raw(Pos::new(cur.line + 1, col));
        if let Some(ins) = &mut self.insert {
            ins.did_ai = true;
        }
    }

    fn backspace(&mut self, buf: &mut Buffer, mode: Bs) {
        let cur = buf.cursor();
        if cur.col == 0 {
            if cur.line == 0 {
                return;
            }
            let prev = Pos::new(cur.line - 1, buf.line_len(cur.line - 1));
            replace(buf, prev, Pos::new(cur.line, 0), "");
            buf.set_cursor_raw(prev);
            if let Some(ins) = &mut self.insert
                && ins.start.line == cur.line
            {
                ins.start = prev;
            }
            return;
        }
        let chars = buf.chars(cur.line);
        let col = cur.col.min(chars.len());
        let from = match mode {
            Bs::Char => {
                if in_indent(&chars, col) && is_blank(chars[col - 1]) {
                    // 'smarttab': back to the previous multiple of 'shiftwidth'.
                    let vcol = super::text::vcol_start(&chars, col);
                    let want = ((vcol - 1) / SHIFTWIDTH) * SHIFTWIDTH;
                    let mut c = col;
                    while c > 0
                        && super::text::vcol_start(&chars, c) > want
                        && is_blank(chars[c - 1])
                    {
                        c -= 1;
                    }
                    let v = super::text::vcol_start(&chars, c);
                    let fill = " ".repeat(want.saturating_sub(v));
                    replace(buf, Pos::new(cur.line, c), cur, &fill);
                    let pos = Pos::new(cur.line, c + fill.len());
                    buf.set_cursor_raw(pos);
                    return;
                }
                col - 1
            }
            Bs::Word | Bs::Line => {
                let mut mincol = 0;
                if mode == Bs::Line {
                    let first = chars.iter().take_while(|&&c| is_blank(c)).count();
                    if first < col {
                        mincol = first;
                    }
                }
                let orig = self.insert.as_ref().map(|i| i.orig);
                let mut c = col;
                let mut word: Option<u8> = None;
                loop {
                    c -= 1;
                    let ch = chars[c];
                    if mode == Bs::Word {
                        match word {
                            None if !is_blank(ch) => word = Some(class(Some(ch), false)),
                            Some(cls) if is_blank(ch) || class(Some(ch), false) != cls => {
                                c += 1;
                                break;
                            }
                            _ => {}
                        }
                    }
                    let at_orig = orig == Some(Pos::new(cur.line, c));
                    if !(c > mincol && !at_orig) {
                        break;
                    }
                }
                c
            }
        };
        let pos = Pos::new(cur.line, from);
        replace(buf, pos, cur, "");
        buf.set_cursor_raw(pos);
        if let Some(ins) = &mut self.insert
            && ins.orig.line == pos.line
            && pos.col < ins.orig.col
        {
            ins.orig.col = pos.col;
        }
    }

    fn delete_forward(&mut self, buf: &mut Buffer) {
        if let Some(ins) = &mut self.insert {
            ins.did_ai = false;
        }
        let cur = buf.cursor();
        let len = buf.line_len(cur.line);
        if cur.col < len {
            replace(buf, cur, Pos::new(cur.line, cur.col + 1), "");
        } else if cur.line + 1 < buf.line_count() {
            replace(buf, cur, Pos::new(cur.line + 1, 0), "");
        }
        buf.set_cursor_raw(cur);
    }

    fn insert_arrow(&mut self, buf: &mut Buffer, key: Key) {
        let cur = buf.cursor();
        let stuck = match key {
            Key::Left => cur.col == 0,
            Key::Right => cur.col >= buf.line_len(cur.line),
            Key::Up => cur.line == 0,
            Key::Down => cur.line + 1 >= buf.line_count(),
            _ => false,
        };
        if stuck {
            return;
        }
        if !self.replaying {
            buf.end_step();
            // Like Vim's `start_arrow()`: the insert so far is a finished change.
            if let Some(ins) = &self.insert
                && !ins.arrow_used
                && let Some(mut change) = self.recording.take()
            {
                change.keys.push(Key::Esc);
                self.last_change = Some(change);
            }
        }
        let len = buf.line_len(cur.line);
        let last = buf.line_count() - 1;
        let pos = match key {
            Key::Left => Pos::new(cur.line, cur.col.saturating_sub(1)),
            Key::Right => Pos::new(cur.line, (cur.col + 1).min(len)),
            Key::Home => Pos::new(cur.line, 0),
            Key::End => Pos::new(cur.line, len),
            Key::Up | Key::Down => {
                let line = if key == Key::Up {
                    cur.line.saturating_sub(1)
                } else {
                    (cur.line + 1).min(last)
                };
                let chars = buf.chars(line);
                Pos::new(line, coladvance(&chars, self.curswant, true))
            }
            _ => cur,
        };
        let did_ai = self.insert.as_ref().is_some_and(|i| i.did_ai);
        if did_ai && pos.line != cur.line && buf.line(cur.line).chars().all(is_blank) {
            buf.splice_lines(cur.line..cur.line + 1, vec![String::new()]);
        }
        buf.set_cursor_raw(pos);
        if let Some(ins) = &mut self.insert {
            ins.arrow_used = true;
            ins.did_ai = false;
            ins.count = 1;
        }
        match key {
            Key::End => self.curswant = END,
            Key::Up | Key::Down => {}
            _ => self.update_curswant(buf),
        }
    }

    fn leave_insert(&mut self, buf: &mut Buffer) {
        let Some(ins) = self.insert.clone() else {
            self.mode = Mode::Normal;
            return;
        };
        let col_before = buf.cursor().col;
        let mut ai_moved = false;
        if !ins.arrow_used {
            if ins.count > 1 && ins.repeat != Repeat::None {
                let was = self.replaying;
                self.replaying = true;
                for _ in repeat_n((), ins.count - 1) {
                    if ins.repeat == Repeat::Lines {
                        self.newline(buf);
                    }
                    for &key in &ins.typed {
                        self.insert_key_raw(buf, key, false);
                    }
                }
                self.replaying = was;
            }
            let did_ai = self.insert.as_ref().is_some_and(|i| i.did_ai);
            if did_ai {
                self.remove_auto_indent(buf);
            }
            ai_moved = buf.cursor().col != col_before;
            if let Some(mut change) = self.recording.take() {
                change.keys.push(Key::Esc);
                self.last_change = Some(change);
            }
        }
        self.insert = None;
        self.mode = Mode::Normal;
        let cur = buf.cursor();
        if cur.col > 0 {
            buf.set_cursor_raw(Pos::new(cur.line, cur.col - 1));
        }
        self.clamp_normal(buf);
        // When the autoindent was removed and the cursor did not move left, the
        // remembered column stays after the indent, like Vim.
        if !(ai_moved && cur.col == 0) {
            self.update_curswant(buf);
        }
        if !self.replaying {
            buf.end_step();
        }
    }

    /// Vim's `stop_insert()` after an autoindent with nothing typed: drop the
    /// white space before the cursor.
    fn remove_auto_indent(&mut self, buf: &mut Buffer) {
        let cur = buf.cursor();
        let mut chars = buf.chars(cur.line);
        let mut col = cur.col.min(chars.len());
        let before = chars.len();
        let mut last;
        loop {
            if col == chars.len() && col > 0 {
                col -= 1;
            }
            last = chars.get(col).copied();
            match last {
                Some(c) if is_blank(c) => {
                    chars.remove(col);
                }
                _ => break,
            }
        }
        if chars.len() == before {
            return;
        }
        buf.splice_lines(cur.line..cur.line + 1, vec![chars.iter().collect()]);
        // Back on the line end when white space was removed before it.
        if last.is_some() && col + 1 == chars.len() {
            col += 1;
        }
        buf.set_cursor_raw(Pos::new(cur.line, col));
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Bs {
    Char,
    Word,
    Line,
}

// --- Parsing -------------------------------------------------------------------------

fn take_count(keys: &[Key]) -> (Option<usize>, &[Key]) {
    let mut n: Option<usize> = None;
    let mut i = 0;
    while let Some(Key::Char(c)) = keys.get(i) {
        let Some(d) = c.to_digit(10) else { break };
        if d == 0 && n.is_none() {
            break;
        }
        n = Some((n.unwrap_or(0) * 10 + d as usize).min(MAX_COUNT));
        i += 1;
    }
    (n, &keys[i..])
}

/// `keys` without its first key.
fn after_first(keys: &[Key]) -> &[Key] {
    keys.split_first().map_or(keys, |(_, rest)| rest)
}

fn mul_count(a: Option<usize>, b: Option<usize>) -> Option<usize> {
    match (a, b) {
        (None, None) => None,
        (a, b) => Some((a.unwrap_or(1).saturating_mul(b.unwrap_or(1))).min(MAX_COUNT)),
    }
}

enum MotionParse {
    Incomplete,
    Invalid,
    Motion(Motion),
}

fn parse_motion(keys: &[Key]) -> MotionParse {
    use MotionParse::{Incomplete, Invalid};
    let Some(&key) = keys.first() else {
        return Incomplete;
    };
    let m = match key {
        Key::Char('h') | Key::Left => Motion::Left,
        Key::Char('l') | Key::Right => Motion::Right,
        Key::Char(' ') => Motion::WrapRight,
        Key::Backspace => Motion::WrapLeft,
        Key::Char('j') | Key::Down => Motion::Down,
        Key::Char('k') | Key::Up => Motion::Up,
        Key::Char('+') | Key::Enter => Motion::NextLine,
        Key::Char('w') => Motion::WordFwd(false),
        Key::Char('W') => Motion::WordFwd(true),
        Key::Char('b') => Motion::WordBack(false),
        Key::Char('B') => Motion::WordBack(true),
        Key::Char('e') => Motion::WordEnd(false),
        Key::Char('E') => Motion::WordEnd(true),
        Key::Char('0') => Motion::LineStart,
        Key::Home => Motion::Home,
        Key::Char('^') => Motion::FirstNonBlank,
        Key::Char('$') | Key::End => Motion::LineEnd,
        Key::Char('G') => Motion::LastLine,
        Key::Char('g') => match keys.get(1) {
            None => return Incomplete,
            Some(Key::Char('g')) => Motion::FirstLine,
            Some(_) => return Invalid,
        },
        Key::Ctrl('d') => Motion::HalfDown,
        Key::Ctrl('u') => Motion::HalfUp,
        Key::Char(c @ ('f' | 'F' | 't' | 'T')) => {
            let ch = match keys.get(1) {
                None => return Incomplete,
                Some(Key::Char(ch)) => *ch,
                Some(Key::Tab) => '\t',
                Some(_) => return Invalid,
            };
            Motion::Find {
                forward: c == 'f' || c == 't',
                till: c == 't' || c == 'T',
                ch,
            }
        }
        Key::Char(';') => Motion::RepeatFind { reverse: false },
        Key::Char(',') => Motion::RepeatFind { reverse: true },
        Key::Char('n') => Motion::SearchNext { reverse: false },
        Key::Char('N') => Motion::SearchNext { reverse: true },
        _ => return Invalid,
    };
    MotionParse::Motion(m)
}

/// `iw`, `a"`, `i(` and friends; `keys` starts at `i` or `a`.
fn parse_object(keys: &[Key]) -> Option<Option<Object>> {
    let include = keys.first() == Some(&Key::Char('a'));
    let Some(key) = keys.get(1) else {
        return Some(None);
    };
    let block = |open, close| Object::Block {
        open,
        close,
        include,
    };
    let obj = match key {
        Key::Char('w') => Object::Word {
            big: false,
            include,
        },
        Key::Char('W') => Object::Word { big: true, include },
        Key::Char(ch @ ('"' | '\'' | '`')) => Object::Quote { ch: *ch, include },
        Key::Char('(' | ')' | 'b') => block('(', ')'),
        Key::Char('[' | ']') => block('[', ']'),
        Key::Char('{' | '}' | 'B') => block('{', '}'),
        Key::Char('<' | '>') => block('<', '>'),
        _ => return None,
    };
    Some(Some(obj))
}

fn parse(keys: &[Key], visual: bool) -> Parse {
    let (count, rest) = take_count(keys);
    let Some(&key) = rest.first() else {
        return Parse::Incomplete;
    };
    let done = |cmd| Parse::Done {
        count,
        cmd,
        body: rest.to_vec(),
    };
    match key {
        Key::Esc | Key::Ctrl('[') | Key::Ctrl('c') => return done(Cmd::Cancel),
        Key::Char(':') => return done(Cmd::CommandLine),
        Key::Char('/') => return done(Cmd::Search(true)),
        Key::Char('?') => return done(Cmd::Search(false)),
        Key::Char('v') => return done(Cmd::Visual(Mode::Visual)),
        Key::Char('V') => return done(Cmd::Visual(Mode::VisualLine)),
        _ => {}
    }
    if visual {
        let cmd = match key {
            Key::Char('d' | 'x') | Key::Delete => Cmd::VisualOp(Op::Delete),
            Key::Char('y') => Cmd::VisualOp(Op::Yank),
            Key::Char('c' | 's') => Cmd::VisualOp(Op::Change),
            Key::Char('>') => Cmd::VisualOp(Op::ShiftRight),
            Key::Char('<') => Cmd::VisualOp(Op::ShiftLeft),
            Key::Char('J') => Cmd::VisualJoin,
            Key::Char('p') => Cmd::VisualPut { before: false },
            Key::Char('P') => Cmd::VisualPut { before: true },
            Key::Char('o') => Cmd::VisualSwap,
            Key::Char('i' | 'a') => match parse_object(rest) {
                None => return Parse::Invalid,
                Some(None) => return Parse::Incomplete,
                Some(Some(obj)) => Cmd::VisualObject(obj),
            },
            _ => match parse_motion(rest) {
                MotionParse::Incomplete => return Parse::Incomplete,
                MotionParse::Invalid => return Parse::Invalid,
                MotionParse::Motion(m) => Cmd::Motion(m),
            },
        };
        return done(cmd);
    }
    let alias = |op, target| done(Cmd::Operator(op, target, None));
    match key {
        Key::Char('x') | Key::Delete => return alias(Op::Delete, Target::Motion(Motion::Right)),
        Key::Char('X') => return alias(Op::Delete, Target::Motion(Motion::Left)),
        Key::Char('D') => return alias(Op::Delete, Target::Motion(Motion::LineEnd)),
        Key::Char('C') => return alias(Op::Change, Target::Motion(Motion::LineEnd)),
        Key::Char('s') => return alias(Op::Change, Target::Motion(Motion::Right)),
        Key::Char('S') => return alias(Op::Change, Target::Lines),
        // Neovim maps `Y` to `y$` by default.
        Key::Char('Y') => return alias(Op::Yank, Target::Motion(Motion::LineEnd)),
        Key::Char('i') => return done(Cmd::Insert(InsertCmd::Before)),
        Key::Char('a') => return done(Cmd::Insert(InsertCmd::After)),
        Key::Char('I') => return done(Cmd::Insert(InsertCmd::LineStart)),
        Key::Char('A') => return done(Cmd::Insert(InsertCmd::LineEnd)),
        Key::Char('o') => return done(Cmd::Insert(InsertCmd::OpenBelow)),
        Key::Char('O') => return done(Cmd::Insert(InsertCmd::OpenAbove)),
        Key::Char('J') => return done(Cmd::Join),
        Key::Char('p') => return done(Cmd::Put { before: false }),
        Key::Char('P') => return done(Cmd::Put { before: true }),
        Key::Char('u') => return done(Cmd::Undo),
        Key::Ctrl('r') => return done(Cmd::Redo),
        Key::Char('.') => return done(Cmd::Repeat),
        // Window commands (`Ctrl+w` and a key) belong to the app.
        Key::Ctrl('w') => {
            return if rest.len() < 2 {
                Parse::Incomplete
            } else {
                Parse::Invalid
            };
        }
        Key::Char('r') => {
            return match rest.get(1) {
                None => Parse::Incomplete,
                Some(Key::Char(c)) => done(Cmd::Replace(*c)),
                Some(Key::Tab) => done(Cmd::Replace('\t')),
                Some(Key::Esc | Key::Ctrl('[')) => done(Cmd::Cancel),
                Some(_) => Parse::Invalid,
            };
        }
        _ => {}
    }
    let op = match key {
        Key::Char('d') => Op::Delete,
        Key::Char('c') => Op::Change,
        Key::Char('y') => Op::Yank,
        Key::Char('>') => Op::ShiftRight,
        Key::Char('<') => Op::ShiftLeft,
        _ => {
            return match parse_motion(rest) {
                MotionParse::Incomplete => Parse::Incomplete,
                MotionParse::Invalid => Parse::Invalid,
                MotionParse::Motion(m) => done(Cmd::Motion(m)),
            };
        }
    };
    let (count2, mut rest2) = take_count(after_first(rest));
    let Some(&first) = rest2.first() else {
        return Parse::Incomplete;
    };
    let mut body = vec![key];
    let mut count = mul_count(count, count2);
    let mut force = None;
    if first != key
        && let Key::Char(c @ ('v' | 'V')) = first
    {
        force = Some(c);
        body.push(first);
        let (count3, rest3) = take_count(after_first(rest2));
        count = mul_count(count, count3);
        rest2 = rest3;
        if rest2.is_empty() {
            return Parse::Incomplete;
        }
    }
    let key2 = rest2[0];
    body.extend_from_slice(rest2);
    let target = if key2 == key {
        Target::Lines
    } else if matches!(key2, Key::Char('i' | 'a')) {
        match parse_object(rest2) {
            None => return Parse::Invalid,
            Some(None) => return Parse::Incomplete,
            Some(Some(obj)) => Target::Object(obj),
        }
    } else {
        match key2 {
            Key::Esc | Key::Ctrl('[') | Key::Ctrl('c') => return Parse::Invalid,
            _ => match parse_motion(rest2) {
                MotionParse::Incomplete => return Parse::Incomplete,
                MotionParse::Invalid => return Parse::Invalid,
                MotionParse::Motion(m) => Target::Motion(m),
            },
        }
    };
    Parse::Done {
        count,
        cmd: Cmd::Operator(op, target, force),
        body,
    }
}

// --- Text helpers --------------------------------------------------------------------

/// Whether every line was deleted, so Vim has no text to delete (`ML_EMPTY`).
fn is_empty(buf: &Buffer) -> bool {
    buf.no_lines()
}

/// Replaces `start..end` (`(line, len)` is the line break) with `text`.
fn replace(buf: &mut Buffer, start: Pos, end: Pos, text: &str) {
    let first = buf.chars(start.line);
    let last = if end.line == start.line {
        first.clone()
    } else {
        buf.chars(end.line)
    };
    let mut joined: String = first[..start.col.min(first.len())].iter().collect();
    joined.push_str(text);
    joined.extend(last[end.col.min(last.len())..].iter());
    let lines: Vec<String> = joined.split('\n').map(str::to_owned).collect();
    buf.splice_lines(start.line..end.line + 1, lines);
}

/// Puts the lines of a linewise register `n` times before line `at`, cursor on
/// the first non-blank of the first one.
fn put_lines(buf: &mut Buffer, at: usize, text: &str, n: usize) {
    let body = text.strip_suffix('\n').unwrap_or(text);
    let one: Vec<String> = body.split('\n').map(str::to_owned).collect();
    let mut lines = Vec::with_capacity(one.len() * n);
    for _ in repeat_n((), n) {
        lines.extend(one.iter().cloned());
    }
    let at = at.min(buf.line_count());
    buf.splice_lines(at..at, lines);
    let chars = buf.chars(at);
    buf.set_cursor_raw(Pos::new(at, first_non_blank(&chars)));
}

/// Joins `n` lines from `line` like `J`, leaving the cursor at the last join.
fn join_lines(buf: &mut Buffer, line: usize, n: usize) {
    let mut joined = buf.line(line).to_owned();
    let mut col = 0;
    // Vim's `endcurr1`: the last char of the line before, None when it was empty.
    let mut last = joined.chars().last();
    for next_line in (line + 1)..(line + n) {
        let next = buf.line(next_line);
        let stripped = next.trim_start_matches(is_blank);
        let space = !stripped.is_empty()
            && !stripped.starts_with(')')
            && !joined.is_empty()
            && last != Some('\t')
            && last != Some(' ');
        last = stripped.chars().last();
        col = joined.chars().count();
        if space {
            joined.push(' ');
        }
        joined.push_str(stripped);
    }
    buf.splice_lines(line..line + n, vec![joined]);
    buf.set_cursor_raw(Pos::new(line, col));
}

/// Shifts one line by `amount` shift widths; empty lines stay empty.
fn shift_line(line: &str, right: bool, amount: usize) -> String {
    if line.is_empty() {
        return String::new();
    }
    let indent = indent_of(line);
    let width = indent_width(&indent);
    let width = if right {
        width + SHIFTWIDTH * amount
    } else {
        width.saturating_sub(SHIFTWIDTH * amount)
    };
    let mut out = make_indent(width);
    out.push_str(&line[indent.len()..]);
    out
}

/// The next literal match of `pattern` after (`forward`) or before `from`,
/// wrapping around the text.
fn find_pattern(buf: &Buffer, from: Pos, pattern: &str, forward: bool) -> Option<Pos> {
    if pattern.is_empty() {
        return None;
    }
    let count = buf.line_count();
    let matches_in = |line: usize| -> Vec<usize> {
        let text = buf.line(line);
        let mut cols = Vec::new();
        let mut start = 0;
        while let Some(i) = text[start..].find(pattern) {
            let byte = start + i;
            cols.push(text[..byte].chars().count());
            let step = text[byte..].chars().next().map_or(1, char::len_utf8);
            start = byte + step;
        }
        cols
    };
    for (k, ()) in repeat_n((), count + 1).enumerate() {
        let line = if forward {
            (from.line + k) % count
        } else {
            (from.line + count * 2 - k) % count
        };
        let cols = matches_in(line);
        let pick = if forward {
            if k == 0 {
                cols.into_iter().find(|&c| c > from.col)
            } else if k == count {
                cols.into_iter().find(|&c| c <= from.col)
            } else {
                cols.into_iter().next()
            }
        } else if k == 0 {
            cols.into_iter().rev().find(|&c| c < from.col)
        } else if k == count {
            cols.into_iter().rev().find(|&c| c >= from.col)
        } else {
            cols.into_iter().next_back()
        };
        if let Some(col) = pick {
            return Some(Pos::new(line, col));
        }
    }
    None
}
