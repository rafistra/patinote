//! The editor widget (F-05, F-06; docs/design/0002 sections 3 and 6), built
//! on `iced::advanced`: one paragraph per source line with colored spans,
//! backgrounds for the visual selection and search matches from
//! `span_bounds`, a full-width cursor line, block and line cursors in token
//! colors, and relative line numbers aligned with soft-wrapped lines.
//!
//! It only draws. Keys go to the Vim engine through the app; the widget
//! reports clicks (as a line and column) and wheel scrolling (as the range of
//! visible lines, so the app can keep the cursor on screen like Vim does).

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};

use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer::{self, Quad};
use iced::advanced::text::{self, Paragraph as _, Span, Text};
use iced::advanced::widget::{self, Widget, tree};
use iced::advanced::{Clipboard, Shell, mouse};
use iced::alignment::Vertical;
use iced::widget::text::{Alignment, LineHeight, Shaping, Wrapping};
use iced::{Border, Color, Element, Event, Font, Length, Pixels, Point, Rectangle, Size};

use super::highlight::{self, LineKind};
use crate::theme::tokens::*;

/// How the cursor is drawn.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Shape {
    /// A block over the character (normal and visual mode).
    Block(Color),
    /// A 2 px line before the character (insert mode).
    Line(Color),
    /// No cursor (read-only notes, a prompt has the focus).
    Hidden,
}

/// A position: line, and character within it.
pub type At = (usize, usize);

/// What the editor shows, borrowed from the app.
pub struct Editor<'a, Message> {
    /// Identifies the note: scrolling starts over when it changes.
    pub id: u64,
    pub lines: &'a [String],
    pub kinds: &'a [LineKind],
    pub cursor: At,
    pub shape: Shape,
    /// The visual selection, inclusive; for linewise selections the end
    /// column is `usize::MAX`.
    pub selection: Option<(At, At)>,
    /// The search pattern whose matches are highlighted.
    pub search: Option<&'a str>,
    pub relative_numbers: bool,
    /// The widest the text column gets (920 px in minimal mode).
    pub max_width: f32,
    on_click: Option<Box<dyn Fn(At) -> Message + 'a>>,
    on_scroll: Option<Box<dyn Fn(usize, usize) -> Message + 'a>>,
}

impl<'a, Message> Editor<'a, Message> {
    /// An editor over `lines`, which must have one [`LineKind`] each.
    pub fn new(
        id: u64,
        lines: &'a [String],
        kinds: &'a [LineKind],
        cursor: At,
        shape: Shape,
    ) -> Self {
        Self {
            id,
            lines,
            kinds,
            cursor,
            shape,
            selection: None,
            search: None,
            relative_numbers: true,
            max_width: SIZE_EDITOR_MAXW,
            on_click: None,
            on_scroll: None,
        }
    }

    /// The message for a click at a text position.
    pub fn on_click(mut self, f: impl Fn(At) -> Message + 'a) -> Self {
        self.on_click = Some(Box::new(f));
        self
    }

    /// The message after the wheel scrolled: first and last visible line.
    pub fn on_scroll(mut self, f: impl Fn(usize, usize) -> Message + 'a) -> Self {
        self.on_scroll = Some(Box::new(f));
        self
    }

    fn column(&self, bounds: Rectangle) -> Rectangle {
        let width = (bounds.width - PAD_EDITOR_SIDE - PAD_EDITOR_SIDE)
            .clamp(SIZE_GUTTER_W + SIZE_GUTTER_W, self.max_width);
        Rectangle {
            x: f32::midpoint(bounds.x, bounds.x + bounds.width - width),
            y: bounds.y + PAD_EDITOR_TOP,
            width,
            height: (bounds.height - PAD_EDITOR_TOP).max(0.0),
        }
    }

