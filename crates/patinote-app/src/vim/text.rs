//! Character classes, display columns, and a cursor that walks the text the way
//! Vim's `inc()`/`dec()` do.

use super::{Buffer, Pos};

/// `'tabstop'` and `'shiftwidth'` under `nvim --clean`.
pub(crate) const TABSTOP: usize = 8;
pub(crate) const SHIFTWIDTH: usize = 8;

/// Vim's character class for word motions: 0 blank, 1 punctuation, 2 word.
///
/// Below U+0100 this follows the default `'iskeyword'` (`@,48-57,_,192-255`).
/// Above it, letters and digits are word characters and everything else is
/// punctuation. Vim gives CJK scripts and emoji classes of their own; that only
/// matters where such characters touch other words, and is not modelled.
pub(crate) fn class(c: Option<char>, big: bool) -> u8 {
    let Some(c) = c else { return 0 };
    if c == ' ' || c == '\t' || c == '\u{a0}' {
        return 0;
    }
    if big {
        return if (c as u32) >= 0x100 && c.is_whitespace() {
            0
        } else {
            1
        };
    }
    if is_word_char(c) {
        return 2;
    }
    if (c as u32) >= 0x100 && c.is_whitespace() {
        return 0;
    }
    1
}

/// Whether `c` is a keyword character (default `'iskeyword'`).
pub(crate) fn is_word_char(c: char) -> bool {
    let n = c as u32;
    if n < 0x100 {
        c.is_ascii_alphanumeric() || c == '_' || (0xC0..=0xFF).contains(&n)
    } else {
        c.is_alphanumeric()
    }
}

pub(crate) fn is_blank(c: char) -> bool {
    c == ' ' || c == '\t'
}

/// Display width of `c` starting at display column `vcol`. Wide (CJK) characters
/// count as one column; the engine does not need their screen width.
pub(crate) fn width(c: char, vcol: usize) -> usize {
    if c == '\t' {
        TABSTOP - vcol % TABSTOP
    } else {
        1
    }
}

/// The display column where char `col` of `chars` starts.
pub(crate) fn vcol_start(chars: &[char], col: usize) -> usize {
    let mut vcol = 0;
    for &c in chars.iter().take(col) {
        vcol += width(c, vcol);
    }
    vcol
}

/// The display column of the cursor on char `col`: the last cell of a tab, or
/// its first cell with `first_cell` (Vim's `w_virtcol`).
pub(crate) fn cursor_vcol(chars: &[char], col: usize, first_cell: bool) -> usize {
    let start = vcol_start(chars, col);
    match chars.get(col) {
        Some(&'\t') if !first_cell => start + width('\t', start) - 1,
        _ => start,
    }
}

/// Vim's `coladvance()`: the char index for display column `want`. With
/// `one_more` the cursor may land on the line end (insert and visual mode).
pub(crate) fn coladvance(chars: &[char], want: usize, one_more: bool) -> usize {
    let mut vcol = 0;
    let mut idx = 0;
    while vcol <= want && idx < chars.len() {
        vcol += width(chars[idx], vcol);
        idx += 1;
    }
    if vcol > want || !one_more {
        idx = idx.saturating_sub(1);
    }
    idx
}

/// Leading whitespace of `line`, as chars.
pub(crate) fn indent_of(line: &str) -> String {
    line.chars().take_while(|&c| is_blank(c)).collect()
}

/// Display width of an indent string.
pub(crate) fn indent_width(indent: &str) -> usize {
    let mut vcol = 0;
    for c in indent.chars() {
        vcol += width(c, vcol);
    }
    vcol
}

/// An indent of `width` columns with `'noexpandtab'`: tabs, then spaces.
pub(crate) fn make_indent(width: usize) -> String {
    let mut s = "\t".repeat(width / TABSTOP);
    s.push_str(&" ".repeat(width % TABSTOP));
    s
}

/// Index of the first non-blank char; on an all-blank line, the last char
/// (Vim's `beginline(BL_WHITE | BL_FIX)`).
pub(crate) fn first_non_blank(chars: &[char]) -> usize {
    let mut col = 0;
    while col < chars.len() && is_blank(chars[col]) && col + 1 < chars.len() {
        col += 1;
    }
    col
}

