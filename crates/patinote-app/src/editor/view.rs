//! The editor widget (F-05, F-06, F-08; docs/design/0002 sections 3 and 6),
//! built on `iced::advanced`: one paragraph per source line with colored
//! spans, backgrounds for the visual selection and search matches from
//! `span_bounds`, a wavy underline under ambiguous links, a full-width cursor
//! line, block and line cursors in token colors, and relative line numbers
//! aligned with soft-wrapped lines. A popup (link suggestions, the ambiguous
//! link picker) can hang under the cursor as an overlay.
//!
//! It only draws. Keys go to the Vim engine through the app; the widget
//! reports clicks (with whether Ctrl was held) and wheel scrolling (as the
//! range of visible lines, so the app can keep the cursor on screen).

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::hash::{DefaultHasher, Hash, Hasher};

use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer::{self, Quad};
use iced::advanced::text::{self, Paragraph as _, Span, Text};
use iced::advanced::widget::{Tree, Widget, tree};
use iced::advanced::{Clipboard, Shell, mouse, overlay};
use iced::alignment::Vertical;
use iced::keyboard::{self, Modifiers};
use iced::widget::text::{Alignment, LineHeight, Shaping, Wrapping};
use iced::{Border, Color, Element, Event, Font, Length, Pixels, Point, Rectangle, Size, Vector};

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
pub struct Editor<'a, Message, Theme = iced::Theme, R = iced::Renderer> {
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
    /// Link targets (lowercase) that match more than one note.
    pub ambiguous: Option<&'a HashSet<String>>,
    pub relative_numbers: bool,
    /// The widest the text column gets (920 px in minimal mode).
    pub max_width: f32,
    /// Text size and line height (`type.editor`, or `type.focus`).
    pub ty: Type,
    /// Show line numbers (hidden in focus mode, F-28).
    pub gutter: bool,
    /// Space above the first line.
    pub top_pad: f32,
    /// Marked on each draw during `--bench`.
    pub drawn: Option<crate::bench::DrawClock>,
    popup: Option<(usize, Element<'a, Message, Theme, R>)>,
    footer: Option<Element<'a, Message, Theme, R>>,
    on_click: Option<Box<dyn Fn(At, bool) -> Message + 'a>>,
    on_scroll: Option<Box<dyn Fn(usize, usize) -> Message + 'a>>,
}

