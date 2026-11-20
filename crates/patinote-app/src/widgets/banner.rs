//! Banners under the bufferline: warning, error and info (0003 section 3.7,
//! mockup 17). They push the editor down instead of covering it.

use iced::widget::{Space, button, column, container, rich_text, row, span, svg};
use iced::{Alignment, Element, Length, Padding};

use crate::theme::styles::{self, BannerKind};
use crate::theme::tokens::*;
use crate::widgets::typed;

/// The icon at the start of a banner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    Warning,
    Error,
    Lock,
    Info,
    /// The Patinote mark.
    Logo,
}

impl Icon {
    /// The SVG file.
    pub fn bytes(self) -> &'static [u8] {
        match self {
            Self::Warning => include_bytes!("../../assets/icons/warning.svg"),
            Self::Error => include_bytes!("../../assets/icons/error.svg"),
            Self::Lock => include_bytes!("../../assets/icons/lock.svg"),
            Self::Info => include_bytes!("../../assets/icons/info.svg"),
            Self::Logo => include_bytes!("../../assets/icons/logo.svg"),
        }
    }
}

/// A button on a banner.
#[derive(Debug, Clone)]
pub struct Action<Message> {
    pub label: String,
    /// The main action: filled, and placed rightmost.
    pub primary: bool,
    pub on_press: Message,
}

/// A banner: a bold first sentence, then an explanation.
#[derive(Debug, Clone)]
pub struct Banner<Message> {
    pub kind: BannerKind,
    pub icon: Icon,
    pub title: String,
    pub body: String,
    /// A command to show on its own line, like the `sysctl` fix.
    pub code: Option<String>,
    pub actions: Vec<Action<Message>>,
}

impl<Message: Clone + 'static> Banner<Message> {
    /// Draws the banner at full width.
    pub fn view(&self) -> Element<'_, Message> {
        let c = self.kind.colors();
        let icon = svg(svg::Handle::from_memory(self.icon.bytes()))
            .width(SIZE_ICON)
            .height(SIZE_ICON)
            .style(styles::icon(c.icon));

        let message: iced::widget::text::Rich<'_, (), Message> = rich_text![
            span(self.title.as_str())
                .font(FONT_UI_SEMIBOLD)
                .color(c.text),
            span(" "),
            span(self.body.as_str()).color(c.muted),
        ]
        .font(TYPE_UI.font)
        .size(TYPE_UI.size)
        .line_height(iced::widget::text::LineHeight::Absolute(
            TYPE_UI.line_height.into(),
        ));

        let mut text_col = column![message].spacing(SPACE_8);
        if let Some(code) = &self.code {
            text_col = text_col.push(
                container(typed(code.as_str(), TYPE_MONO_UI).color(TEXT_STRONG))
                    .padding(Padding::from([SPACE_2, SPACE_8]))
                    .style(styles::banner_code(self.kind)),
            );
        }

        // Secondary buttons first, the main action rightmost (0003 section 6).
        let mut buttons = row![].spacing(SPACE_12).align_y(Alignment::Center);
        let ordered = self
            .actions
            .iter()
            .filter(|a| !a.primary)
            .chain(self.actions.iter().filter(|a| a.primary));
        for a in ordered {
            let label = typed(a.label.as_str(), TYPE_UI).font(if a.primary {
                FONT_UI_SEMIBOLD
            } else {
                FONT_UI
            });
            let b = button(container(label).center_y(Length::Fill))
                .height(SIZE_CONTROL_H)
                .padding(Padding::from([0.0, SPACE_14]))
                .on_press(a.on_press.clone());
            buttons = buttons.push(if a.primary {
                b.style(styles::banner_action(self.kind))
            } else {
                b.style(styles::banner_button(self.kind))
            });
        }

        let body = row![icon, container(text_col).width(Length::Fill), buttons]
            .spacing(SPACE_12)
            .align_y(Alignment::Center);
        column![
            container(body)
                .width(Length::Fill)
                .padding(Padding::from([SPACE_12, SPACE_20]))
                .style(styles::banner(self.kind)),
            container(Space::new().width(Length::Fill).height(SIZE_BORDER))
                .style(styles::banner_rule(self.kind)),
        ]
        .into()
    }
}
