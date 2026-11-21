//! A note's text as lines, the cursor, and the undo history.

use std::ops::Range;

use super::Pos;

/// A note's text as lines, the cursor, and per-note undo history.
///
/// Every change goes through [`Buffer::splice_lines`], which also records it in
/// the open undo step. The engine closes a step after each whole command, so
/// one undo reverts one command (F-06.3).
#[derive(Debug, Clone)]
pub struct Buffer {
    lines: Vec<String>,
    trailing_newline: bool,
    cursor: Pos,
    revision: u64,
    undo: Vec<UndoEntry>,
    redo: Vec<UndoEntry>,
    step: Option<Step>,
    /// Every line was deleted (Vim's `ML_EMPTY`): there is no text to delete.
    no_lines: bool,
}

/// One undoable command: lines `start..start + new.len()` replaced `old`.
#[derive(Debug, Clone)]
struct UndoEntry {
    start: usize,
    old: Vec<String>,
    new: Vec<String>,
    /// The cursor when the change began (Vim's `uh_cursor`).
    cursor: Pos,
}

/// The undo step being recorded: lines `lo..hi` (current numbering) were `before`.
#[derive(Debug, Clone)]
struct Step {
    lo: usize,
    hi: usize,
    before: Vec<String>,
    cursor: Pos,
    /// Keep the step even if nothing changed.
    forced: bool,
}

/// Where the cursor goes after an undo or redo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UndoCursor {
    /// Exactly here.
    At(Pos),
    /// On this line, at the remembered column (Vim's `curswant`).
    Line(usize),
}

impl Default for Buffer {
    fn default() -> Self {
        Self::from_text("")
    }
}

impl Buffer {
    /// Splits `text` on `'\n'`. An empty text is one empty line, and a final `'\n'`
    /// does not add an empty line; [`Buffer::text`] restores it.
    pub fn from_text(text: &str) -> Self {
        let (body, trailing_newline) = match text.strip_suffix('\n') {
            Some(body) => (body, true),
            None => (text, false),
        };
        Self {
            lines: body.split('\n').map(str::to_owned).collect(),
            trailing_newline,
            cursor: Pos::default(),
            revision: 0,
            undo: Vec::new(),
            redo: Vec::new(),
            step: None,
            no_lines: false,
        }
    }

    /// The text, lines joined with `'\n'`, with the final newline if the source had one.
    pub fn text(&self) -> String {
        let mut text = self.lines.join("\n");
        if self.trailing_newline {
            text.push('\n');
        }
        text
    }

    /// The lines, without line breaks. Never empty.
    pub fn lines(&self) -> &[String] {
        &self.lines
    }

    /// Whether the source text ended with a newline.
    pub fn trailing_newline(&self) -> bool {
        self.trailing_newline
    }

    /// The cursor. `col` is a char index and may equal the line length (insert mode).
    pub fn cursor(&self) -> Pos {
        self.cursor
    }

    /// Moves the cursor, clamped to the text.
    pub fn set_cursor(&mut self, pos: Pos) {
        let line = pos.line.min(self.lines.len() - 1);
        let col = pos.col.min(self.line_len(line));
        self.cursor = Pos { line, col };
    }

