//! Reusable pieces of the UI, drawn only with [`crate::theme`].

pub mod banner;
pub mod cmdline;
pub mod statusline;

use iced::widget::{Text, text};

use crate::theme::tokens::{TYPE_LABEL, Type};

/// Text set in a typography token.
pub fn typed<'a>(content: impl text::IntoFragment<'a>, ty: Type) -> Text<'a> {
    text(content)
        .font(ty.font)
        .size(ty.size)
        .line_height(text::LineHeight::Absolute(ty.line_height.into()))
}

/// A section label (`type.label`): uppercase. Iced 0.14 cannot space
/// letters, so the 0.8 px tracking of 0003 is not applied.
pub fn label<'a>(content: &str) -> Text<'a> {
    typed(content.to_uppercase(), TYPE_LABEL)
}
