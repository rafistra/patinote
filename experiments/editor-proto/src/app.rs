//! The prototype application: state, input, status line, and the automatic
//! benchmark used to measure latency and memory (see `--bench`).

use std::cell::RefCell;
use std::collections::HashSet;
use std::ops::Range;
use std::time::{Duration, Instant};

use iced::advanced::text::Highlighter;
use iced::keyboard::{self, key::Named};
use iced::widget::{column, container, row, space, text, text_editor};
use iced::{Element, Font, Length, Subscription, Task, Theme, window};

/// Lets a widget be passed through a closure in builder chains.
trait Apply: Sized {
    fn apply<T>(self, f: impl FnOnce(Self) -> T) -> T {
        f(self)
    }
}

impl<T> Apply for T {}

use crate::editor::{Editor, MONO};
use crate::highlight::{self, LineKind};
use crate::tokens::*;
use crate::vim::{Buffer, Key, Mode, Vim};

/// Approximate number of text rows on screen, for scrolling.
const VISIBLE_ROWS: usize = 28;

pub struct App {
    pub path: String,
    pub buf: Buffer,
    pub vim: Vim,
    pub kinds: Vec<LineKind>,
    pub top: usize,
    pub ambiguous: HashSet<String>,
    pub drawn_at: RefCell<Option<Instant>>,
    /// `Some` in `--builtin` mode: the stock `text_editor` with our highlighter.
    pub builtin: Option<text_editor::Content>,
    pub bench: Option<Bench>,
    pub started: Instant,
}

#[derive(Debug, Clone)]
pub enum Message {
    Key(Key),
    Frame(Instant),
    Edit(text_editor::Action),
    Finish,
}

/// Options from the command line.
pub struct Options {
    pub path: String,
    pub builtin: bool,
    pub bench: bool,
    pub started: Instant,
}

impl App {
    pub fn new(opts: Options) -> Self {
        let text = std::fs::read_to_string(&opts.path)
            .unwrap_or_else(|e| format!("cannot read {}: {e}", opts.path));
        let buf = Buffer::from_text(&text);
        let kinds = highlight::line_kinds(&buf.lines);
        let name = std::path::Path::new(&opts.path)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        Self {
            path: name,
            builtin: opts.builtin.then(|| text_editor::Content::with_text(&text)),
            buf,
            vim: Vim::default(),
            kinds,
            top: 0,
            // The fixture vault has two notes named "Rapat" (fixtures/README.md).
            ambiguous: HashSet::from(["rapat".to_owned()]),
            drawn_at: RefCell::new(None),
            bench: opts.bench.then(Bench::new),
            started: opts.started,
        }
    }

