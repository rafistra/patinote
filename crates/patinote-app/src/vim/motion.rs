//! Word motions, `f`/`t`, and text objects, ported from Neovim's `textobject.c`
//! and `search.c` so cursor positions match.

use std::iter::repeat_n;

use super::text::{Walker, in_indent, is_blank};
use super::{Buffer, Pos};

/// Vim's `fwd_word()`: `w`/`W`. With `eol` (an operator is pending) it stops at
/// the end of the line instead of moving to the next one. Returns false when the
/// end of the text was hit before `count` words.
pub(crate) fn fwd_word(w: &mut Walker<'_>, count: usize, eol: bool) -> bool {
    let mut count = count;
    while count > 0 {
        count -= 1;
        let sclass = w.cls();
        let last_line = w.pos.line + 1 == w.line_count();
        let i = w.inc();
        if i == -1 || (i >= 1 && last_line) {
            return false;
        }
        if i >= 1 && eol && count == 0 {
            return true;
        }
        if sclass != 0 {
            while w.cls() == sclass {
                let i = w.inc();
                if i == -1 || (i >= 1 && eol && count == 0) {
                    return true;
                }
            }
        }
        while w.cls() == 0 {
            if w.pos.col == 0 && w.line_empty() {
                break;
            }
            let i = w.inc();
            if i == -1 || (i >= 1 && eol && count == 0) {
                return true;
            }
        }
    }
    true
}

/// Vim's `bck_word()`: `b`/`B`. With `stop`, stays at the start of the current word.
pub(crate) fn bck_word(w: &mut Walker<'_>, count: usize, stop: bool) -> bool {
    let mut stop = stop;
    let mut count = count;
    while count > 0 {
        count -= 1;
        let sclass = w.cls();
        if w.dec() == -1 {
            return false;
        }
        let mut finished = false;
        if !stop || sclass == w.cls() || sclass == 0 {
            while w.cls() == 0 {
                if w.pos.col == 0 && w.line_empty() {
                    finished = true;
                    break;
                }
                if w.dec() == -1 {
                    return true;
                }
            }
            if !finished && w.skip_chars(w.cls(), false) {
                return true;
            }
        }
        if !finished {
            w.inc();
        }
        stop = false;
    }
    true
}

/// Vim's `end_word()`: `e`/`E`. With `stop`, stays at the end of the current word;
/// with `empty`, stops on an empty line.
pub(crate) fn end_word(w: &mut Walker<'_>, count: usize, stop: bool, empty: bool) -> bool {
    let mut stop = stop;
    let mut count = count;
    while count > 0 {
        count -= 1;
        let sclass = w.cls();
        if w.inc() == -1 {
            return false;
        }
        let mut finished = false;
        if w.cls() == sclass && sclass != 0 {
            if w.skip_chars(sclass, true) {
                return false;
            }
        } else if !stop || sclass == 0 {
            while w.cls() == 0 {
                if w.pos.col == 0 && w.line_empty() && empty {
                    finished = true;
                    break;
                }
                if w.inc() == -1 {
                    return false;
                }
            }
            if !finished && w.skip_chars(w.cls(), true) {
                return false;
            }
        }
        if !finished {
            w.dec();
        }
        stop = false;
    }
    true
}

/// Vim's `searchc()` on one line: the column `f`/`F`/`t`/`T` lands on, or `None`.
/// `skip_adjacent` is set for a repeated `t`/`T` (`;`/`,`), which must move.
pub(crate) fn find_char(
    chars: &[char],
    col: usize,
    target: char,
    forward: bool,
    till: bool,
    count: usize,
    skip_adjacent: bool,
) -> Option<usize> {
    let mut col = col;
    let mut stop = !skip_adjacent;
    for _ in repeat_n((), count) {
        loop {
            if forward {
                col += 1;
                if col >= chars.len() {
                    return None;
                }
            } else {
                if col == 0 {
                    return None;
                }
                col -= 1;
            }
            if chars[col] == target && stop {
                break;
            }
            stop = true;
        }
    }
    if till {
        if forward {
            col -= 1;
        } else {
            col += 1;
        }
    }
    Some(col)
}

