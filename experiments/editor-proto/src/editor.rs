//! Custom editor view built on `iced::advanced`. It answers items 2–7 of the
//! display checklist in CLAUDE.md, section 3, which the built-in `text_editor`
//! cannot draw: per-run backgrounds, a full-width cursor line, a wavy underline,
//! block and line cursors in token colors, and a relative line-number gutter
//! that stays aligned with soft-wrapped lines.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::time::Instant;

use iced::advanced::layout::{self, Layout};
use iced::advanced::mouse;
use iced::advanced::renderer::{self, Quad};
use iced::advanced::text::{self, Paragraph as _, Span, Text};
use iced::advanced::widget::{self, Widget, tree};
use iced::alignment::Vertical;
use iced::font::Weight;
use iced::widget::text::{Alignment, LineHeight, Shaping, Wrapping};
use iced::{Border, Color, Element, Font, Length, Pixels, Point, Rectangle, Size};

use crate::highlight::{self, LineKind};
use crate::tokens::*;
use crate::vim::{Mode, Vim};

/// The bundled editor font, loaded from bytes at startup.
pub const MONO: Font = Font::with_name("JetBrains Mono");

/// What the editor shows. Borrowed from the application state.
pub struct Editor<'a> {
    pub lines: &'a [String],
    pub kinds: &'a [LineKind],
    pub vim: &'a Vim,
    pub top: usize,
    pub ambiguous: &'a HashSet<String>,
    pub relative_numbers: bool,
    /// Filled with the time `draw` finished, for latency measurements.
    pub drawn_at: &'a RefCell<Option<Instant>>,
}

/// Background of a run of text.
#[derive(Clone, Copy, PartialEq)]
enum Bg {
    None,
    Selection,
    Match,
    Current,
}

struct Run {
    bg: Bg,
    ambiguous: bool,
}

struct Cached<P> {
    key: u64,
    paragraph: P,
    runs: Vec<Run>,
}

/// Paragraphs are cached per line and rebuilt only when the line, its
/// decorations or the column width change.
pub struct State<P> {
    lines: RefCell<HashMap<usize, Cached<P>>>,
}

impl<'a> Editor<'a> {
    fn text_width(bounds: Rectangle) -> f32 {
        Self::column(bounds).width - GUTTER_WIDTH
    }

    fn column(bounds: Rectangle) -> Rectangle {
        let width =
            (bounds.width - 2.0 * EDITOR_PAD_SIDE).clamp(GUTTER_WIDTH + 40.0, EDITOR_MAX_WIDTH);
        Rectangle {
            x: bounds.x + (bounds.width - width) / 2.0,
            y: bounds.y + EDITOR_PAD_TOP,
            width,
            height: bounds.height - EDITOR_PAD_TOP,
        }
    }

    /// Splits line `i` into styled runs, including selection and search decorations.
    fn runs(&self, i: usize) -> (Vec<(String, Color, bool)>, Vec<Run>) {
        let text = &self.lines[i];
        let chars: Vec<char> = text.chars().collect();
        let n = chars.len();
        let segs = highlight::segments(text, self.kinds[i]);
        let matches = self.vim.matches_in(text);
        let current = (self.vim.mode != Mode::Search)
            .then(|| {
                matches
                    .iter()
                    .find(|(s, _)| self.vim.cursor.line == i && *s == self.vim.cursor.col)
            })
            .flatten()
            .copied();
        let selection = self.vim.selection().and_then(|(a, b)| {
            (a.line <= i && i <= b.line).then(|| {
                let s = if a.line == i { a.col } else { 0 };
                let e = if b.line == i {
                    (b.col + 1).min(n.max(1))
                } else {
                    n.max(1)
                };
                (s, e)
            })
        });

        let mut cuts: Vec<usize> = vec![0, n];
        for s in &segs {
            cuts.extend([s.start, s.end]);
        }
        for (s, e) in matches.iter().copied().chain(selection) {
            cuts.extend([s, e]);
        }
        cuts.retain(|&c| c <= n);
        cuts.sort_unstable();
        cuts.dedup();

        let mut spans = Vec::new();
        let mut runs = Vec::new();
        for w in cuts.windows(2) {
            let (a, b) = (w[0], w[1]);
            if a == b {
                continue;
            }
            let seg = segs.iter().find(|s| s.start <= a && b <= s.end);
            let mut color = seg.map_or(TEXT_PRIMARY, |s| s.color);
            let bold = seg.is_some_and(|s| s.bold);
            let ambiguous = seg
                .and_then(|s| s.link_target.as_deref())
                .is_some_and(|t| self.ambiguous.contains(&t.to_lowercase()));
            let in_sel = selection.is_some_and(|(s, e)| s <= a && b <= e);
            let bg = if current.is_some_and(|(s, e)| s <= a && b <= e) {
                color = EDITOR_SEARCH_CURRENT_TEXT;
                Bg::Current
            } else if in_sel {
                if color == TEXT_GUTTER {
                    color = TEXT_SECONDARY;
                }
                Bg::Selection
            } else if matches.iter().any(|&(s, e)| s <= a && b <= e) {
                Bg::Match
            } else {
                Bg::None
            };
            spans.push((chars[a..b].iter().collect(), color, bold));
            runs.push(Run { bg, ambiguous });
        }
        if n == 0 {
            // An empty selected line still shows one cell of selection, as in Vim.
            let bg = if selection.is_some() {
                Bg::Selection
            } else {
                Bg::None
            };
            spans.push((" ".to_owned(), TEXT_PRIMARY, false));
            runs.push(Run {
                bg,
                ambiguous: false,
            });
        }
        (spans, runs)
    }
}

