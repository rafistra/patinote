//! Iced widget styles, built only from [`super::tokens`] (0003 section 1).
//! Widgets call these functions and never build a style themselves.

use iced::widget::{button, container, svg, text_input};
use iced::{Background, Border, Color, Shadow, Theme};

use super::tokens::*;
use crate::mode::Mode;

/// The three banner kinds (0003 section 3.7).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BannerKind {
    /// Data could be lost if the user picks wrong.
    Warning,
    /// Something failed and Patinote is retrying.
    Error,
    /// A limitation to know about, without risk to data.
    Info,
}

/// Colors of one banner kind.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BannerColors {
    pub bg: Color,
    pub border: Color,
    pub text: Color,
    pub muted: Color,
    pub icon: Color,
    /// Background of the main action button.
    pub action: Color,
}

impl BannerKind {
    /// The banner's colors.
    pub fn colors(self) -> BannerColors {
        match self {
            Self::Warning => BannerColors {
                bg: BANNER_WARNING_BG,
                border: BANNER_WARNING_BORDER,
                text: BANNER_WARNING_TEXT,
                muted: BANNER_WARNING_MUTED,
                icon: STATUS_WARNING,
                action: STATUS_WARNING,
            },
            Self::Error => BannerColors {
                bg: BANNER_ERROR_BG,
                border: BANNER_ERROR_BORDER,
                text: BANNER_ERROR_TEXT,
                muted: BANNER_ERROR_MUTED,
                icon: STATUS_ERROR,
                action: STATUS_ERROR,
            },
            Self::Info => BannerColors {
                bg: BANNER_INFO_BG,
                border: BANNER_INFO_BORDER,
                text: BANNER_INFO_TEXT,
                muted: BANNER_INFO_MUTED,
                icon: STATUS_INFO,
                action: STATUS_INFO,
            },
        }
    }
}

/// Message levels on the command line (0003 section 6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Info,
    Warning,
    Error,
}

impl Level {
    /// The text color of a command line message.
    pub fn color(self) -> Color {
        match self {
            Self::Info => TEXT_MUTED,
            Self::Warning => STATUS_WARNING,
            Self::Error => STATUS_ERROR,
        }
    }
}

/// The badge color of a mode.
pub fn mode_color(mode: Mode) -> Color {
    match mode {
        Mode::Normal => MODE_NORMAL,
        Mode::Insert => MODE_INSERT,
        Mode::Visual => MODE_VISUAL,
        Mode::Read => MODE_READ,
        Mode::Conflict => MODE_CONFLICT,
    }
}

fn border(color: Color, width: f32, radius: f32) -> Border {
    Border {
        color,
        width,
        radius: radius.into(),
    }
}

fn filled(bg: Color, text: Color) -> container::Style {
    container::Style {
        text_color: Some(text),
        background: Some(Background::Color(bg)),
        border: Border::default(),
        shadow: Shadow::default(),
        snap: true,
    }
}

// --- Containers ---------------------------------------------------------------

/// The app background (`bg.app`).
pub fn app(_: &Theme) -> container::Style {
    filled(BG_APP, TEXT_PRIMARY)
}

/// Bufferline and other chrome (`bg.chrome`).
pub fn chrome(_: &Theme) -> container::Style {
    filled(BG_CHROME, TEXT_SECONDARY)
}

/// A banner row; the bottom border is drawn by [`banner_rule`].
pub fn banner(kind: BannerKind) -> impl Fn(&Theme) -> container::Style {
    move |_| {
        let c = kind.colors();
        filled(c.bg, c.text)
    }
}

/// The 1 px line under a banner.
pub fn banner_rule(kind: BannerKind) -> impl Fn(&Theme) -> container::Style {
    move |_| filled(kind.colors().border, kind.colors().border)
}

/// A command shown inside a banner, like the `sysctl` line.
pub fn banner_code(kind: BannerKind) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        border: border(kind.colors().border, SIZE_BORDER, RADIUS_CURSORLINE),
        ..filled(BG_CHROME, TEXT_STRONG)
    }
}