    pub fn title(&self) -> String {
        format!("{} — Patinote editor prototype", self.path)
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Key(key) => self.key(key),
            Message::Edit(action) => {
                if let Some(content) = &mut self.builtin {
                    content.perform(action);
                }
            }
            Message::Frame(_) => return self.bench_frame(),
            Message::Finish => {
                if let Some(bench) = &self.bench {
                    println!("{}", bench.report(self.started));
                }
                return iced::exit();
            }
        }
        Task::none()
    }

    fn key(&mut self, key: Key) {
        let before = self.buf.lines.len();
        self.vim.handle(&mut self.buf, key);
        if self.buf.lines.len() != before || self.vim.mode == Mode::Insert {
            self.kinds = highlight::line_kinds(&self.buf.lines);
        }
        let line = self.vim.cursor.line;
        if line < self.top {
            self.top = line;
        } else if line >= self.top + VISIBLE_ROWS {
            self.top = line + 1 - VISIBLE_ROWS;
        }
    }

    fn bench_frame(&mut self) -> Task<Message> {
        let drawn = *self.drawn_at.borrow();
        let Some(bench) = &mut self.bench else {
            return Task::none();
        };
        if bench.first_draw.is_none() {
            bench.first_draw = drawn;
        }
        let phase = bench.phase;
        bench.frames.push((Instant::now(), phase));
        if let (Some(sent), Some(drawn)) = (bench.pending, drawn)
            && drawn > sent
        {
            bench.record(drawn - sent);
            bench.pending = None;
        }
        if bench.pending.is_some() || bench.frames.len() % 2 == 1 {
            return Task::none();
        }
        match bench.next_key() {
            Some(key) => {
                bench.pending = Some(Instant::now());
                self.key(key);
                Task::none()
            }
            None if !bench.done => {
                bench.done = true;
                // Let the app go idle before reading memory.
                Task::perform(async { std::thread::sleep(Duration::from_secs(2)) }, |()| {
                    Message::Finish
                })
            }
            None => Task::none(),
        }
    }

    pub fn subscription(&self) -> Subscription<Message> {
        let keys = keyboard::listen().filter_map(|event| match event {
            keyboard::Event::KeyPressed { key, text, .. } => match key {
                keyboard::Key::Named(Named::Escape) => Some(Message::Key(Key::Esc)),
                keyboard::Key::Named(Named::Enter) => Some(Message::Key(Key::Enter)),
                keyboard::Key::Named(Named::Backspace) => Some(Message::Key(Key::Backspace)),
                _ => text
                    .and_then(|t| t.chars().next())
                    .filter(|c| !c.is_control())
                    .map(|c| Message::Key(Key::Char(c))),
            },
            _ => None,
        });
        let running = self.bench.as_ref().is_some_and(|b| !b.done);
        if self.builtin.is_some() {
            Subscription::none()
        } else if running {
            Subscription::batch([keys, window::frames().map(Message::Frame)])
        } else {
            keys
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
        let body: Element<'_, Message> = match &self.builtin {
            Some(content) => container(
                text_editor(content)
                    .on_action(Message::Edit)
                    .font(MONO)
                    .size(EDITOR_TEXT_SIZE)
                    .line_height(iced::widget::text::LineHeight::Absolute(
                        EDITOR_LINE_HEIGHT.into(),
                    ))
                    .height(Length::Fill)
                    .highlight_with::<MarkdownHighlighter>((), |color, _theme| {
                        iced::advanced::text::highlighter::Format {
                            color: Some(*color),
                            font: None,
                        }
                    })
                    .style(|_theme, _status| text_editor::Style {
                        background: BG_APP.into(),
                        border: iced::Border::default(),
                        placeholder: TEXT_MUTED,
                        value: TEXT_PRIMARY,
                        selection: EDITOR_SELECTION,
                    }),
            )
            .max_width(EDITOR_MAX_WIDTH)
            .padding([EDITOR_PAD_TOP, 0.0])
            .apply(|inner| {
                container(inner)
                    .center_x(Length::Fill)
                    .style(|_| container::background(BG_APP))
            })
            .into(),
            None => Editor {
                lines: &self.buf.lines,
                kinds: &self.kinds,
                vim: &self.vim,
                top: self.top,
                ambiguous: &self.ambiguous,
                relative_numbers: true,
                drawn_at: &self.drawn_at,
            }
            .into(),
        };
        column![body, self.status_line()].into()
    }

    fn status_line(&self) -> Element<'_, Message> {
        let (label, color) = match self.vim.mode {
            Mode::Normal | Mode::Search => ("NORMAL", MODE_NORMAL),
            Mode::Insert => ("INSERT", MODE_INSERT),
            Mode::Visual => ("VISUAL", MODE_VISUAL),
        };
        let mono = |s: String, c| text(s).font(MONO).size(MONO_UI_SIZE).color(c);
        let badge = container(
            text(label)
                .font(Font {
                    weight: iced::font::Weight::Bold,
                    ..MONO
                })
                .size(MONO_UI_SIZE)
                .color(TEXT_ON_ACCENT),
        )
        .padding([0.0, 10.0])
        .center_y(Length::Fill)
        .style(move |_| container::background(color));
        let search = if self.vim.mode == Mode::Search {
            mono(format!("/{}", self.vim.search_input), TEXT_PRIMARY)
        } else if let Some(pat) = &self.vim.search {
            let all: Vec<(usize, usize)> = self
                .buf
                .lines
                .iter()
                .enumerate()
                .flat_map(|(l, t)| self.vim.matches_in(t).into_iter().map(move |(s, _)| (l, s)))
                .collect();
            let at = all
                .iter()
                .position(|&(l, s)| l == self.vim.cursor.line && s == self.vim.cursor.col);
            let pos = at.map_or("-".to_owned(), |a| (a + 1).to_string());
            mono(format!("/{pat} [{pos}/{}]", all.len()), SYNTAX_CODE)
        } else {
            mono(String::new(), TEXT_MUTED)
        };
        let pos = mono(
            format!("{}:{}", self.vim.cursor.line + 1, self.vim.cursor.col + 1),
            TEXT_SECONDARY,
        );
        container(
            row![
                badge,
                mono(self.path.clone(), TEXT_SECONDARY),
                search,
                space::horizontal(),
                pos
            ]
            .spacing(12)
            .padding([0.0, 12.0])
            .align_y(iced::Alignment::Center),
        )
        .height(STATUSLINE_HEIGHT)
        .width(Length::Fill)
        .style(|_| container::background(BG_STATUSLINE))
        .into()
    }

    pub fn theme(&self) -> Theme {
        Theme::Dark
    }
}