fn paragraph<P: text::Paragraph<Font = Font>>(spans: &[(String, Color, bool)], width: f32) -> P {
    let spans: Vec<Span<'_, (), Font>> = spans
        .iter()
        .map(|(t, c, bold)| {
            let font = if *bold {
                Font {
                    weight: Weight::Bold,
                    ..MONO
                }
            } else {
                MONO
            };
            Span::new(t.as_str()).color(*c).font(font)
        })
        .collect();
    P::with_spans(Text {
        content: spans.as_slice(),
        bounds: Size::new(width, f32::INFINITY),
        size: Pixels(EDITOR_TEXT_SIZE),
        line_height: LineHeight::Absolute(Pixels(EDITOR_LINE_HEIGHT)),
        font: MONO,
        align_x: Alignment::Left,
        align_y: Vertical::Top,
        shaping: Shaping::Advanced,
        wrapping: Wrapping::WordOrGlyph,
    })
}

fn small_text(content: String, bounds: Size, align_x: Alignment, bold: bool) -> Text<String, Font> {
    Text {
        content,
        bounds,
        size: Pixels(EDITOR_TEXT_SIZE),
        line_height: LineHeight::Absolute(Pixels(EDITOR_LINE_HEIGHT)),
        font: if bold {
            Font {
                weight: Weight::Bold,
                ..MONO
            }
        } else {
            MONO
        },
        align_x,
        align_y: Vertical::Top,
        shaping: Shaping::Basic,
        wrapping: Wrapping::None,
    }
}