/// Whether `col` is at or before the first non-blank of `chars` (Vim's `inindent(0)`).
pub(crate) fn in_indent(chars: &[char], col: usize) -> bool {
    chars.iter().take_while(|&&c| is_blank(c)).count() >= col
}

/// A position in the text that steps over chars and line ends like Vim's
/// `inc()`/`dec()`. `(line, len)` is the line end (Vim's NUL).
pub(crate) struct Walker<'a> {
    buf: &'a Buffer,
    pub(crate) pos: Pos,
    chars: Vec<char>,
    pub(crate) big: bool,
}

impl<'a> Walker<'a> {
    pub(crate) fn new(buf: &'a Buffer, pos: Pos, big: bool) -> Self {
        Self {
            buf,
            pos,
            chars: buf.chars(pos.line),
            big,
        }
    }

    pub(crate) fn set(&mut self, pos: Pos) {
        if pos.line != self.pos.line {
            self.chars = self.buf.chars(pos.line);
        }
        self.pos = pos;
    }

    pub(crate) fn line_count(&self) -> usize {
        self.buf.line_count()
    }

    /// The char under the position; `None` at a line end.
    pub(crate) fn get(&self) -> Option<char> {
        self.chars.get(self.pos.col).copied()
    }

    pub(crate) fn cls(&self) -> u8 {
        class(self.get(), self.big)
    }

    pub(crate) fn line_empty(&self) -> bool {
        self.chars.is_empty()
    }

    /// Vim's `inc()`: 0 moved within the line, 2 moved onto the line end, 1 moved
    /// to the next line, -1 at the end of the text.
    pub(crate) fn inc(&mut self) -> i8 {
        if self.pos.col < self.chars.len() {
            self.pos.col += 1;
            return if self.pos.col < self.chars.len() {
                0
            } else {
                2
            };
        }
        if self.pos.line + 1 < self.buf.line_count() {
            self.set(Pos {
                line: self.pos.line + 1,
                col: 0,
            });
            return 1;
        }
        -1
    }

    /// Vim's `dec()`: 0 moved within the line, 1 moved to the end of the previous
    /// line, -1 at the start of the text.
    pub(crate) fn dec(&mut self) -> i8 {
        if self.pos.col > 0 {
            self.pos.col -= 1;
            return 0;
        }
        if self.pos.line > 0 {
            let line = self.pos.line - 1;
            let len = self.buf.line_len(line);
            self.set(Pos { line, col: len });
            return 1;
        }
        -1
    }

    /// Vim's `incl()`: like `inc()` but skips over a line end.
    pub(crate) fn incl(&mut self) -> i8 {
        let r = self.inc();
        if r >= 1 && self.pos.col > 0 {
            return self.inc();
        }
        r
    }

    /// Vim's `decl()`: like `dec()` but skips over a line end.
    pub(crate) fn decl(&mut self) -> i8 {
        let r = self.dec();
        if r == 1 && self.pos.col > 0 {
            return self.dec();
        }
        r
    }

    /// Moves while the class stays `cls`. Returns true at the end (or start) of
    /// the text (Vim's `skip_chars()`).
    pub(crate) fn skip_chars(&mut self, cls: u8, forward: bool) -> bool {
        while self.cls() == cls {
            let r = if forward { self.inc() } else { self.dec() };
            if r == -1 {
                return true;
            }
        }
        false
    }

    /// Vim's `back_in_line()`: to the start of the run of same-class chars.
    pub(crate) fn back_in_line(&mut self) {
        let sclass = self.cls();
        loop {
            if self.pos.col == 0 {
                break;
            }
            self.dec();
            if self.cls() != sclass {
                self.inc();
                break;
            }
        }
    }

    /// Vim's `oneleft()`.
    pub(crate) fn oneleft(&mut self) -> bool {
        if self.pos.col == 0 {
            return false;
        }
        self.pos.col -= 1;
        true
    }
}
