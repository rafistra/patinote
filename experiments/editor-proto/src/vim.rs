//! Minimal Vim logic, free of Iced types so it can be tested without a window
//! (CLAUDE.md, section 5). Covers only what the prototype needs; the real
//! module (F-06) is written in step 2.3 against `nvim --headless` fixtures.

/// Editing mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Insert,
    Visual,
    /// Typing a `/` search pattern.
    Search,
}

/// A position: line index and character index within the line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct Pos {
    pub line: usize,
    pub col: usize,
}

/// A key press, already decoded from the window system.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Key {
    Char(char),
    Esc,
    Enter,
    Backspace,
}

/// Document text, one `String` per line.
#[derive(Debug, Clone, Default)]
pub struct Buffer {
    pub lines: Vec<String>,
}

impl Buffer {
    /// Splits text into lines; an empty text has one empty line.
    pub fn from_text(text: &str) -> Self {
        let mut lines: Vec<String> = text.lines().map(str::to_owned).collect();
        if lines.is_empty() {
            lines.push(String::new());
        }
        Self { lines }
    }

    fn len(&self, line: usize) -> usize {
        self.lines[line].chars().count()
    }

    fn byte(&self, line: usize, col: usize) -> usize {
        let s = &self.lines[line];
        s.char_indices().nth(col).map_or(s.len(), |(i, _)| i)
    }

    fn char_at(&self, p: Pos) -> Option<char> {
        self.lines.get(p.line)?.chars().nth(p.col)
    }
}

/// Vim state.
#[derive(Debug, Clone)]
pub struct Vim {
    pub mode: Mode,
    pub cursor: Pos,
    /// Start of the visual selection.
    pub anchor: Pos,
    /// Last committed search pattern.
    pub search: Option<String>,
    /// Pattern being typed after `/`.
    pub search_input: String,
    pending: Option<char>,
    register: String,
}

impl Default for Vim {
    fn default() -> Self {
        Self {
            mode: Mode::Normal,
            cursor: Pos::default(),
            anchor: Pos::default(),
            search: None,
            search_input: String::new(),
            pending: None,
            register: String::new(),
        }
    }
}

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

impl Vim {
    /// The visual selection as an inclusive (start, end) pair, if any.
    pub fn selection(&self) -> Option<(Pos, Pos)> {
        (self.mode == Mode::Visual).then(|| {
            if self.anchor <= self.cursor {
                (self.anchor, self.cursor)
            } else {
                (self.cursor, self.anchor)
            }
        })
    }

    /// Handles one key.
    pub fn handle(&mut self, buf: &mut Buffer, key: Key) {
        match self.mode {
            Mode::Insert => self.insert(buf, key),
            Mode::Search => self.search_key(buf, key),
            Mode::Normal | Mode::Visual => self.normal(buf, key),
        }
        self.clamp(buf);
    }

    fn clamp(&mut self, buf: &Buffer) {
        self.cursor.line = self.cursor.line.min(buf.lines.len() - 1);
        let len = buf.len(self.cursor.line);
        let max = if self.mode == Mode::Insert {
            len
        } else {
            len.saturating_sub(1)
        };
        self.cursor.col = self.cursor.col.min(max);
    }

    fn insert(&mut self, buf: &mut Buffer, key: Key) {
        let Pos { line, col } = self.cursor;
        match key {
            Key::Esc => {
                self.mode = Mode::Normal;
                self.cursor.col = col.saturating_sub(1);
            }
            Key::Char(c) => {
                let at = buf.byte(line, col);
                buf.lines[line].insert(at, c);
                self.cursor.col += 1;
            }
            Key::Enter => {
                let at = buf.byte(line, col);
                let rest = buf.lines[line].split_off(at);
                buf.lines.insert(line + 1, rest);
                self.cursor = Pos {
                    line: line + 1,
                    col: 0,
                };
            }
            Key::Backspace => {
                if col > 0 {
                    let at = buf.byte(line, col - 1);
                    buf.lines[line].remove(at);
                    self.cursor.col -= 1;
                } else if line > 0 {
                    let cur = buf.lines.remove(line);
                    let prev_len = buf.len(line - 1);
                    buf.lines[line - 1].push_str(&cur);
                    self.cursor = Pos {
                        line: line - 1,
                        col: prev_len,
                    };
                }
            }
        }
    }

    fn search_key(&mut self, buf: &Buffer, key: Key) {
        match key {
            Key::Esc => {
                self.search_input.clear();
                self.mode = Mode::Normal;
            }
            Key::Enter => {
                self.mode = Mode::Normal;
                if !self.search_input.is_empty() {
                    self.search = Some(std::mem::take(&mut self.search_input));
                    self.next_match(buf, true);
                }
            }
            Key::Backspace => {
                if self.search_input.pop().is_none() {
                    self.mode = Mode::Normal;
                }
            }
            Key::Char(c) => self.search_input.push(c),
        }
    }

