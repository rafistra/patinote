//! The bufferline (F-28, mockup 01): the Patinote mark, then the open notes,
//! numbered, with a dot when a note has unsaved changes.

use iced::widget::{Space, button, column, container, row, text};
use iced::{Alignment, Element, Length, Padding};

use crate::theme::styles;
use crate::theme::tokens::*;
use crate::widgets::banner::Icon;
use crate::widgets::{icon, typed};

/// One open note in the bufferline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tab {
    pub title: String,
    /// Unsaved changes: a `●` after the title.
    pub modified: bool,
    /// Insert mode: the dot takes `mode.insert` (0003 section 3.3).
    pub insert: bool,
}

/// The bufferline. The mark is shown when notes are open (mockup 01); the
/// screens without notes show the name alone (mockups 10, 11).
pub fn view<'a, Message: Clone + 'a>(
    tabs: &[Tab],
    active: Option<usize>,
    on_select: impl Fn(usize) -> Message,
) -> Element<'a, Message> {
    let name = typed("Patinote", TYPE_UI)
        .font(FONT_UI_SEMIBOLD)
        .color(TEXT_STRONG);
    let brand: Element<'a, Message> = if tabs.is_empty() {
        name.into()
    } else {
        row![icon(Icon::Logo, SIZE_LOGO, ACCENT), name]
            .spacing(SPACE_8)
            .align_y(Alignment::Center)
            .into()
    };
    let mut line = row![
        container(brand)
            .padding(Padding::from([0.0, SPACE_14]))
            .center_y(Length::Fill)
    ]
    .height(SIZE_BUFFERLINE_H);

    for (i, tab) in tabs.iter().enumerate() {
        let is_active = active == Some(i);
        let mut label = row![typed(format!("{} {}", i + 1, tab.title), TYPE_MONO_UI)]
            .spacing(SPACE_8)
            .align_y(Alignment::Center);
        if tab.modified {
            let dot = if tab.insert && is_active {
                MODE_INSERT
            } else {
                ACCENT
            };
            label = label.push(text("●").font(FONT_MONO).size(TYPE_MONO_UI.size).color(dot));
        }
        let underline: Element<'a, Message> = if is_active {
            container(Space::new().width(Length::Fill).height(SIZE_FOCUS_OUTLINE))
                .style(styles::accent_bar)
                .into()
        } else {
            Space::new().height(SIZE_FOCUS_OUTLINE).into()
        };
        let face = column![
            container(label)
                .padding(Padding::from([0.0, SPACE_14]))
                .center_y(SIZE_BUFFERLINE_H - SIZE_FOCUS_OUTLINE),
            underline,
        ]
        .width(Length::Shrink);
        line = line.push(
            button(face)
                .padding(0)
                .height(Length::Fill)
                .on_press(on_select(i))
                .style(styles::buffer_tab(is_active)),
        );
    }
    container(line)
        .width(Length::Fill)
        .height(SIZE_BUFFERLINE_H)
        .style(styles::chrome)
        .into()
}