impl<'a, Message, Theme, R> Widget<Message, Theme, R> for Editor<'a>
where
    R: text::Renderer<Font = Font> + renderer::Renderer,
{
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Fill)
    }

    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State<R::Paragraph>>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::<R::Paragraph> {
            lines: RefCell::new(HashMap::new()),
        })
    }

    fn layout(
        &mut self,
        _tree: &mut widget::Tree,
        _renderer: &R,
        limits: &layout::Limits,
    ) -> layout::Node {
        layout::Node::new(limits.max())
    }

    fn draw(
        &self,
        tree: &widget::Tree,
        renderer: &mut R,
        _theme: &Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();
        let state = tree.state.downcast_ref::<State<R::Paragraph>>();
        let mut cache = state.lines.borrow_mut();
        let col = Self::column(bounds);
        let text_w = Self::text_width(bounds);
        let text_x = col.x + GUTTER_WIDTH;
        let clip = bounds.intersection(viewport).unwrap_or(bounds);

        renderer.fill_quad(
            Quad {
                bounds,
                ..Quad::default()
            },
            BG_APP,
        );

        let cur = self.vim.cursor;
        let mut y = col.y;
        let mut i = self.top;
        let mut cursor_paint: Option<(Rectangle, Option<char>, Color)> = None;
        // Lines that left the screen are evicted, so memory stays bounded.
        cache.retain(|&k, _| k >= self.top && k < self.top + 200);

        while i < self.lines.len() && y < bounds.y + bounds.height {
            let (spans, runs) = self.runs(i);
            let mut h = DefaultHasher::new();
            for (t, c, b) in &spans {
                t.hash(&mut h);
                c.r.to_bits().hash(&mut h);
                c.g.to_bits().hash(&mut h);
                b.hash(&mut h);
            }
            for r in &runs {
                (r.bg as u8, r.ambiguous).hash(&mut h);
            }
            text_w.to_bits().hash(&mut h);
            let key = h.finish();
            let entry = cache.entry(i).or_insert_with(|| Cached {
                key: !key,
                paragraph: paragraph(&[], 1.0),
                runs: Vec::new(),
            });
            if entry.key != key {
                entry.paragraph = paragraph(&spans, text_w);
                entry.runs = runs;
                entry.key = key;
            }
            let p = &entry.paragraph;
            let height = p.min_bounds().height.max(EDITOR_LINE_HEIGHT);
            let origin = Point::new(text_x, y);

            // Item 3: full-width background for the cursor line.
            if i == cur.line {
                renderer.fill_quad(
                    Quad {
                        bounds: Rectangle {
                            x: col.x,
                            y,
                            width: col.width,
                            height,
                        },
                        border: Border {
                            radius: RADIUS_CURSORLINE.into(),
                            ..Border::default()
                        },
                        ..Quad::default()
                    },
                    BG_CURSORLINE,
                );
            }

            // Item 2: per-run backgrounds; item 4: wavy underline for ambiguous links.
            for (idx, run) in entry.runs.iter().enumerate() {
                let color = match run.bg {
                    Bg::None => None,
                    Bg::Selection => Some(EDITOR_SELECTION),
                    Bg::Match => Some(EDITOR_SEARCH_MATCH),
                    Bg::Current => Some(EDITOR_SEARCH_CURRENT),
                };
                if color.is_none() && !run.ambiguous {
                    continue;
                }
                for r in p.span_bounds(idx) {
                    let r = Rectangle {
                        x: r.x + origin.x,
                        y: r.y + origin.y,
                        ..r
                    };
                    if let Some(color) = color {
                        renderer.fill_quad(
                            Quad {
                                bounds: r,
                                ..Quad::default()
                            },
                            color,
                        );
                    }
                    if run.ambiguous {
                        wavy_underline(renderer, r, STATUS_WARNING);
                    }
                }
            }

            // Item 6: relative numbers, drawn on the first visual row of each line.
            let number = if i == cur.line || !self.relative_numbers {
                i + 1
            } else {
                i.abs_diff(cur.line)
            };
            renderer.fill_text(
                small_text(
                    number.to_string(),
                    Size::new(GUTTER_WIDTH - 12.0, EDITOR_LINE_HEIGHT),
                    Alignment::Right,
                    i == cur.line,
                ),
                Point::new(col.x + GUTTER_WIDTH - 12.0, y),
                if i == cur.line {
                    TEXT_PRIMARY
                } else {
                    TEXT_GUTTER
                },
                clip,
            );

            renderer.fill_paragraph(p, origin, TEXT_PRIMARY, clip);

            // Item 5: block cursor in normal and visual mode, 2 px line in insert mode.
            if i == cur.line && self.vim.mode != Mode::Search {
                let pos = p.grapheme_position(0, cur.col).unwrap_or(Point::ORIGIN);
                let cell = cell_width(p, cur.col, pos.x);
                let at = Point::new(origin.x + pos.x, origin.y + pos.y);
                let ch = self.lines[i].chars().nth(cur.col);
                cursor_paint = Some(match self.vim.mode {
                    Mode::Insert => (
                        Rectangle {
                            x: at.x,
                            y: at.y,
                            width: CURSOR_INSERT_WIDTH,
                            height: EDITOR_LINE_HEIGHT,
                        },
                        None,
                        MODE_INSERT,
                    ),
                    Mode::Visual => (
                        Rectangle {
                            x: at.x,
                            y: at.y,
                            width: cell,
                            height: EDITOR_LINE_HEIGHT,
                        },
                        ch,
                        EDITOR_CURSOR_VISUAL,
                    ),
                    _ => (
                        Rectangle {
                            x: at.x,
                            y: at.y,
                            width: cell,
                            height: EDITOR_LINE_HEIGHT,
                        },
                        ch,
                        MODE_NORMAL,
                    ),
                });
            }

            y += height;
            i += 1;
        }

        if let Some((rect, ch, color)) = cursor_paint {
            // A separate layer so the cursor and the glyph under it cover the text below.
            renderer.with_layer(clip, |r| {
                r.fill_quad(
                    Quad {
                        bounds: rect,
                        ..Quad::default()
                    },
                    color,
                );
                if let Some(ch) = ch.filter(|c| !c.is_whitespace()) {
                    r.fill_text(
                        small_text(
                            ch.to_string(),
                            Size::new(rect.width * 2.0, rect.height),
                            Alignment::Left,
                            false,
                        ),
                        rect.position(),
                        TEXT_ON_ACCENT,
                        clip,
                    );
                }
            });
        }

        *self.drawn_at.borrow_mut() = Some(Instant::now());
    }
}

/// Width of the character cell at `col`, from the next grapheme's position.
fn cell_width<P: text::Paragraph>(p: &P, col: usize, x: f32) -> f32 {
    match p.grapheme_position(0, col + 1) {
        Some(next) if next.x > x => next.x - x,
        _ => EDITOR_TEXT_SIZE * 0.6,
    }
}

/// Draws a zigzag line under `r` with small quads (item 4).
fn wavy_underline<R: renderer::Renderer>(renderer: &mut R, r: Rectangle, color: Color) {
    let base = r.y + r.height - 4.0;
    let step = 2.0;
    let mut x = r.x;
    let mut k = 0u32;
    while x < r.x + r.width {
        let dy = match k % 4 {
            0 => 0.0,
            1 | 3 => 1.0,
            _ => 2.0,
        };
        renderer.fill_quad(
            Quad {
                bounds: Rectangle {
                    x,
                    y: base + dy,
                    width: step,
                    height: 1.0,
                },
                ..Quad::default()
            },
            color,
        );
        x += step;
        k += 1;
    }
}

impl<'a, Message: 'a, Theme: 'a, R> From<Editor<'a>> for Element<'a, Message, Theme, R>
where
    R: text::Renderer<Font = Font> + renderer::Renderer + 'a,
{
    fn from(editor: Editor<'a>) -> Self {
        Element::new(editor)
    }
}
