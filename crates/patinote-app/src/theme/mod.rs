//! The design system: tokens from `docs/design/0003-ui-design-system.md` and
//! the Iced styles built from them. Phase 2 has only the dark theme; a light
//! theme (Phase 8) adds a second set of tokens.

pub mod styles;
pub mod tokens;

/// The Iced theme. Widgets take their colors from [`styles`], so this only
/// sets the colors of widgets drawn without a style.
pub fn theme() -> iced::Theme {
    use tokens::*;
    iced::Theme::custom(
        "Patinote Dark",
        iced::theme::Palette {
            background: BG_APP,
            text: TEXT_PRIMARY,
            primary: ACCENT,
            success: STATUS_OK,
            warning: STATUS_WARNING,
            danger: STATUS_ERROR,
        },
    )
}