    /// Splits line `i` into colored spans and their backgrounds.
    fn runs(&self, i: usize) -> (Vec<(String, Color, bool)>, Vec<Bg>) {
        let text = &self.lines[i];
        let chars: Vec<char> = text.chars().collect();
        let n = chars.len();
        let kind = self.kinds.get(i).copied().unwrap_or(LineKind::Text);
        let segs = highlight::segments(text, kind);
        let matches = self
            .search
            .map(|p| matches_in(&chars, p))
            .unwrap_or_default();
        let current = matches
            .iter()
            .find(|(s, _)| self.cursor.0 == i && *s == self.cursor.1)
            .copied();
        let selection = self.selection.and_then(|(a, b)| {
            (a.0 <= i && i <= b.0).then(|| {
                let s = if a.0 == i { a.1.min(n) } else { 0 };
                let e = if b.0 == i {
                    b.1.saturating_add(1).min(n.max(1))
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
        let mut bgs = Vec::new();
        for w in cuts.windows(2) {
            let (a, b) = (w[0], w[1]);
            if a == b {
                continue;
            }
            let seg = segs.iter().find(|s| s.start <= a && b <= s.end);
            let mut color = seg.map_or(TEXT_PRIMARY, |s| s.color);
            let bold = seg.is_some_and(|s| s.bold);
            let bg = if current.is_some_and(|(s, e)| s <= a && b <= e) {
                color = EDITOR_SEARCH_CURRENT_TEXT;
                Bg::Current
            } else if selection.is_some_and(|(s, e)| s <= a && b <= e) {
                // Dim text stays readable on the selection (0003 3.5).
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
            bgs.push(bg);
        }
        if n == 0 {
            // An empty selected line still shows one cell, as in Vim.
            spans.push((" ".to_owned(), TEXT_PRIMARY, false));
            bgs.push(if selection.is_some() {
                Bg::Selection
            } else {
                Bg::None
            });
        }
        (spans, bgs)
    }
}

/// Every match of `pattern` in a line, as character ranges.
fn matches_in(chars: &[char], pattern: &str) -> Vec<(usize, usize)> {
    let pat: Vec<char> = pattern.chars().collect();
    if pat.is_empty() || pat.len() > chars.len() {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut i = 0;
    while i + pat.len() <= chars.len() {
        if chars[i..i + pat.len()] == pat[..] {
            out.push((i, i + pat.len()));
            i += pat.len();
        } else {
            i += 1;
        }
    }
    out
}

/// Background of a run of text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Bg {
    None,
    Selection,
    Match,
    Current,
}

struct Cached<P> {
    key: u64,
    paragraph: P,
    bgs: Vec<Bg>,
}

/// Paragraphs cached per line, rebuilt only when the line, its decorations
/// or the column width change; and where the view is scrolled to.
pub struct State<P> {
    lines: RefCell<HashMap<usize, Cached<P>>>,
    id: Cell<u64>,
    top: Cell<usize>,
    /// The cursor the last frame showed; when it moves, the view follows it.
    last_cursor: Cell<At>,
    /// False after the wheel scrolled, until the cursor moves.
    follow: Cell<bool>,
    /// First and last line drawn.
    visible: Cell<(usize, usize)>,
}

fn editor_font(bold: bool) -> Font {
    if bold { FONT_MONO_BOLD } else { FONT_MONO }
}

fn paragraph<P: text::Paragraph<Font = Font>>(spans: &[(String, Color, bool)], width: f32) -> P {
    let spans: Vec<Span<'_, (), Font>> = spans
        .iter()
        .map(|(t, c, bold)| Span::new(t.as_str()).color(*c).font(editor_font(*bold)))
        .collect();
    P::with_spans(Text {
        content: spans.as_slice(),
        bounds: Size::new(width, f32::INFINITY),
        size: Pixels(TYPE_EDITOR.size),
        line_height: LineHeight::Absolute(Pixels(TYPE_EDITOR.line_height)),
        font: FONT_MONO,
        align_x: Alignment::Left,
        align_y: Vertical::Top,
        shaping: Shaping::Advanced,
        wrapping: Wrapping::WordOrGlyph,
    })
}

fn cell_text(content: String, bounds: Size, align_x: Alignment, bold: bool) -> Text<String, Font> {
    Text {
        content,
        bounds,
        size: Pixels(TYPE_EDITOR.size),
        line_height: LineHeight::Absolute(Pixels(TYPE_EDITOR.line_height)),
        font: editor_font(bold),
        align_x,
        align_y: Vertical::Top,
        shaping: Shaping::Basic,
        wrapping: Wrapping::None,
    }
}

impl<Message, Theme, R> Widget<Message, Theme, R> for Editor<'_, Message>
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
            id: Cell::new(self.id),
            top: Cell::new(0),
            last_cursor: Cell::new(self.cursor),
            follow: Cell::new(true),
            visible: Cell::new((0, 0)),
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

    fn update(
        &mut self,
        tree: &mut widget::Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _renderer: &R,
        _clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        _viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();
        let Some(at) = cursor.position_in(bounds) else {
            return;
        };
        let state = tree.state.downcast_ref::<State<R::Paragraph>>();
        match event {
            Event::Mouse(mouse::Event::WheelScrolled { delta }) => {
                let lines = match delta {
                    mouse::ScrollDelta::Lines { y, .. } => -*y * SCROLL_LINES_PER_NOTCH,
                    mouse::ScrollDelta::Pixels { y, .. } => -*y / TYPE_EDITOR.line_height,
                };
                let steps = lines.round() as isize;
                if steps == 0 {
                    return;
                }
                let max = self.lines.len().saturating_sub(1) as isize;
                let top = (state.top.get() as isize + steps).clamp(0, max) as usize;
                state.top.set(top);
                state.follow.set(false);
                if let Some(f) = &self.on_scroll {
                    let (first, last) = state.visible.get();
                    let height = last.saturating_sub(first);
                    shell.publish(f(top, (top + height).min(max as usize)));
                }
                shell.capture_event();
                shell.request_redraw();
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let Some(f) = &self.on_click else { return };
                let col = self.column(bounds);
                let text_x = col.x + SIZE_GUTTER_W;
                let point = Point::new(bounds.x + at.x, bounds.y + at.y);
                let cache = state.lines.borrow();
                let mut y = col.y;
                let (first, last) = state.visible.get();
                for i in first..=last.min(self.lines.len().saturating_sub(1)) {
                    let Some(entry) = cache.get(&i) else { break };
                    let h = entry
                        .paragraph
                        .min_bounds()
                        .height
                        .max(TYPE_EDITOR.line_height);
                    if point.y < y + h {
                        let local = Point::new((point.x - text_x).max(0.0), point.y - y);
                        let col = entry
                            .paragraph
                            .hit_test(local)
                            .map_or(0, |hit| hit.cursor());
                        shell.publish(f((i, col)));
                        shell.capture_event();
                        return;
                    }
                    y += h;
                }
                let last_line = self.lines.len().saturating_sub(1);
                shell.publish(f((last_line, 0)));
                shell.capture_event();
            }
            _ => {}
        }
    }

    fn mouse_interaction(
        &self,
        _tree: &widget::Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _viewport: &Rectangle,
        _renderer: &R,
    ) -> mouse::Interaction {
        if cursor.is_over(layout.bounds()) {
            mouse::Interaction::Text
        } else {
            mouse::Interaction::None
        }
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
        if state.id.get() != self.id {
            state.id.set(self.id);
            state.top.set(0);
            state.follow.set(true);
            cache.clear();
        }
        if state.last_cursor.get() != self.cursor {
            state.last_cursor.set(self.cursor);
            state.follow.set(true);
        }
        let col = self.column(bounds);
        let text_w = (col.width - SIZE_GUTTER_W).max(SIZE_EDITOR_CELL_W);
        let text_x = col.x + SIZE_GUTTER_W;
        let clip = bounds.intersection(viewport).unwrap_or(bounds);
        let bottom = bounds.y + bounds.height;
        let line_h = TYPE_EDITOR.line_height;
        let count = self.lines.len();
        let cur = (self.cursor.0.min(count.saturating_sub(1)), self.cursor.1);

        renderer.fill_quad(
            Quad {
                bounds,
                ..Quad::default()
            },
            BG_APP,
        );

        let entry_for = |i: usize, cache: &mut HashMap<usize, Cached<R::Paragraph>>| -> f32 {
            let (spans, bgs) = self.runs(i);
            let mut h = DefaultHasher::new();
            for (t, c, b) in &spans {
                t.hash(&mut h);
                c.r.to_bits().hash(&mut h);
                c.g.to_bits().hash(&mut h);
                c.b.to_bits().hash(&mut h);
                b.hash(&mut h);
            }
            bgs.hash(&mut h);
            text_w.to_bits().hash(&mut h);
            let key = h.finish();
            let entry = cache.entry(i).or_insert_with(|| Cached {
                key: !key,
                paragraph: paragraph(&[], text_w),
                bgs: Vec::new(),
            });
            if entry.key != key {
                entry.paragraph = paragraph(&spans, text_w);
                entry.bgs = bgs;
                entry.key = key;
            }
            entry.paragraph.min_bounds().height.max(line_h)
        };

        // Keep the cursor on screen unless the wheel moved the view.
        if state.follow.get() && count > 0 {
            let mut top = state.top.get().min(cur.0);
            loop {
                let mut used = 0.0;
                for i in top..=cur.0 {
                    used += entry_for(i, &mut cache);
                }
                if used <= col.height || top == cur.0 {
                    break;
                }
                top += 1;
            }
            state.top.set(top);
        }
        let top = state.top.get().min(count.saturating_sub(1));

        let mut y = col.y;
        let mut i = top;
        let mut cursor_paint: Option<(Rectangle, Option<char>, Color)> = None;
        while i < count && y < bottom {
            let height = entry_for(i, &mut cache);
            let Some(entry) = cache.get(&i) else { break };
            let p = &entry.paragraph;
            let origin = Point::new(text_x, y);

            if i == cur.0 {
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

            for (idx, bg) in entry.bgs.iter().enumerate() {
                let color = match bg {
                    Bg::None => continue,
                    Bg::Selection => EDITOR_SELECTION,
                    Bg::Match => EDITOR_SEARCH_MATCH,
                    Bg::Current => EDITOR_SEARCH_CURRENT,
                };
                for r in p.span_bounds(idx) {
                    renderer.fill_quad(
                        Quad {
                            bounds: Rectangle {
                                x: r.x + origin.x,
                                y: r.y + origin.y,
                                ..r
                            },
                            ..Quad::default()
                        },
                        color,
                    );
                }
            }

            // Relative numbers on the first visual row of each line; the
            // cursor line shows its real number (F-05).
            let number = if i == cur.0 || !self.relative_numbers {
                i + 1
            } else {
                i.abs_diff(cur.0)
            };
            renderer.fill_text(
                cell_text(
                    number.to_string(),
                    Size::new(SIZE_GUTTER_W - SPACE_14, line_h),
                    Alignment::Right,
                    i == cur.0,
                ),
                Point::new(col.x + SIZE_GUTTER_W - SPACE_14, y),
                if i == cur.0 { TEXT_STRONG } else { TEXT_GUTTER },
                clip,
            );

            renderer.fill_paragraph(p, origin, TEXT_PRIMARY, clip);

            if i == cur.0 && self.shape != Shape::Hidden {
                let n = self.lines[i].chars().count();
                let c = cur.1.min(n);
                let pos = p.grapheme_position(0, c).unwrap_or(Point::ORIGIN);
                let at = Point::new(origin.x + pos.x, origin.y + pos.y);
                let cell = match p.grapheme_position(0, c + 1) {
                    Some(next) if next.x > pos.x && next.y == pos.y => next.x - pos.x,
                    _ => SIZE_EDITOR_CELL_W,
                };
                let ch = self.lines[i].chars().nth(c);
                cursor_paint = match self.shape {
                    Shape::Line(color) => Some((
                        Rectangle {
                            x: at.x,
                            y: at.y,
                            width: SIZE_CURSOR_INSERT_W,
                            height: line_h,
                        },
                        None,
                        color,
                    )),
                    Shape::Block(color) => Some((
                        Rectangle {
                            x: at.x,
                            y: at.y,
                            width: cell,
                            height: line_h,
                        },
                        ch,
                        color,
                    )),
                    Shape::Hidden => None,
                };
            }

            y += height;
            i += 1;
        }
        state.visible.set((top, i.saturating_sub(1)));
        // Lines far from the screen are dropped, so memory stays bounded.
        cache.retain(|&k, _| k + count.min(256) >= top && k <= i + 256);

        if let Some((rect, ch, color)) = cursor_paint {
            // Its own layer, so the cursor and the glyph on it cover the text.
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
                        cell_text(
                            ch.to_string(),
                            Size::new(rect.width + rect.width, rect.height),
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
    }
}

impl<'a, Message: 'a, Theme: 'a, R> From<Editor<'a, Message>> for Element<'a, Message, Theme, R>
where
    R: text::Renderer<Font = Font> + renderer::Renderer + 'a,
{
    fn from(editor: Editor<'a, Message>) -> Self {
        Element::new(editor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_are_found_without_overlap() {
        let chars: Vec<char> = "indeks dan indeks".chars().collect();
        assert_eq!(matches_in(&chars, "indeks"), [(0, 6), (11, 17)]);
        assert_eq!(matches_in(&chars, ""), []);
        let chars: Vec<char> = "aaa".chars().collect();
        assert_eq!(matches_in(&chars, "aa"), [(0, 2)]);
    }
}