/// The status line (`bg.statusline`).
pub fn statusline(_: &Theme) -> container::Style {
    filled(BG_STATUSLINE, TEXT_SECONDARY)
}

/// A highlighted status line segment: the path, `line:column`.
pub fn statusline_segment(_: &Theme) -> container::Style {
    filled(BG_STATUSLINE_SEGMENT, TEXT_PRIMARY)
}

/// The mode badge: bold `text.on-accent` on the mode color.
pub fn mode_badge(mode: Mode) -> impl Fn(&Theme) -> container::Style {
    move |_| filled(mode_color(mode), TEXT_ON_ACCENT)
}

/// The command line row.
pub fn cmdline(_: &Theme) -> container::Style {
    filled(BG_APP, TEXT_MUTED)
}

/// A thin separator line (`border.default`).
pub fn rule(_: &Theme) -> container::Style {
    filled(BORDER_DEFAULT, BORDER_DEFAULT)
}

/// A separator between stacked command line messages (`bg.card.inline`).
pub fn rule_faint(_: &Theme) -> container::Style {
    filled(BG_CARD_INLINE, BG_CARD_INLINE)
}

/// A framed sample on a component sheet: `bg.app` with `border.default`.
pub fn frame(_: &Theme) -> container::Style {
    container::Style {
        border: border(BORDER_DEFAULT, SIZE_BORDER, RADIUS_POPUP),
        ..filled(BG_APP, TEXT_PRIMARY)
    }
}

/// Overlays: quick switcher, autocomplete, leader popup, dialogs.
pub fn overlay(radius: f32) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        border: border(BORDER_STRONG, SIZE_BORDER, radius),
        ..filled(BG_OVERLAY, TEXT_PRIMARY)
    }
}

/// A selected list item (`bg.selected`).
pub fn list_item(selected: bool) -> impl Fn(&Theme) -> container::Style {
    move |_| {
        let mut s = filled(
            if selected {
                BG_SELECTED
            } else {
                Color::TRANSPARENT
            },
            if selected { TEXT_STRONG } else { TEXT_PRIMARY },
        );
        s.border.radius = RADIUS_ITEM.into();
        s
    }
}

/// A filled bar: the 2 px `accent` line under the active buffer.
pub fn accent_bar(_: &Theme) -> container::Style {
    filled(ACCENT, TEXT_ON_ACCENT)
}

/// A command or path shown in a box (the missing vault path): `bg.chrome`
/// with `border.default`.
pub fn path_box(_: &Theme) -> container::Style {
    container::Style {
        border: border(BORDER_DEFAULT, SIZE_BORDER, RADIUS_CONTROL),
        ..filled(BG_CHROME, TEXT_STRONG)
    }
}

/// The side panel and explorer (`bg.panel`), set off by `border.default`.
pub fn side_panel(_: &Theme) -> container::Style {
    container::Style {
        border: border(BORDER_DEFAULT, SIZE_BORDER, 0.0),
        ..filled(BG_PANEL, TEXT_PRIMARY)
    }
}

// --- Buttons ------------------------------------------------------------------

fn button_style(bg: Option<Color>, text: Color, edge: Option<Color>) -> button::Style {
    button::Style {
        background: bg.map(Background::Color),
        text_color: text,
        border: border(
            edge.unwrap_or(Color::TRANSPARENT),
            if edge.is_some() { SIZE_BORDER } else { 0.0 },
            RADIUS_CONTROL,
        ),
        shadow: Shadow::default(),
        snap: true,
    }
}

/// Keyboard focus is a 2 px `accent` outline (0003 section 8).
fn focused(mut style: button::Style, focus: bool) -> button::Style {
    if focus {
        style.border.color = ACCENT;
        style.border.width = SIZE_FOCUS_OUTLINE;
    }
    style
}

