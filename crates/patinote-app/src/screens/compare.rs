//! Two versions of a note side by side (F-11, mockups 08 and 16): lines
//! only on one side are marked in that side's color, never by color alone:
//! each side has a title, and a legend explains the colors (0003 3.7).

use iced::widget::{Column, Space, column, container, row, scrollable};
use iced::{Alignment, Color, Element, Length, Padding};

use crate::diff::{Line, Sides};
use crate::theme::styles;
use crate::theme::tokens::*;
use crate::widgets::{label, typed};

/// The color of one side: orange for what came from outside (the disk, a
/// Syncthing copy), blue for the user's own version or the original.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Disk,
    Mine,
}

impl Side {
    fn colors(self) -> (Color, Color) {
        match self {
            Self::Disk => (DIFF_DISK_BG, DIFF_DISK_MARKER),
            Self::Mine => (DIFF_MINE_BG, DIFF_MINE_MARKER),
        }
    }
}

fn column_view<'a, Message: 'a>(title: String, lines: &[Line], side: Side) -> Element<'a, Message> {
    let (bg, marker) = side.colors();
    let mut list = Column::new();
    for l in lines {
        let number = typed(l.number.to_string(), TYPE_EDITOR)
            .color(if l.only_here { marker } else { TEXT_GUTTER })
            .width(SIZE_GUTTER_W - SPACE_12);
        let text = typed(l.text.clone(), TYPE_EDITOR).color(TEXT_PRIMARY);
        let line = row![number, text]
            .spacing(SPACE_12)
            .align_y(Alignment::Center);
        let cell = container(line)
            .width(Length::Fill)
            .padding(Padding::from([0.0, SPACE_20]));
        list = list.push(if l.only_here {
            cell.style(styles::diff_line(bg))
        } else {
            cell
        });
    }
    column![
        container(label(&title).color(TEXT_SUBTLE)).padding(Padding {
            top: SPACE_24,
            right: SPACE_20,
            bottom: SPACE_14,
            left: SPACE_20,
        }),
        scrollable(list).height(Length::Fill),
    ]
    .width(Length::FillPortion(1))
    .into()
}

fn legend_box<'a, Message: 'a>(side: Side) -> Element<'a, Message> {
    let (bg, marker) = side.colors();
    container(Space::new().width(SPACE_12).height(SPACE_12))
        .style(styles::legend(bg, marker))
        .into()
}

/// The two columns and the legend.
#[allow(clippy::too_many_arguments)]
pub fn view<'a, Message: 'a>(
    sides: &Sides,
    left: (String, Side),
    right: (String, Side),
    legend: (&str, &str),
    explanation: &str,
) -> Element<'a, Message> {
    let body = row![
        column_view(left.0, &sides.left, left.1),
        container(Space::new().width(SIZE_BORDER).height(Length::Fill)).style(styles::rule),
        column_view(right.0, &sides.right, right.1),
    ]
    .height(Length::Fill);
    let foot = row![
        legend_box(left.1),
        typed(legend.0.to_owned(), TYPE_UI_SMALL).color(TEXT_SECONDARY),
        Space::new().width(SPACE_8),
        legend_box(right.1),
        typed(legend.1.to_owned(), TYPE_UI_SMALL).color(TEXT_SECONDARY),
        Space::new().width(SPACE_8),
        typed(explanation.to_owned(), TYPE_UI_SMALL).color(TEXT_MUTED),
    ]
    .spacing(SPACE_8)
    .align_y(Alignment::Center);
    column![
        body,
        container(Space::new().width(Length::Fill).height(SIZE_BORDER)).style(styles::rule),
        container(foot).padding(Padding::from([SPACE_10, SPACE_20])),
    ]
    .into()
}