impl<'a, Message, Theme, R> Editor<'a, Message, Theme, R> {
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
            ambiguous: None,
            relative_numbers: true,
            max_width: SIZE_EDITOR_MAXW,
            ty: TYPE_EDITOR,
            gutter: true,
            top_pad: PAD_EDITOR_TOP,
            drawn: None,
            popup: None,
            footer: None,
            on_click: None,
            on_scroll: None,
        }
    }

    /// The message for a click at a text position; `true` when Ctrl was
    /// held (Ctrl+click follows a link, F-08).
    pub fn on_click(mut self, f: impl Fn(At, bool) -> Message + 'a) -> Self {
        self.on_click = Some(Box::new(f));
        self
    }

    /// The message after the wheel scrolled: first and last visible line.
    pub fn on_scroll(mut self, f: impl Fn(usize, usize) -> Message + 'a) -> Self {
        self.on_scroll = Some(Box::new(f));
        self
    }

    /// Shows `content` just under the cursor line, its left edge at
    /// character `col` of that line.
    pub fn popup(mut self, col: usize, content: impl Into<Element<'a, Message, Theme, R>>) -> Self {
        self.popup = Some((col, content.into()));
        self
    }

    /// Shows `content` under the note's last line, in the text column (the
    /// backlinks of mockup 01).
    pub fn footer(mut self, content: impl Into<Element<'a, Message, Theme, R>>) -> Self {
        self.footer = Some(content.into());
        self
    }

    /// The children in tree order: footer, then popup.
    fn kids(&self) -> Vec<&Element<'a, Message, Theme, R>> {
        self.footer
            .iter()
            .chain(self.popup.iter().map(|(_, p)| p))
            .collect()
    }

    fn gutter_w(&self) -> f32 {
        if self.gutter { SIZE_GUTTER_W } else { 0.0 }
    }

    fn column(&self, bounds: Rectangle) -> Rectangle {
        let width = (bounds.width - PAD_EDITOR_SIDE - PAD_EDITOR_SIDE)
            .clamp(SIZE_GUTTER_W + SIZE_GUTTER_W, self.max_width);
        Rectangle {
            x: f32::midpoint(bounds.x, bounds.x + bounds.width - width),
            y: bounds.y + self.top_pad,
            width,
            height: (bounds.height - self.top_pad).max(0.0),
        }
    }

    /// Splits line `i` into colored spans and their decorations.
    fn runs(&self, i: usize) -> (Vec<(String, Color, bool)>, Vec<Deco>) {
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
        let mut decos = Vec::new();
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
                .zip(self.ambiguous)
                .is_some_and(|(t, set)| set.contains(&t.trim().to_lowercase()));
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
            decos.push(Deco { bg, ambiguous });
        }
        if n == 0 {
            // An empty selected line still shows one cell, as in Vim.
            spans.push((" ".to_owned(), TEXT_PRIMARY, false));
            decos.push(Deco {
                bg: if selection.is_some() {
                    Bg::Selection
                } else {
                    Bg::None
                },
                ambiguous: false,
            });
        }
        (spans, decos)
    }

    /// Builds (or reuses) the paragraph of line `i`; returns its height.
    fn entry<P: text::Paragraph<Font = Font>>(
        &self,
        i: usize,
        width: f32,
        cache: &mut HashMap<usize, Cached<P>>,
    ) -> f32 {
        let (spans, decos) = self.runs(i);
        let mut h = DefaultHasher::new();
        for (t, c, b) in &spans {
            t.hash(&mut h);
            c.r.to_bits().hash(&mut h);
            c.g.to_bits().hash(&mut h);
            c.b.to_bits().hash(&mut h);
            b.hash(&mut h);
        }
        decos.hash(&mut h);
        width.to_bits().hash(&mut h);
        let key = h.finish();
        let entry = cache.entry(i).or_insert_with(|| Cached {
            key: !key,
            paragraph: paragraph(&[], width, self.ty),
            decos: Vec::new(),
        });
        if entry.key != key {
            entry.paragraph = paragraph(&spans, width, self.ty);
            entry.decos = decos;
            entry.key = key;
        }
        entry.paragraph.min_bounds().height.max(self.ty.line_height)
    }

    /// Scrolls to keep the cursor visible (unless the wheel moved the view)
    /// and remembers where the cursor and the popup anchor are, relative to
    /// the widget. Runs in `layout`, so overlays know it the same frame.
    fn prepare<P: text::Paragraph<Font = Font>>(&self, state: &State<P>, size: Size) {
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
        let count = self.lines.len();
        if count == 0 {
            return;
        }
        let col = self.column(Rectangle::new(Point::ORIGIN, size));
        let text_w = (col.width - self.gutter_w()).max(SIZE_EDITOR_CELL_W);
        let cur = self.cursor.0.min(count - 1);
        if state.follow.get() {
            let mut top = state.top.get().min(cur);
            loop {
                let used: f32 = (top..=cur).map(|i| self.entry(i, text_w, &mut cache)).sum();
                if used <= col.height || top == cur {
                    break;
                }
                top += 1;
            }
            state.top.set(top);
        }
        let top = state.top.get().min(count - 1);
        let mut y = col.y;
        for i in top..cur {
            y += self.entry(i, text_w, &mut cache);
        }
        self.entry(cur, text_w, &mut cache);
        let anchor = self.popup.as_ref().map_or(self.cursor.1, |(c, _)| *c);
        let line_h = self.ty.line_height;
        let at = |c: usize| {
            cache.get(&cur).map_or(Point::ORIGIN, |e| {
                char_position(&e.paragraph, &self.lines[cur], c, line_h)
            })
        };
        let n = self.lines[cur].chars().count();
        let pos = at(self.cursor.1.min(n));
        let anchor_pos = at(anchor.min(n));
        let text_x = col.x + self.gutter_w();
        state.cursor_rect.set(Some(Rectangle {
            x: text_x + pos.x,
            y: y + pos.y,
            width: SIZE_EDITOR_CELL_W,
            height: self.ty.line_height,
        }));
        // Where the note ends, if that is on screen: the footer goes there.
        let mut end = col.y;
        let mut last_visible = true;
        for i in top..count {
            end += self.entry(i, text_w, &mut cache);
            if end > size.height {
                last_visible = false;
                break;
            }
        }
        state.footer_y.set(last_visible.then_some(end));
        // The popup hangs under the cursor's visual row, from the anchor.
        state.anchor.set(Some(Point::new(
            text_x + anchor_pos.x,
            y + pos.y.max(anchor_pos.y) + self.ty.line_height,
        )));
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

/// How a run of text is decorated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct Deco {
    bg: Bg,
    /// Under a link that matches more than one note (F-08, mockup 12).
    ambiguous: bool,
}

