//! A note's text, read-only, in the editor column. A stand-in until the
//! editor of step 2.3 replaces it; it already uses the editor's column,
//! padding and type (0003 section 5).

use iced::widget::{container, scrollable};
use iced::{Alignment, Element, Length, Padding};

use crate::theme::tokens::*;
use crate::widgets::typed;

/// Draws `text` in the editor column.
pub fn view<'a, Message: 'a>(text: &'a str) -> Element<'a, Message> {
    let body = container(typed(text, TYPE_EDITOR).color(TEXT_PRIMARY))
        .max_width(SIZE_EDITOR_MAXW)
        .width(Length::Fill);
    scrollable(
        container(body)
            .width(Length::Fill)
            .align_x(Alignment::Center)
            .padding(Padding {
                top: PAD_EDITOR_TOP,
                right: PAD_EDITOR_SIDE,
                bottom: PAD_EDITOR_TOP,
                left: PAD_EDITOR_SIDE,
            }),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}
