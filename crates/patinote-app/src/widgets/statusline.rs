//! The status line (0003 sections 5 and 6, mockup 17): mode badge, path,
//! optional notes, and one indicator on the right.

use iced::widget::{Space, container, rich_text, row, span};
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

/// What the status line shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusLine {
    pub mode: Mode,
    pub path: String,
    /// Unsaved changes: `[+]` after the path.
    pub modified: bool,
    /// Muted notes after the path, like `read-only`.
    pub notes: Vec<String>,
    pub indicator: Option<Indicator>,
}

fn segment<'a, Message: 'a>(
    content: impl Into<Element<'a, Message>>,
) -> container::Container<'a, Message> {
    container(content)
        .padding(Padding::from([0.0, SPACE_12]))
        .center_y(Length::Fill)
}

impl StatusLine {
    /// Draws the status line at full width.
    pub fn view<'a, Message: 'a>(&'a self) -> Element<'a, Message> {
        let badge = segment(typed(self.mode.label(), TYPE_MONO_UI).font(FONT_MONO_BOLD))
            .style(styles::mode_badge(self.mode));
        let path = if self.modified {
            format!("{} [+]", self.path)
        } else {
            self.path.clone()
        };
        let mut line = row![
            badge,
            segment(typed(path, TYPE_MONO_UI)).style(styles::statusline_segment)
        ]
        .align_y(Alignment::Center)
        .height(SIZE_STATUSLINE_H);
        for note in &self.notes {
            line = line.push(segment(
                typed(note.as_str(), TYPE_MONO_UI).color(TEXT_MUTED),
            ));
        }
        line = line.push(Space::new().width(Length::Fill));
        if let Some(indicator) = &self.indicator {
            let (glyph, glyph_color, text, text_color) = indicator.parts();
            let indicator: iced::widget::text::Rich<'a, (), Message> = rich_text![
                span(glyph).color(glyph_color),
                span(" "),
                span(text).color(text_color),
            ]
            .font(TYPE_MONO_UI.font)
            .size(TYPE_MONO_UI.size);
            line = line.push(segment(indicator));
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
