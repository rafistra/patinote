//! The leader popup (F-28, mockup 02): shown 300 ms after Space when no
//! other key followed, a grid of the keys that may come next.

use iced::widget::{Column, Row, container, rich_text, span};
use iced::{Element, Length, Padding};

use crate::theme::styles;
use crate::theme::tokens::*;
use crate::widgets::typed;

/// The keys after the leader in Phase 2 (F-28), in the mockup's order.
pub const KEYS: [(&str, &str); 9] = [
    ("f", "find note"),
    ("n", "new note"),
    ("e", "explorer"),
    ("b", "backlinks"),
    ("o", "outline"),
    ("r", "reading mode"),
    ("c", "conflicts"),
    ("x", "close buffer"),
    ("l", "switch layout"),
];

/// The popup, `size.leader.w` wide.
pub fn view<'a, Message: 'a>() -> Element<'a, Message> {
    let mut grid = Column::new().spacing(SPACE_6);
    for chunk in KEYS.chunks(4) {
        let mut line = Row::new().spacing(SPACE_16);
        for (key, what) in chunk {
            let item: iced::widget::text::Rich<'a, (), Message> = rich_text![
                span(*key).font(FONT_MONO_BOLD).color(ACCENT),
                span(" ").font(FONT_MONO),
                span(*what).font(FONT_MONO).color(TEXT_PRIMARY),
            ]
            .size(TYPE_MONO_UI.size);
            line = line.push(container(item).width(Length::FillPortion(1)));
        }
        for _ in chunk.len()..4 {
            line = line.push(container("").width(Length::FillPortion(1)));
        }
        grid = grid.push(line);
    }
    container(
        Column::new()
            .spacing(SPACE_10)
            .push(typed("<leader> = Space", TYPE_MONO_UI).color(TEXT_SUBTLE))
            .push(grid),
    )
    .width(SIZE_LEADER_W)
    .padding(Padding::from([SPACE_14, SPACE_16]))
    .style(styles::leader_popup)
    .into()
}