/// Item 1 with the stock widget: a `text_editor` highlighter that colors runs
/// with the bundled font. It can only set color and font (`text::Format`).
pub struct MarkdownHighlighter {
    line: usize,
    in_front: bool,
    in_fence: bool,
}

impl Highlighter for MarkdownHighlighter {
    type Settings = ();
    type Highlight = iced::Color;
    type Iterator<'a> = std::vec::IntoIter<(Range<usize>, iced::Color)>;

    fn new(_settings: &()) -> Self {
        Self {
            line: 0,
            in_front: false,
            in_fence: false,
        }
    }

    fn update(&mut self, _settings: &()) {}

    fn change_line(&mut self, line: usize) {
        self.line = line;
        if line == 0 {
            self.in_front = false;
            self.in_fence = false;
        }
    }

    fn highlight_line(&mut self, line: &str) -> Self::Iterator<'_> {
        let kind = if (self.line == 0 && line == "---") || self.in_front {
            self.in_front = !(self.line > 0 && line == "---");
            LineKind::Frontmatter
        } else if line.trim_start().starts_with("```") || self.in_fence {
            if line.trim_start().starts_with("```") {
                self.in_fence = !self.in_fence;
            }
            LineKind::Fence
        } else {
            LineKind::Text
        };
        self.line += 1;
        let byte = |c: usize| line.char_indices().nth(c).map_or(line.len(), |(i, _)| i);
        highlight::segments(line, kind)
            .into_iter()
            .map(|s| (byte(s.start)..byte(s.end), s.color))
            .collect::<Vec<_>>()
            .into_iter()
    }

    fn current_line(&self) -> usize {
        self.line
    }
}