    /// Increases on every change to the text, undo and redo included.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Whether there is anything to undo.
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty() || self.step.is_some()
    }

    /// Whether there is anything to redo.
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    // --- Reading ---------------------------------------------------------------------

    pub(crate) fn line_count(&self) -> usize {
        self.lines.len()
    }

    pub(crate) fn line(&self, line: usize) -> &str {
        self.lines.get(line).map_or("", String::as_str)
    }

    pub(crate) fn line_len(&self, line: usize) -> usize {
        self.line(line).chars().count()
    }

    pub(crate) fn chars(&self, line: usize) -> Vec<char> {
        self.line(line).chars().collect()
    }

    pub(crate) fn char_at(&self, pos: Pos) -> Option<char> {
        self.line(pos.line).chars().nth(pos.col)
    }

    /// The text in `start..end`; `(line, len)` is the line break of `line`.
    pub(crate) fn slice(&self, start: Pos, end: Pos) -> String {
        if end <= start {
            return String::new();
        }
        let mut out = String::new();
        for line in start.line..=end.line.min(self.lines.len() - 1) {
            let chars = self.chars(line);
            let from = if line == start.line { start.col } else { 0 };
            let to = if line == end.line {
                end.col.min(chars.len())
            } else {
                chars.len()
            };
            out.extend(chars.get(from.min(to)..to).unwrap_or_default());
            if line < end.line {
                out.push('\n');
            }
        }
        out
    }

    // --- Writing ---------------------------------------------------------------------

    pub(crate) fn set_cursor_raw(&mut self, pos: Pos) {
        self.cursor = pos;
    }

    /// Replaces lines `range` with `new`, recording the change in the open undo step.
    pub(crate) fn splice_lines(&mut self, range: Range<usize>, new: Vec<String>) {
        let end = range.end.min(self.lines.len());
        let start = range.start.min(end);
        let added = new.len();
        self.record(start, end, added);
        self.lines.splice(start..end, new);
        self.no_lines = self.lines.is_empty();
        if self.no_lines {
            self.lines.push(String::new());
            if let Some(step) = &mut self.step {
                step.hi += 1;
            }
        }
        self.revision += 1;
    }

    /// Saves lines `range` for undo without changing them, like Vim's `u_save()`
    /// before a command that may change nothing (`>>` on an empty line): the
    /// command still counts as one undo step.
    pub(crate) fn save_lines(&mut self, range: Range<usize>) {
        let end = range.end.min(self.lines.len());
        let start = range.start.min(end);
        self.record(start, end, end - start);
        if let Some(step) = &mut self.step {
            step.forced = true;
        }
    }

    /// Grows the open undo step (or opens one) to cover lines `start..end`, which
    /// are about to become `added` lines.
    fn record(&mut self, start: usize, end: usize, added: usize) {
        let cursor = self.cursor;
        let step = self.step.get_or_insert_with(|| Step {
            lo: start,
            hi: start,
            before: Vec::new(),
            cursor,
            forced: false,
        });
        // Lines outside the step are still as they were when it began.
        let lo = step.lo.min(start);
        let hi = step.hi.max(end);
        let mut before = self.lines[lo..step.lo].to_vec();
        before.append(&mut step.before);
        before.extend_from_slice(&self.lines[step.hi..hi]);
        step.before = before;
        step.lo = lo;
        step.hi = hi - (end - start) + added;
    }

    /// Whether every line was deleted, leaving the one empty line a buffer always has.
    pub(crate) fn no_lines(&self) -> bool {
        self.no_lines
    }

    /// Closes the open undo step, if any, as one undo entry.
    pub(crate) fn end_step(&mut self) {
        let Some(step) = self.step.take() else {
            return;
        };
        let mut old = step.before;
        let mut new: Vec<String> = self.lines[step.lo..step.hi].to_vec();
        if old == new {
            // Nothing changed: drop the step unless a command saved it on purpose.
            if !step.forced {
                return;
            }
        } else {
            while let (Some(a), Some(b)) = (old.last(), new.last()) {
                if a != b {
                    break;
                }
                old.pop();
                new.pop();
            }
        }
        self.undo.push(UndoEntry {
            start: step.lo,
            old,
            new,
            cursor: step.cursor,
        });
        self.redo.clear();
    }

    /// Reverts the last undo entry. Returns where the cursor goes.
    pub(crate) fn undo(&mut self) -> Option<UndoCursor> {
        self.end_step();
        let entry = self.undo.pop()?;
        let cursor = self.apply(&entry.new, &entry.old, entry.start, entry.cursor);
        self.redo.push(entry);
        Some(cursor)
    }

    /// Re-applies the last undone entry. Returns where the cursor goes.
    pub(crate) fn redo(&mut self) -> Option<UndoCursor> {
        self.end_step();
        let entry = self.redo.pop()?;
        let cursor = self.apply(&entry.old, &entry.new, entry.start, entry.cursor);
        self.undo.push(entry);
        Some(cursor)
    }

    /// Replaces `current` at `start` by `restored`, and places the cursor the way
    /// Vim's `u_undoredo()` does.
    fn apply(
        &mut self,
        current: &[String],
        restored: &[String],
        start: usize,
        uh: Pos,
    ) -> UndoCursor {
        // Vim numbers lines from 1 and `top` is the line above the change.
        let top = start;
        let newsize = restored.len();
        let uh_lnum = uh.line + 1;
        let mut lnum: Option<usize> = None;
        if uh_lnum >= top && uh_lnum <= top + newsize + 1 {
            lnum = Some(uh_lnum);
        } else {
            let same = restored
                .iter()
                .zip(current.iter())
                .take_while(|(a, b)| a == b)
                .count();
            if same == newsize {
                lnum = Some(top + 1);
            } else if same < newsize {
                lnum = Some(top + same + 1);
            }
        }
        let end = (start + current.len()).min(self.lines.len());
        self.lines.splice(start..end, restored.iter().cloned());
        self.no_lines = self.lines.is_empty();
        if self.no_lines {
            self.lines.push(String::new());
        }
        self.revision += 1;

        let mut lnum = lnum.unwrap_or(self.cursor.line + 1);
        if uh_lnum + 1 == lnum && lnum > 1 {
            lnum -= 1;
        }
        let count = self.lines.len();
        if lnum > count {
            return UndoCursor::At(Pos {
                line: count - 1,
                col: 0,
            });
        }
        if uh_lnum == lnum {
            return UndoCursor::At(Pos {
                line: lnum - 1,
                col: uh.col,
            });
        }
        UndoCursor::Line(lnum - 1)
    }
}