struct Cached<P> {
    key: u64,
    paragraph: P,
    decos: Vec<Deco>,
}

/// Paragraphs cached per line, rebuilt only when the line, its decorations
/// or the column width change; where the view is scrolled to; and where the
/// cursor is.
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
    /// The cursor cell and the popup's top-left, relative to the widget.
    cursor_rect: Cell<Option<Rectangle>>,
    anchor: Cell<Option<Point>>,
    /// Where the note's last line ends, relative to the widget, when it is
    /// on screen.
    footer_y: Cell<Option<f32>>,
    modifiers: Cell<Modifiers>,
}

fn editor_font(bold: bool) -> Font {
    if bold { FONT_MONO_BOLD } else { FONT_MONO }
}

fn paragraph<P: text::Paragraph<Font = Font>>(
    spans: &[(String, Color, bool)],
    width: f32,
    ty: Type,
) -> P {
    let spans: Vec<Span<'_, (), Font>> = spans
        .iter()
        .map(|(t, c, bold)| Span::new(t.as_str()).color(*c).font(editor_font(*bold)))
        .collect();
    P::with_spans(Text {
        content: spans.as_slice(),
        bounds: Size::new(width, f32::INFINITY),
        size: Pixels(ty.size),
        line_height: LineHeight::Absolute(Pixels(ty.line_height)),
        font: FONT_MONO,
        align_x: Alignment::Left,
        align_y: Vertical::Top,
        shaping: Shaping::Advanced,
        wrapping: Wrapping::WordOrGlyph,
    })
}

/// Where character `c` of `text` sits in its paragraph, relative to the
/// paragraph's top left. `grapheme_position` only looks at one visual row
/// and counts from that row's start, so the row holding `c` is found first.
fn char_position<P: text::Paragraph<Font = Font>>(
    p: &P,
    text: &str,
    c: usize,
    line_h: f32,
) -> Point {
    let rows = ((p.min_bounds().height / line_h).round() as usize).max(1);
    let (mut row, mut start) = (0, 0);
    for k in 1..rows {
        match row_start(p, text, k, line_h) {
            Some(s) if s <= c && s > start => (row, start) = (k, s),
            _ => break,
        }
    }
    let y = row as f32 * line_h;
    p.grapheme_position(row, c - start)
        .map_or(Point::new(0.0, y), |at| Point::new(at.x, y + at.y))
}

/// The first character of visual row `row`.
fn row_start<P: text::Paragraph<Font = Font>>(
    p: &P,
    text: &str,
    row: usize,
    line_h: f32,
) -> Option<usize> {
    // Just above the row's bottom edge: inside the row.
    let y = (row as f32 + 1.0) * line_h - 1.0;
    p.hit_test(Point::new(-1.0, y))
        .map(|hit| char_index(text, hit.cursor()))
}

/// The character of `text` under `point`, relative to the paragraph.
fn char_at<P: text::Paragraph<Font = Font>>(p: &P, text: &str, point: Point) -> usize {
    p.hit_test(point)
        .map_or(0, |hit| char_index(text, hit.cursor()))
}

/// Hits are byte offsets; the editor counts characters.
fn char_index(text: &str, byte: usize) -> usize {
    text.char_indices().take_while(|(i, _)| *i < byte).count()
}

fn cell_text(
    content: String,
    bounds: Size,
    align_x: Alignment,
    bold: bool,
    ty: Type,
) -> Text<String, Font> {
    Text {
        content,
        bounds,
        size: Pixels(ty.size),
        line_height: LineHeight::Absolute(Pixels(ty.line_height)),
        font: editor_font(bold),
        align_x,
        align_y: Vertical::Top,
        shaping: Shaping::Basic,
        wrapping: Wrapping::None,
    }
}