/// Scripted keys and the measurements taken while they are replayed.
pub struct Bench {
    phases: Vec<(&'static str, Vec<Key>)>,
    phase: usize,
    index: usize,
    pub pending: Option<Instant>,
    /// Frame times with the phase they belong to.
    pub frames: Vec<(Instant, usize)>,
    pub first_draw: Option<Instant>,
    samples: Vec<Vec<f64>>,
    pub done: bool,
}

impl Bench {
    fn new() -> Self {
        let typing: String = "the quick brown fox jumps over the lazy dog ".repeat(4);
        let mut type_keys = vec![Key::Char('i')];
        type_keys.extend(typing.chars().map(Key::Char));
        type_keys.push(Key::Esc);
        let mut search = vec![Key::Char('/')];
        search.extend("indeks".chars().map(Key::Char));
        search.push(Key::Enter);
        search.extend(std::iter::repeat_n(Key::Char('n'), 30));
        let mut visual = vec![Key::Char('v')];
        visual.extend(std::iter::repeat_n(Key::Char('j'), 30));
        visual.push(Key::Esc);
        let phases = vec![
            ("hold_j", std::iter::repeat_n(Key::Char('j'), 200).collect()),
            ("typing", type_keys),
            ("search_n", search),
            ("visual_j", visual),
        ];
        let samples = vec![Vec::new(); phases.len()];
        Self {
            phases,
            phase: 0,
            index: 0,
            pending: None,
            frames: Vec::new(),
            first_draw: None,
            samples,
            done: false,
        }
    }

    fn next_key(&mut self) -> Option<Key> {
        while self.phase < self.phases.len() {
            if let Some(k) = self.phases[self.phase].1.get(self.index) {
                self.index += 1;
                return Some(k.clone());
            }
            self.phase += 1;
            self.index = 0;
        }
        None
    }

    fn record(&mut self, d: Duration) {
        let phase = if self.index == 0 {
            self.phase.saturating_sub(1)
        } else {
            self.phase
        };
        let last = self.samples.len() - 1;
        self.samples[phase.min(last)].push(d.as_secs_f64() * 1000.0);
    }

    fn report(&self, started: Instant) -> String {
        let stats = |v: &[f64]| {
            let mut s = v.to_vec();
            s.sort_by(f64::total_cmp);
            let q = |p: f64| {
                s.get(((s.len() as f64 - 1.0) * p).round() as usize)
                    .copied()
                    .unwrap_or(f64::NAN)
            };
            format!(
                "{{\"n\":{},\"p50\":{:.2},\"p95\":{:.2},\"max\":{:.2}}}",
                s.len(),
                q(0.5),
                q(0.95),
                q(1.0)
            )
        };
        let phases: Vec<String> = self
            .phases
            .iter()
            .zip(&self.samples)
            .map(|((name, _), v)| format!("\"{name}\":{}", stats(v)))
            .collect();
        let intervals: Vec<String> = self
            .phases
            .iter()
            .enumerate()
            .map(|(i, (name, _))| {
                let v: Vec<f64> = self
                    .frames
                    .windows(2)
                    .filter(|w| w[0].1 == i && w[1].1 == i)
                    .map(|w| (w[1].0 - w[0].0).as_secs_f64() * 1000.0)
                    .collect();
                format!("\"{name}\":{}", stats(&v))
            })
            .collect();
        let (pss, rss) = memory_kb();
        let startup = self
            .first_draw
            .map_or(f64::NAN, |d| (d - started).as_secs_f64() * 1000.0);
        format!(
            "{{\"backend\":\"{}\",\"fontconfig_file\":\"{}\",\"startup_to_first_draw_ms\":{startup:.1},\"pss_kb\":{pss},\"rss_kb\":{rss},\"event_to_draw_ms\":{{{}}},\"frame_interval_ms\":{{{}}}}}",
            std::env::var("ICED_BACKEND").unwrap_or_default(),
            std::env::var("FONTCONFIG_FILE").unwrap_or_default(),
            phases.join(","),
            intervals.join(","),
        )
    }
}

/// Reads PSS and RSS of this process in kB from /proc/self/smaps_rollup.
pub fn memory_kb() -> (u64, u64) {
    let text = std::fs::read_to_string("/proc/self/smaps_rollup").unwrap_or_default();
    let field = |name: &str| {
        text.lines()
            .find(|l| l.starts_with(name))
            .and_then(|l| l.split_whitespace().nth(1))
            .and_then(|v| v.parse().ok())
            .unwrap_or(0)
    };
    (field("Pss:"), field("Rss:"))
}