/// A text object range: `start` to `end`, `end` included when `inclusive`;
/// `linewise` for whole lines.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ObjectRange {
    pub start: Pos,
    pub end: Pos,
    pub inclusive: bool,
}

/// Vim's `current_word()` outside visual mode: `iw`, `aw`, `iW`, `aW`. On failure
/// the error is where the cursor was left, as Vim does not move it back.
pub(crate) fn current_word(
    buf: &Buffer,
    cursor: Pos,
    count: usize,
    include: bool,
    big: bool,
) -> Result<ObjectRange, Pos> {
    let mut w = Walker::new(buf, cursor, big);
    let mut inclusive = true;
    let mut include_white = false;
    w.back_in_line();
    let start_pos = w.pos;
    let mut start = start_pos;
    if (w.cls() == 0) == include {
        if !end_word(&mut w, 1, true, true) {
            return Err(w.pos);
        }
    } else {
        fwd_word(&mut w, 1, true);
        if w.pos.col == 0 {
            w.decl();
        } else {
            w.oneleft();
        }
        if include {
            include_white = true;
        }
    }
    let mut count = count.saturating_sub(1);
    while count > 0 {
        inclusive = true;
        if w.incl() == -1 {
            return Err(w.pos);
        }
        if include != (w.cls() == 0) {
            if !fwd_word(&mut w, 1, true) && count > 1 {
                return Err(w.pos);
            }
            if !w.oneleft() {
                inclusive = false;
            }
        } else if !end_word(&mut w, 1, true, true) {
            return Err(w.pos);
        }
        count -= 1;
    }
    if include_white && (w.cls() != 0 || (w.pos.col == 0 && !inclusive)) {
        // No white space after the word: take the white space before it instead,
        // but not the indent.
        let end = w.pos;
        w.set(start_pos);
        if w.oneleft() {
            w.back_in_line();
            if w.cls() == 0 && w.pos.col > 0 {
                start = w.pos;
            }
        }
        w.set(end);
    }
    Ok(ObjectRange {
        start,
        end: w.pos,
        inclusive,
    })
}

/// Vim's `find_next_quote()`: the next unescaped `quote` at or after `col`.
fn find_next_quote(line: &[char], col: usize, quote: char, escape: bool) -> Option<usize> {
    let mut col = col;
    while col < line.len() {
        let c = line[col];
        if escape && c == '\\' && col + 1 < line.len() {
            col += 2;
            continue;
        }
        if c == quote {
            return Some(col);
        }
        col += 1;
    }
    None
}

/// Vim's `find_prev_quote()`: the previous unescaped `quote` before `col`, or 0.
fn find_prev_quote(line: &[char], col: usize, quote: char) -> usize {
    let mut col = col;
    while col > 0 {
        col -= 1;
        let mut n = 0;
        while col > n && line[col - n - 1] == '\\' {
            n += 1;
        }
        if n % 2 == 1 {
            col -= n;
        } else if line[col] == quote {
            break;
        }
    }
    col
}