/// A zigzag line under `r`, drawn with small quads (0003 3.5).
fn wavy_underline<R: renderer::Renderer>(renderer: &mut R, r: Rectangle, color: Color) {
    let base = r.y + r.height - SIZE_WAVE_OFFSET;
    let mut x = r.x;
    let mut k = 0u32;
    while x < r.x + r.width {
        let dy = match k % 4 {
            0 => 0.0,
            2 => SIZE_WAVE_STEP,
            _ => SIZE_BORDER,
        };
        renderer.fill_quad(
            Quad {
                bounds: Rectangle {
                    x,
                    y: base + dy,
                    width: SIZE_WAVE_STEP,
                    height: SIZE_BORDER,
                },
                ..Quad::default()
            },
            color,
        );
        x += SIZE_WAVE_STEP;
        k += 1;
    }
}

impl<Message, Theme, R> Widget<Message, Theme, R> for Editor<'_, Message, Theme, R>
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
            cursor_rect: Cell::new(None),
            anchor: Cell::new(None),
            footer_y: Cell::new(None),
            modifiers: Cell::new(Modifiers::default()),
        })
    }

    fn children(&self) -> Vec<Tree> {
        self.kids()
            .into_iter()
            .map(|k| Tree::new(k.as_widget()))
            .collect()
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(&self.kids());
    }

    fn layout(&mut self, tree: &mut Tree, renderer: &R, limits: &layout::Limits) -> layout::Node {
        let size = limits.max();
        let state = tree.state.downcast_ref::<State<R::Paragraph>>();
        self.prepare(state, size);
        let footer_y = state.footer_y.get();
        let col = self.column(Rectangle::new(Point::ORIGIN, size));
        let text_w = (col.width - self.gutter_w()).max(SIZE_EDITOR_CELL_W);
        let Some(footer) = &mut self.footer else {
            return layout::Node::new(size);
        };
        let child_limits = layout::Limits::new(Size::ZERO, Size::new(text_w, f32::INFINITY));
        let node = footer
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, &child_limits);
        // Off the screen when the note's end is not visible.
        let y = footer_y.map_or(size.height + size.height, |y| y + SPACE_32);
        layout::Node::with_children(
            size,
            vec![node.move_to(Point::new(col.x + self.gutter_w(), y))],
        )
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _renderer: &R,
        _clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        _viewport: &Rectangle,
    ) {
        if let (Some(footer), Some(child)) = (&mut self.footer, layout.children().next())
            && tree
                .state
                .downcast_ref::<State<R::Paragraph>>()
                .footer_y
                .get()
                .is_some()
        {
            footer.as_widget_mut().update(
                &mut tree.children[0],
                event,
                child,
                cursor,
                _renderer,
                _clipboard,
                shell,
                _viewport,
            );
            if shell.is_event_captured() {
                return;
            }
        }
        let state = tree.state.downcast_ref::<State<R::Paragraph>>();
        if let Event::Keyboard(keyboard::Event::ModifiersChanged(m)) = event {
            state.modifiers.set(*m);
            return;
        }
        let bounds = layout.bounds();
        let Some(at) = cursor.position_in(bounds) else {
            return;
        };
        match event {
            Event::Mouse(mouse::Event::WheelScrolled { delta }) => {
                let lines = match delta {
                    mouse::ScrollDelta::Lines { y, .. } => -*y * SCROLL_LINES_PER_NOTCH,
                    mouse::ScrollDelta::Pixels { y, .. } => -*y / self.ty.line_height,
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
                let ctrl = state.modifiers.get().control();
                let col = self.column(bounds);
                let text_x = col.x + self.gutter_w();
                let point = Point::new(bounds.x + at.x, bounds.y + at.y);
                let cache = state.lines.borrow();
                let mut y = col.y;
                let (first, last) = state.visible.get();
                for i in first..=last.min(self.lines.len().saturating_sub(1)) {
                    let Some(entry) = cache.get(&i) else { break };
                    let h = entry.paragraph.min_bounds().height.max(self.ty.line_height);
                    if point.y < y + h {
                        let local = Point::new((point.x - text_x).max(0.0), point.y - y);
                        let col = char_at(&entry.paragraph, &self.lines[i], local);
                        shell.publish(f((i, col), ctrl));
                        shell.capture_event();
                        return;
                    }
                    y += h;
                }
                let last_line = self.lines.len().saturating_sub(1);
                shell.publish(f((last_line, 0), ctrl));
                shell.capture_event();
            }
            _ => {}
        }
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &R,
    ) -> mouse::Interaction {
        if let (Some(footer), Some(child)) = (&self.footer, layout.children().next())
            && cursor.is_over(child.bounds())
        {
            return footer.as_widget().mouse_interaction(
                &tree.children[0],
                child,
                cursor,
                viewport,
                renderer,
            );
        }
        if cursor.is_over(layout.bounds()) {
            mouse::Interaction::Text
        } else {
            mouse::Interaction::None
        }
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut R,
        _theme: &Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        if let Some(clock) = &self.drawn {
            clock.mark();
        }
        let bounds = layout.bounds();
        let state = tree.state.downcast_ref::<State<R::Paragraph>>();
        let mut cache = state.lines.borrow_mut();
        let col = self.column(bounds);
        let text_w = (col.width - self.gutter_w()).max(SIZE_EDITOR_CELL_W);
        let text_x = col.x + self.gutter_w();
        let clip = bounds.intersection(viewport).unwrap_or(bounds);
        let bottom = bounds.y + bounds.height;
        let line_h = self.ty.line_height;
        let count = self.lines.len();
        let cur = (self.cursor.0.min(count.saturating_sub(1)), self.cursor.1);

        // No background fill: the editor sits on the window's `bg.app`
        // (0002 section 11).
        let top = state.top.get().min(count.saturating_sub(1));
        let mut y = col.y;
        let mut i = top;
        let mut cursor_paint: Option<(Rectangle, Option<char>, Color)> = None;
        while i < count && y < bottom {
            let height = self.entry(i, text_w, &mut cache);
            // Only whole lines, as in Vim: a cut line's text would draw
            // past the editor (tiny-skia does not clip it). A line taller
            // than the editor still shows from its top.
            if i != top && y + height > bottom {
                break;
            }
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

            for (idx, deco) in entry.decos.iter().enumerate() {
                let color = match deco.bg {
                    Bg::None => None,
                    Bg::Selection => Some(EDITOR_SELECTION),
                    Bg::Match => Some(EDITOR_SEARCH_MATCH),
                    Bg::Current => Some(EDITOR_SEARCH_CURRENT),
                };
                if color.is_none() && !deco.ambiguous {
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
                    if deco.ambiguous {
                        wavy_underline(renderer, r, EDITOR_LINK_AMBIGUOUS);
                    }
                }
            }

            // Relative numbers on the first visual row of each line; the
            // cursor line shows its real number (F-05).
            let number = if i == cur.0 || !self.relative_numbers {
                i + 1
            } else {
                i.abs_diff(cur.0)
            };
            if self.gutter {
                renderer.fill_text(
                    cell_text(
                        number.to_string(),
                        Size::new(SIZE_GUTTER_W - SPACE_14, line_h),
                        Alignment::Right,
                        i == cur.0,
                        self.ty,
                    ),
                    Point::new(col.x + SIZE_GUTTER_W - SPACE_14, y),
                    if i == cur.0 { TEXT_STRONG } else { TEXT_GUTTER },
                    clip,
                );
            }

            renderer.fill_paragraph(p, origin, TEXT_PRIMARY, clip);

            if i == cur.0 && self.shape != Shape::Hidden {
                let n = self.lines[i].chars().count();
                let c = cur.1.min(n);
                let pos = char_position(p, &self.lines[i], c, line_h);
                let at = Point::new(origin.x + pos.x, origin.y + pos.y);
                let next = char_position(p, &self.lines[i], (c + 1).min(n), line_h);
                let cell = if next.x > pos.x && next.y == pos.y {
                    next.x - pos.x
                } else {
                    SIZE_EDITOR_CELL_W
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
        if let (Some(footer), Some(child)) = (&self.footer, layout.children().next())
            && state.footer_y.get().is_some()
        {
            footer.as_widget().draw(
                &tree.children[0],
                renderer,
                _theme,
                _style,
                child,
                _cursor,
                &clip,
            );
        }
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
                            self.ty,
                        ),
                        rect.position(),
                        TEXT_ON_ACCENT,
                        clip,
                    );
                }
            });
        }
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        _renderer: &R,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, Theme, R>> {
        let index = usize::from(self.footer.is_some());
        let (_, content) = self.popup.as_mut()?;
        let state = tree.state.downcast_ref::<State<R::Paragraph>>();
        let anchor = state.anchor.get()?;
        let bounds = layout.bounds();
        let child = tree.children.get_mut(index)?;
        Some(overlay::Element::new(Box::new(Popup {
            content,
            tree: child,
            position: Point::new(bounds.x + anchor.x, bounds.y + anchor.y) + translation,
            line_height: self.ty.line_height,
            viewport: *viewport,
        })))
    }
}

