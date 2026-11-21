//! Command line messages (0003 section 6): info in muted text, warnings
//! start with `W:`, errors with `E:`.

use iced::widget::container;
use iced::{Element, Length, Padding};

use crate::theme::styles::{self, Level};
use crate::theme::tokens::*;
use crate::widgets::typed;

/// How long an error message stays at least, in seconds; other messages
/// go away on the next key press.
pub const ERROR_MIN_SECONDS: u64 = 5;

/// The text of a message, with its `W:` or `E:` prefix.
pub fn message_text(level: Level, text: &str) -> String {
    match level {
        Level::Info => text.to_owned(),
        Level::Warning => format!("W: {text}"),
        Level::Error => format!("E: {text}"),
    }
}

/// One command line row showing a message.
pub fn message<'a, Message: 'a>(level: Level, text: &str) -> Element<'a, Message> {
    container(typed(message_text(level, text), TYPE_MONO_UI).color(level.color()))
        .width(Length::Fill)
        .height(SIZE_CMDLINE_H)
        .padding(Padding::from([0.0, SPACE_12]))
        .center_y(SIZE_CMDLINE_H)
        .style(styles::cmdline)
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefixes_follow_0003() {
        assert_eq!(
            message_text(Level::Info, "\"A.md\" updated from disk"),
            "\"A.md\" updated from disk"
        );
        assert_eq!(
            message_text(Level::Warning, "2 notes are named \"Rapat\""),
            "W: 2 notes are named \"Rapat\""
        );
        assert_eq!(
            message_text(Level::Error, "Can't save: the disk is full"),
            "E: Can't save: the disk is full"
        );
    }
}
