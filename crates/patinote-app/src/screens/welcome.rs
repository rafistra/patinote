//! The screens before a vault is open (F-01, mockup 10): the welcome card
//! on first run, and "Vault not found" when the last vault is gone.

use iced::widget::{Space, button, column, container, rich_text, row, span};
use iced::{Alignment, Element, Length, Padding};

use crate::theme::styles;
use crate::theme::tokens::*;
use crate::widgets::banner::Icon;
use crate::widgets::{icon, label, typed};

/// Which card to show.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Card {
    /// No vault yet.
    Welcome,
    /// The last vault is missing; the path as shown (`~/Vault`).
    Missing { path: String },
}

/// The messages a card's buttons send.
#[derive(Debug, Clone)]
pub struct Actions<Message> {
    pub choose: Message,
    pub retry: Message,
}

/// Button text with its key in mono, like `Choose folder… Enter`.
fn with_key<'a, Message: 'a>(text: &'a str, key: &'a str, primary: bool) -> Element<'a, Message> {
    let key_color = if primary { TEXT_ON_ACCENT } else { TEXT_SUBTLE };
    let rich: iced::widget::text::Rich<'a, (), Message> = rich_text![
        span(text).font(if primary { FONT_UI_SEMIBOLD } else { FONT_UI }),
        span(" "),
        span(key).font(FONT_MONO).color(key_color),
    ]
    .size(TYPE_UI.size);
    container(rich).center_y(Length::Fill).into()
}

fn paragraph<'a, Message: 'a>(text: &'a str, color: iced::Color) -> Element<'a, Message> {
    typed(text, TYPE_UI).color(color).into()
}

/// The card itself, `size.welcome.w` wide.
pub fn card<'a, Message: Clone + 'a>(
    card: &Card,
    actions: &Actions<Message>,
) -> Element<'a, Message> {
    let body: Element<'a, Message> = match card {
        Card::Welcome => {
            let title = row![
                icon(Icon::Logo, SIZE_LOGO_LARGE, ACCENT),
                typed("Welcome to Patinote", TYPE_TITLE).color(TEXT_STRONG),
            ]
            .spacing(SPACE_10)
            .align_y(Alignment::Center);
            let where_index: iced::widget::text::Rich<'a, (), Message> = rich_text![
                span("Patinote doesn't change or add files when it opens a vault. Its index lives in "),
                span("~/.cache/patinote/")
                    .font(FONT_MONO)
                    .color(SYNTAX_CODE),
                span("."),
            ]
            .font(TYPE_UI.font)
            .size(TYPE_UI.size)
            .line_height(iced::widget::text::LineHeight::Absolute(TYPE_UI.line_height.into()))
            .color(TEXT_MUTED);
            let choose = button(with_key("Choose folder…", "Enter", true))
                .padding(Padding::from([0.0, SPACE_14]))
                .height(SIZE_CONTROL_H_LARGE)
                .on_press(actions.choose.clone())
                .style(styles::button_primary(false));
            column![
                title,
                Space::new().height(SPACE_4),
                paragraph(
                    "Pick a folder of Markdown notes to use as your vault. An Obsidian vault works as it is.",
                    TEXT_SECONDARY,
                ),
                where_index,
                Space::new().height(SPACE_14),
                row![
                    choose,
                    typed("Opens the system folder picker", TYPE_UI_SMALL).color(TEXT_MUTED),
                ]
                .spacing(SPACE_10)
                .align_y(Alignment::Center),
            ]
            .spacing(SPACE_8)
            .into()
        }
        Card::Missing { path } => {
            let title = row![
                icon(Icon::Warning, SIZE_ICON, STATUS_WARNING),
                typed("Vault not found", TYPE_TITLE).color(TEXT_STRONG),
            ]
            .spacing(SPACE_10)
            .align_y(Alignment::Center);
            let retry = button(with_key("Try again", "r", false))
                .padding(Padding::from([0.0, SPACE_14]))
                .height(SIZE_CONTROL_H_LARGE)
                .on_press(actions.retry.clone())
                .style(styles::button_secondary(false));
            let choose = button(with_key("Choose another folder…", "Enter", true))
                .padding(Padding::from([0.0, SPACE_14]))
                .height(SIZE_CONTROL_H_LARGE)
                .on_press(actions.choose.clone())
                .style(styles::button_primary(false));
            column![
                title,
                Space::new().height(SPACE_4),
                paragraph("The last vault you opened is no longer there:", TEXT_SECONDARY),
                container(typed(path.clone(), TYPE_MONO_UI).color(TEXT_STRONG))
                    .width(Length::Fill)
                    .padding(Padding::from([SPACE_8, SPACE_12]))
                    .style(styles::path_box),
                paragraph(
                    "It may have been moved or renamed, or it's on a drive that isn't mounted. Nothing was deleted by Patinote.",
                    TEXT_MUTED,
                ),
                Space::new().height(SPACE_14),
                row![retry, choose].spacing(SPACE_10),
            ]
            .spacing(SPACE_8)
            .into()
        }
    };
    container(body)
        .width(SIZE_WELCOME_W)
        .padding(PAD_DIALOG)
        .style(styles::overlay(RADIUS_DIALOG))
        .into()
}

/// The body of the screen: one card, centered, 96 px from the top.
pub fn view<'a, Message: Clone + 'a>(c: &Card, actions: &Actions<Message>) -> Element<'a, Message> {
    container(card(c, actions))
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(Alignment::Center)
        .padding(Padding {
            top: SPACE_48 + SPACE_48,
            ..Padding::from(SPACE_32)
        })
        .into()
}

/// Both cards side by side with their labels, as drawn in mockup 10; used
/// by the UI snapshot test only.
pub fn sheet<'a, Message: Clone + 'a>(
    welcome: &Card,
    missing: &Card,
    actions: &Actions<Message>,
) -> Element<'a, Message> {
    let labeled = |title: &str, c| {
        column![label(title).color(TEXT_SUBTLE), card(c, actions)].spacing(SPACE_10)
    };
    container(
        row![
            labeled("First run · no vault yet", welcome),
            labeled("Last vault missing", missing),
        ]
        .spacing(SPACE_48),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .align_x(Alignment::Center)
    .padding(Padding {
        top: SPACE_48 + SPACE_48,
        ..Padding::from(SPACE_32)
    })
    .into()
}