/// The popup under the cursor.
struct Popup<'a, 'b, Message, Theme, R> {
    content: &'b mut Element<'a, Message, Theme, R>,
    tree: &'b mut Tree,
    /// Top-left under the cursor line.
    position: Point,
    line_height: f32,
    viewport: Rectangle,
}

impl<Message, Theme, R> overlay::Overlay<Message, Theme, R> for Popup<'_, '_, Message, Theme, R>
where
    R: renderer::Renderer,
{
    fn layout(&mut self, renderer: &R, bounds: Size) -> layout::Node {
        let limits = layout::Limits::new(Size::ZERO, bounds);
        let node = self
            .content
            .as_widget_mut()
            .layout(self.tree, renderer, &limits);
        let size = node.size();
        // Below the cursor line, or above it when there is no room below;
        // never past the window's right edge.
        let x = self.position.x.min(bounds.width - size.width).max(0.0);
        let y = if self.position.y + size.height <= bounds.height {
            self.position.y
        } else {
            (self.position.y - self.line_height - size.height).max(0.0)
        };
        node.move_to(Point::new(x, y))
    }

    fn draw(
        &self,
        renderer: &mut R,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
    ) {
        self.content.as_widget().draw(
            self.tree,
            renderer,
            theme,
            style,
            layout,
            cursor,
            &self.viewport,
        );
    }

    fn update(
        &mut self,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &R,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
    ) {
        self.content.as_widget_mut().update(
            self.tree,
            event,
            layout,
            cursor,
            renderer,
            clipboard,
            shell,
            &self.viewport,
        );
    }

    fn mouse_interaction(
        &self,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &R,
    ) -> mouse::Interaction {
        self.content.as_widget().mouse_interaction(
            self.tree,
            layout,
            cursor,
            &self.viewport,
            renderer,
        )
    }
}

