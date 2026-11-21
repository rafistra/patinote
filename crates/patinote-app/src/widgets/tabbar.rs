//! The tab bar of classic mode (F-28, mockup 03): the mark and vault name,
//! then each open note as a tab with a close button; the active tab sits on
//! `bg.app` with a 2 px `accent` line on top.

use iced::widget::{Space, button, column, container, row, text};
use iced::{Alignment, Element, Length, Padding};

use crate::theme::styles;
use crate::theme::tokens::*;
use crate::widgets::banner::Icon;
use crate::widgets::bufferline::Tab;
use crate::widgets::{icon, typed};

/// The tab bar, `size.tabbar.h` high.
pub fn view<'a, Message: Clone + 'a>(
    vault: &str,
    tabs: &[Tab],
    active: Option<usize>,
    on_select: impl Fn(usize) -> Message,
    on_close: impl Fn(usize) -> Message,
) -> Element<'a, Message> {
    let brand = row![
        icon(Icon::Logo, SIZE_LOGO, ACCENT),
        typed("Patinote", TYPE_UI)
            .font(FONT_UI_SEMIBOLD)
            .color(TEXT_STRONG),
        typed(format!("· {vault}"), TYPE_UI_SMALL).color(TEXT_SUBTLE),
    ]
    .spacing(SPACE_8)
    .align_y(Alignment::Center);
    let mut bar = row![
        container(brand)
            .width(SIZE_EXPLORER_W)
            .padding(Padding::from([0.0, SPACE_16]))
            .center_y(Length::Fill)
    ]
    .height(SIZE_TABBAR_H);
    for (i, tab) in tabs.iter().enumerate() {
        let is_active = active == Some(i);
        let mut label = row![typed(tab.title.clone(), TYPE_UI).color(if is_active {
            TEXT_STRONG
        } else {
            TEXT_MUTED
        })]
        .spacing(SPACE_10)
        .align_y(Alignment::Center);
        if tab.modified {
            label = label.push(text("●").size(TYPE_UI_SMALL.size).color(
                if tab.insert && is_active {
                    MODE_INSERT
                } else {
                    ACCENT
                },
            ));
        }
        label = label.push(
            button(typed("×", TYPE_UI).color(TEXT_SUBTLE))
                .padding(0)
                .on_press(on_close(i))
                .style(styles::statusline_button(false)),
        );
        let line: Element<'a, Message> = if is_active {
            container(Space::new().width(Length::Fill).height(SIZE_FOCUS_OUTLINE))
                .style(styles::accent_bar)
                .into()
        } else {
            Space::new().height(SIZE_FOCUS_OUTLINE).into()
        };
        let face = column![
            line,
            container(label)
                .padding(Padding::from([0.0, SPACE_14]))
                .center_y(SIZE_TABBAR_H - SIZE_FOCUS_OUTLINE)
        ]
        .width(Length::Shrink);
        bar = bar.push(
            button(face)
                .padding(0)
                .height(Length::Fill)
                .on_press(on_select(i))
                .style(styles::classic_tab(is_active)),
        );
    }
    container(bar)
        .width(Length::Fill)
        .height(SIZE_TABBAR_H)
        .style(styles::chrome)
        .into()
}