/// The main button: `accent` with `text.on-accent`.
pub fn button_primary(focus: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, _| focused(button_style(Some(ACCENT), TEXT_ON_ACCENT, None), focus)
}

/// A secondary button: outlined with `border.control`.
pub fn button_secondary(focus: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, _| {
        focused(
            button_style(None, TEXT_PRIMARY, Some(BORDER_CONTROL)),
            focus,
        )
    }
}

/// A buffer in the bufferline: the active one on `tab.active.bg` in
/// `text.strong`, the others in `text.subtle` (0003 section 6).
pub fn buffer_tab(active: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, _| {
        let mut s = button_style(
            active.then_some(TAB_ACTIVE_BG),
            if active { TEXT_STRONG } else { TEXT_SUBTLE },
            None,
        );
        s.border.radius = iced::border::Radius::default();
        s
    }
}

/// A button in the status line (`␣e explorer`): `text.secondary`, no frame;
/// on `accent` with `text.on-accent` when its panel is open (mockup 02).
pub fn statusline_button(on: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, _| {
        let mut s = if on {
            button_style(Some(ACCENT), TEXT_ON_ACCENT, None)
        } else {
            button_style(None, TEXT_SECONDARY, None)
        };
        s.border.radius = iced::border::Radius::default();
        s
    }
}

/// A backlink card (0003 section 6): `bg.card` in the panel,
/// `bg.card.inline` under the note; keyboard focus is `bg.selected`.
pub fn backlink_card(
    inline: bool,
    focused: bool,
) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, _| {
        let bg = if focused {
            BG_SELECTED
        } else if inline {
            BG_CARD_INLINE
        } else {
            BG_CARD
        };
        let mut s = button_style(Some(bg), TEXT_PRIMARY, None);
        s.border.radius = RADIUS_CONTROL.into();
        s
    }
}

/// A row of a list that is a button (recent notes): like [`list_item`].
pub fn list_button(selected: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, _| {
        let mut s = button_style(
            selected.then_some(BG_SELECTED),
            if selected { TEXT_STRONG } else { TEXT_PRIMARY },
            None,
        );
        s.border.radius = RADIUS_ITEM.into();
        s
    }
}

/// The main action of a banner, rightmost: the banner's status color.
pub fn banner_action(kind: BannerKind) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, _| button_style(Some(kind.colors().action), TEXT_ON_ACCENT, None)
}

/// Other banner buttons: outlined with the banner's border color.
pub fn banner_button(kind: BannerKind) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, _| {
        let c = kind.colors();
        button_style(None, c.text, Some(c.border))
    }
}

/// Dims what is behind an overlay (`bg.scrim`).
pub fn scrim(_: &Theme) -> container::Style {
    filled(BG_SCRIM, TEXT_PRIMARY)
}

// --- Inputs -------------------------------------------------------------------

/// The quick switcher's input: no frame, `text.strong`.
pub fn switcher_input(_: &Theme, _: text_input::Status) -> text_input::Style {
    text_input::Style {
        background: Background::Color(Color::TRANSPARENT),
        border: Border::default(),
        icon: TEXT_MUTED,
        placeholder: TEXT_MUTED,
        value: TEXT_STRONG,
        selection: EDITOR_SELECTION,
    }
}

/// The `:` input on the command line: no frame, `text.primary`.
pub fn cmdline_input(_: &Theme, _: text_input::Status) -> text_input::Style {
    text_input::Style {
        background: Background::Color(Color::TRANSPARENT),
        border: Border::default(),
        icon: TEXT_MUTED,
        placeholder: TEXT_MUTED,
        value: TEXT_PRIMARY,
        selection: EDITOR_SELECTION,
    }
}

// --- Icons --------------------------------------------------------------------

/// Colors a line icon.
pub fn icon(color: Color) -> impl Fn(&Theme, svg::Status) -> svg::Style {
    move |_, _| svg::Style { color: Some(color) }
}