impl<'a, Message: 'a, Theme: 'a, R> From<Editor<'a, Message, Theme, R>>
    for Element<'a, Message, Theme, R>
where
    R: text::Renderer<Font = Font> + renderer::Renderer + 'a,
{
    fn from(editor: Editor<'a, Message, Theme, R>) -> Self {
        Element::new(editor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    type P = iced::advanced::graphics::text::Paragraph;

    fn laid_out(text: &str, width: f32) -> P {
        paragraph(
            &[(text.to_owned(), TEXT_PRIMARY, false)],
            width,
            TYPE_EDITOR,
        )
    }

    #[test]
    fn f05_the_cursor_on_a_wrapped_row_is_on_that_row() {
        let text = "satu dua tiga empat lima enam tujuh delapan sembilan sepuluh";
        let lh = TYPE_EDITOR.line_height;
        // Narrow enough that the line wraps.
        let p = laid_out(text, f32::from(160u16));
        assert!(p.min_bounds().height > lh, "the line wraps");
        assert_eq!(char_position(&p, text, 0, lh), Point::ORIGIN);
        let end = char_position(&p, text, text.chars().count(), lh);
        assert!(end.y >= lh, "the end is on a later row: {end:?}");
        // Each character is drawn where clicking it lands.
        for c in 0..text.chars().count() {
            let at = char_position(&p, text, c, lh);
            let hit = char_at(&p, text, Point::new(at.x + 1.0, at.y + lh - 1.0));
            assert_eq!(hit, c, "character {c} at {at:?}");
        }
    }

    #[test]
    fn f05_clicking_after_non_ascii_text_lands_on_the_character_clicked() {
        let text = "é—ü—é abc";
        let lh = TYPE_EDITOR.line_height;
        let p = laid_out(text, SIZE_EDITOR_MAXW);
        for c in 0..text.chars().count() {
            let at = char_position(&p, text, c, lh);
            let hit = char_at(&p, text, Point::new(at.x + 1.0, lh - 1.0));
            assert_eq!(hit, c, "character {c}");
        }
    }

    #[test]
    fn matches_are_found_without_overlap() {
        let chars: Vec<char> = "indeks dan indeks".chars().collect();
        assert_eq!(matches_in(&chars, "indeks"), [(0, 6), (11, 17)]);
        assert_eq!(matches_in(&chars, ""), []);
        let chars: Vec<char> = "aaa".chars().collect();
        assert_eq!(matches_in(&chars, "aa"), [(0, 2)]);
    }
}