    /// All matches of the current search on `line`, as character ranges.
    pub fn matches_in(&self, text: &str) -> Vec<(usize, usize)> {
        let Some(pat) = self.search.as_deref().filter(|p| !p.is_empty()) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        let mut from = 0;
        while let Some(i) = text[from..].find(pat) {
            let start = text[..from + i].chars().count();
            out.push((start, start + pat.chars().count()));
            from += i + pat.len().max(1);
        }
        out
    }

    fn next_match(&mut self, buf: &Buffer, forward: bool) {
        let n = buf.lines.len();
        let here = self.cursor;
        for step in 0..=n {
            let line = if forward {
                (here.line + step) % n
            } else {
                (here.line + n - step % n) % n
            };
            let mut ms = self.matches_in(&buf.lines[line]);
            if !forward {
                ms.reverse();
            }
            for (start, _) in ms {
                let p = Pos { line, col: start };
                let ok = if step == 0 {
                    if forward { p > here } else { p < here }
                } else {
                    true
                };
                if ok {
                    self.cursor = p;
                    return;
                }
            }
        }
    }

    fn word_forward(&mut self, buf: &Buffer) {
        let mut p = self.cursor;
        let start_word = buf.char_at(p).is_some_and(is_word);
        // Skip the rest of the current word, then whitespace.
        while let Some(c) = buf.char_at(p) {
            if is_word(c) != start_word || c.is_whitespace() {
                break;
            }
            p.col += 1;
        }
        loop {
            match buf.char_at(p) {
                Some(c) if c.is_whitespace() => p.col += 1,
                Some(_) => break,
                None if p.line + 1 < buf.lines.len() => {
                    p = Pos {
                        line: p.line + 1,
                        col: 0,
                    }
                }
                None => break,
            }
            if buf.char_at(p).is_some_and(|c| !c.is_whitespace()) {
                break;
            }
        }
        self.cursor = p;
    }

    fn word_back(&mut self, buf: &Buffer) {
        let mut p = self.cursor;
        let back = |p: &mut Pos| -> bool {
            if p.col > 0 {
                p.col -= 1;
                true
            } else if p.line > 0 {
                p.line -= 1;
                p.col = buf.len(p.line);
                true
            } else {
                false
            }
        };
        if !back(&mut p) {
            return;
        }
        while buf.char_at(p).is_none_or(char::is_whitespace) {
            if !back(&mut p) {
                break;
            }
        }
        let w = buf.char_at(p).is_some_and(is_word);
        while p.col > 0
            && buf
                .char_at(Pos {
                    line: p.line,
                    col: p.col - 1,
                })
                .is_some_and(|c| !c.is_whitespace() && is_word(c) == w)
        {
            p.col -= 1;
        }
        self.cursor = p;
    }

    fn delete_selection(&mut self, buf: &mut Buffer) {
        let Some((a, b)) = self.selection() else {
            return;
        };
        let mut taken = String::new();
        if a.line == b.line {
            let s = buf.byte(a.line, a.col);
            let e = buf.byte(a.line, (b.col + 1).min(buf.len(a.line)));
            taken.push_str(&buf.lines[a.line][s..e]);
            buf.lines[a.line].replace_range(s..e, "");
        } else {
            let s = buf.byte(a.line, a.col);
            let e = buf.byte(b.line, (b.col + 1).min(buf.len(b.line)));
            let tail = buf.lines[b.line][e..].to_owned();
            taken.push_str(&buf.lines[a.line][s..]);
            buf.lines[a.line].truncate(s);
            buf.lines[a.line].push_str(&tail);
            buf.lines.drain(a.line + 1..=b.line);
        }
        self.register = taken;
        self.cursor = a;
        self.mode = Mode::Normal;
    }

