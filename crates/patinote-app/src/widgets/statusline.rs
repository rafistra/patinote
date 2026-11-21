//! The status line (F-28; 0003 sections 5 and 6; mockups 01, 11, 17): mode
//! badge, path, notes (word and backlink counts), the index indicator, the
//! conflict count, panel buttons and `line:column`.

use iced::widget::{Space, button, container, rich_text, row, span};
use iced::{Alignment, Color, Element, Length, Padding};

use crate::mode::Mode;
use crate::theme::styles;
use crate::theme::tokens::*;
use crate::widgets::typed;

/// The indicator at the right end of the status line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Indicator {
    /// The index is up to date.
    Indexed,
    /// Indexing is running (F-03).
    Indexing { done: usize, total: usize },
    /// The watcher is off and the vault is polled (F-04).
    Polling { seconds: u64 },
    /// Saving keeps failing (F-05).
    NotSaved,
}

impl Indicator {
    /// (glyph, glyph color, text, text color).
    fn parts(&self) -> (&'static str, Color, String, Color) {
        match self {
            Self::Indexed => ("●", STATUS_OK, "indexed".into(), TEXT_MUTED),
            Self::Indexing { done, total } => (
                "◌",
                ACCENT,
                format!("indexing {} / {}", thousands(*done), thousands(*total)),
                TEXT_SECONDARY,
            ),
            Self::Polling { seconds } => (
                "⚠",
                STATUS_WARNING,
                format!("polling every {seconds} s"),
                STATUS_WARNING,
            ),
            Self::NotSaved => ("✕", STATUS_ERROR, "not saved".into(), STATUS_ERROR),
        }
    }
}

/// `1240` → `1,240`.
pub fn thousands(n: usize) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, d) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(d);
    }
    out
}

/// A panel button at the right of the status line, like `␣e explorer`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusButton {
    pub label: &'static str,
    /// Its panel is open.
    pub on: bool,
}

/// What the status line shows.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StatusLine {
    pub mode: Mode,
    /// The note's path; empty when no note is open (`no note open`).
    pub path: String,
    /// Unsaved changes: `[+]` after the path.
    pub modified: bool,
    /// Muted notes after the path, like `186 words` or `read-only`.
    pub notes: Vec<String>,
    pub indicator: Option<Indicator>,
    /// Syncthing conflicts (F-11); shown only when there are some.
    pub conflicts: usize,
    /// Panel buttons. With them, the indicator follows the notes instead of
    /// sitting at the right end (mockup 01).
    pub buttons: Vec<StatusButton>,
    /// `line:column` of the cursor.
    pub position: Option<String>,
    /// A muted note at the right, before the position (`layout: classic`).
    pub right_note: Option<String>,
}

fn segment<'a, Message: 'a>(
    content: impl Into<Element<'a, Message>>,
) -> container::Container<'a, Message> {
    container(content)
        .padding(Padding::from([0.0, SPACE_12]))
        .center_y(Length::Fill)
}

fn indicator_segment<'a, Message: 'a>(indicator: &Indicator) -> container::Container<'a, Message> {
    let (glyph, glyph_color, text, text_color) = indicator.parts();
    let rich: iced::widget::text::Rich<'a, (), Message> = rich_text![
        span(glyph).color(glyph_color),
        span(" "),
        span(text).color(text_color),
    ]
    .font(TYPE_MONO_UI.font)
    .size(TYPE_MONO_UI.size);
    segment(rich)
}

impl StatusLine {
    /// Draws the status line at full width; its buttons do nothing.
    pub fn view<'a, Message: Clone + 'a>(&self) -> Element<'a, Message> {
        self.view_with(|_| None)
    }

    /// Draws the status line; `on_button` gives the message of each button.
    pub fn view_with<'a, Message: Clone + 'a>(
        &self,
        on_button: impl Fn(usize) -> Option<Message>,
    ) -> Element<'a, Message> {
        let badge = segment(typed(self.mode.label(), TYPE_MONO_UI).font(FONT_MONO_BOLD))
            .style(styles::mode_badge(self.mode));
        let path: Element<'a, Message> = if self.path.is_empty() {
            segment(typed("no note open", TYPE_MONO_UI).color(TEXT_MUTED)).into()
        } else {
            let path = if self.modified {
                format!("{} [+]", self.path)
            } else {
                self.path.clone()
            };
            segment(typed(path, TYPE_MONO_UI))
                .style(styles::statusline_segment)
                .into()
        };
        let mut line = row![badge, path]
            .align_y(Alignment::Center)
            .height(SIZE_STATUSLINE_H);
        for note in &self.notes {
            line = line.push(segment(typed(note.clone(), TYPE_MONO_UI).color(TEXT_MUTED)));
        }
        // With buttons the indicator follows the notes (mockup 01);
        // otherwise it sits at the right end (mockups 11, 17).
        let inline = !self.buttons.is_empty();
        if inline && let Some(indicator) = &self.indicator {
            line = line.push(indicator_segment(indicator));
        }
        if self.conflicts > 0 {
            let label = if self.conflicts == 1 {
                "⚠ 1 conflict".to_owned()
            } else {
                format!("⚠ {} conflicts", self.conflicts)
            };
            line = line.push(segment(typed(label, TYPE_MONO_UI).color(STATUS_WARNING)));
        }
        line = line.push(Space::new().width(Length::Fill));
        if !inline && let Some(indicator) = &self.indicator {
            line = line.push(indicator_segment(indicator));
        }
        for (i, b) in self.buttons.iter().enumerate() {
            let mut btn = button(container(typed(b.label, TYPE_MONO_UI)).center_y(Length::Fill))
                .padding(Padding::from([0.0, SPACE_12]))
                .height(Length::Fill)
                .style(styles::statusline_button(b.on));
            if let Some(m) = on_button(i) {
                btn = btn.on_press(m);
            }
            line = line.push(container(btn).center_y(Length::Fill));
        }
        if let Some(note) = &self.right_note {
            line = line.push(segment(typed(note.clone(), TYPE_MONO_UI).color(TEXT_MUTED)));
        }
        if let Some(pos) = &self.position {
            line = line
                .push(segment(typed(pos.clone(), TYPE_MONO_UI)).style(styles::statusline_segment));
        }
        container(line)
            .width(Length::Fill)
            .height(SIZE_STATUSLINE_H)
            .style(styles::statusline)
            .into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thousands_are_grouped_like_the_mockup() {
        assert_eq!(thousands(0), "0");
        assert_eq!(thousands(999), "999");
        assert_eq!(thousands(1_240), "1,240");
        assert_eq!(thousands(1_234_567), "1,234,567");
    }
}