/// Vim's `current_quote()` outside visual mode: `i"`, `a"`, `i'`, `a'`.
pub(crate) fn current_quote(
    buf: &Buffer,
    cursor: Pos,
    count: usize,
    include: bool,
    quote: char,
) -> Option<ObjectRange> {
    let line = buf.chars(cursor.line);
    let first = cursor.col;
    if first >= line.len() {
        return None;
    }
    let (mut col_start, mut col_end);
    if line[first] == quote {
        // On a quote: count quotes from the line start to tell opening from closing.
        col_end = 0;
        loop {
            col_start = find_next_quote(&line, col_end, quote, false)?;
            if col_start > first {
                return None;
            }
            col_end = find_next_quote(&line, col_start + 1, quote, true)?;
            if col_start <= first && first <= col_end {
                break;
            }
            col_end += 1;
        }
    } else {
        col_start = find_prev_quote(&line, first, quote);
        if line[col_start] != quote {
            col_start = find_next_quote(&line, col_start, quote, false)?;
        }
        col_end = find_next_quote(&line, col_start + 1, quote, true)?;
    }
    if include {
        if line.get(col_end + 1).is_some_and(|&c| is_blank(c)) {
            while line.get(col_end + 1).is_some_and(|&c| is_blank(c)) {
                col_end += 1;
            }
        } else {
            while col_start > 0 && is_blank(line[col_start - 1]) {
                col_start -= 1;
            }
        }
        return Some(ObjectRange {
            start: Pos {
                line: cursor.line,
                col: col_start,
            },
            end: Pos {
                line: cursor.line,
                col: col_end,
            },
            inclusive: true,
        });
    }
    // `2i"` is the quoted text with the quotes but without white space.
    let skip = usize::from(count < 2);
    Some(ObjectRange {
        start: Pos {
            line: cursor.line,
            col: col_start + skip,
        },
        end: Pos {
            line: cursor.line,
            col: col_end,
        },
        inclusive: count >= 2,
    })
}

/// The unmatched `open` before `pos` (`backward`) or the unmatched `close` after
/// it, across lines, ignoring quotes (Vim's `findmatch()` with `cpo-%`).
fn find_match(buf: &Buffer, pos: Pos, open: char, close: char, backward: bool) -> Option<Pos> {
    let mut w = Walker::new(buf, pos, false);
    let mut depth = 0usize;
    loop {
        let r = if backward { w.dec() } else { w.inc() };
        if r == -1 {
            return None;
        }
        let Some(c) = w.get() else { continue };
        if backward {
            if c == close {
                depth += 1;
            } else if c == open {
                if depth == 0 {
                    return Some(w.pos);
                }
                depth -= 1;
            }
        } else if c == open {
            depth += 1;
        } else if c == close {
            if depth == 0 {
                return Some(w.pos);
            }
            depth -= 1;
        }
    }
}

/// Vim's `current_block()` outside visual mode: `i(`, `a(`, `i[`, `a[`.
pub(crate) fn current_block(
    buf: &Buffer,
    cursor: Pos,
    count: usize,
    include: bool,
    open: char,
    close: char,
) -> Option<ObjectRange> {
    let mut pos = cursor;
    if buf.char_at(pos) == Some(open) {
        pos.col += 1;
    }
    let mut start_pos = None;
    // Inside a block: the enclosing one. Outside: the next one after the cursor.
    let backward = find_match(buf, pos, open, close, true).is_some();
    for _ in repeat_n((), count.max(1)) {
        let found = if backward {
            find_match(buf, pos, open, close, true)
        } else {
            find_match(buf, pos, close, open, false)
        };
        // Vim fails when there are fewer blocks than the count.
        let p = found?;
        pos = p;
        start_pos = Some(p);
    }
    let start_pos = start_pos?;
    let end_pos = find_match(buf, start_pos, open, close, false)?;
    let mut start = start_pos;
    let mut end = end_pos;
    let mut sol = false;
    if !include {
        let mut w = Walker::new(buf, start, false);
        w.incl();
        start = w.pos;
        sol = end.col == 0;
        let mut w = Walker::new(buf, end, false);
        w.decl();
        loop {
            let chars = buf.chars(w.pos.line);
            if !in_indent(&chars, w.pos.col + 1) {
                break;
            }
            sol = true;
            if w.decl() != 0 {
                break;
            }
        }
        end = w.pos;
    }
    if sol {
        let mut w = Walker::new(buf, end, false);
        w.incl();
        return Some(ObjectRange {
            start,
            end: w.pos,
            inclusive: false,
        });
    }
    if start <= end {
        return Some(ObjectRange {
            start,
            end,
            inclusive: true,
        });
    }
    // Nothing between the brackets: an empty range.
    Some(ObjectRange {
        start,
        end: start,
        inclusive: false,
    })
}