    fn normal(&mut self, buf: &mut Buffer, key: Key) {
        let Key::Char(c) = key else {
            if key == Key::Esc {
                self.mode = Mode::Normal;
                self.pending = None;
            }
            return;
        };
        if let Some(p) = self.pending.take() {
            match (p, c) {
                ('d', 'd') if self.mode == Mode::Normal => {
                    self.register = buf.lines[self.cursor.line].clone();
                    if buf.lines.len() > 1 {
                        buf.lines.remove(self.cursor.line);
                    } else {
                        buf.lines[0].clear();
                    }
                    self.cursor.col = 0;
                }
                ('g', 'g') => self.cursor = Pos::default(),
                _ => {}
            }
            return;
        }
        let Pos { line, col } = self.cursor;
        match c {
            'h' => self.cursor.col = col.saturating_sub(1),
            'l' => self.cursor.col = col + 1,
            'j' => self.cursor.line = (line + 1).min(buf.lines.len() - 1),
            'k' => self.cursor.line = line.saturating_sub(1),
            '0' => self.cursor.col = 0,
            '$' => self.cursor.col = usize::MAX,
            'w' => self.word_forward(buf),
            'b' => self.word_back(buf),
            'G' => self.cursor.line = buf.lines.len() - 1,
            'x' | 'd' if self.mode == Mode::Visual => self.delete_selection(buf),
            'g' | 'd' => self.pending = Some(c),
            'n' => self.next_match(buf, true),
            'N' => self.next_match(buf, false),
            '/' if self.mode == Mode::Normal => {
                self.mode = Mode::Search;
                self.search_input.clear();
            }
            'v' => {
                if self.mode == Mode::Visual {
                    self.mode = Mode::Normal;
                } else {
                    self.mode = Mode::Visual;
                    self.anchor = self.cursor;
                }
            }
            'x' if self.mode == Mode::Normal => {
                if col < buf.len(line) {
                    let at = buf.byte(line, col);
                    buf.lines[line].remove(at);
                }
            }
            'i' if self.mode == Mode::Normal => self.mode = Mode::Insert,
            'a' if self.mode == Mode::Normal => {
                self.mode = Mode::Insert;
                self.cursor.col = (col + 1).min(buf.len(line));
            }
            'A' if self.mode == Mode::Normal => {
                self.mode = Mode::Insert;
                self.cursor.col = buf.len(line);
            }
            'o' if self.mode == Mode::Normal => {
                buf.lines.insert(line + 1, String::new());
                self.cursor = Pos {
                    line: line + 1,
                    col: 0,
                };
                self.mode = Mode::Insert;
            }
            'O' if self.mode == Mode::Normal => {
                buf.lines.insert(line, String::new());
                self.cursor = Pos { line, col: 0 };
                self.mode = Mode::Insert;
            }
            _ => {}
        }
        if c == '$' {
            self.cursor.col = buf.len(self.cursor.line).saturating_sub(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(text: &str, keys: &str) -> (Buffer, Vim) {
        let mut buf = Buffer::from_text(text);
        let mut vim = Vim::default();
        for c in keys.chars() {
            let key = match c {
                '⎋' => Key::Esc,
                '⏎' => Key::Enter,
                '⌫' => Key::Backspace,
                c => Key::Char(c),
            };
            vim.handle(&mut buf, key);
        }
        (buf, vim)
    }

    #[test]
    fn motions() {
        let (_, v) = run("one two three\nfour", "ww");
        assert_eq!(v.cursor, Pos { line: 0, col: 8 });
        let (_, v) = run("one two three\nfour", "$b");
        assert_eq!(v.cursor, Pos { line: 0, col: 8 });
        let (_, v) = run("a\nb\nc", "jjjk");
        assert_eq!(v.cursor.line, 1);
        let (_, v) = run("a\nb\nc", "Ggg");
        assert_eq!(v.cursor.line, 0);
    }

    #[test]
    fn insert_and_escape() {
        let (b, v) = run("ac", "lib⎋");
        assert_eq!(b.lines, ["abc"]);
        assert_eq!((v.mode, v.cursor.col), (Mode::Normal, 1));
        let (b, _) = run("one", "Atwo⏎x⎋");
        assert_eq!(b.lines, ["onetwo", "x"]);
        let (b, _) = run("ab\ncd", "ji⌫⎋");
        assert_eq!(b.lines, ["abcd"]);
    }

    #[test]
    fn deletes() {
        let (b, _) = run("a\nb\nc", "jdd");
        assert_eq!(b.lines, ["a", "c"]);
        let (b, _) = run("abc", "lx");
        assert_eq!(b.lines, ["ac"]);
        let (b, _) = run("hello world", "lvlld");
        assert_eq!(b.lines, ["ho world"]);
    }

    #[test]
    fn search() {
        let (_, v) = run("x idx\nidx y idx", "/idx⏎");
        assert_eq!(v.cursor, Pos { line: 0, col: 2 });
        let (_, v) = run("x idx\nidx y idx", "/idx⏎nn");
        assert_eq!(v.cursor, Pos { line: 1, col: 6 });
        let (_, v) = run("x idx\nidx y idx", "/idx⏎N");
        assert_eq!(v.cursor, Pos { line: 1, col: 6 });
        assert_eq!(v.matches_in("idx idx"), [(0, 3), (4, 7)]);
    }
}
