//! Subset of the design tokens in docs/design/0003-ui-design-system.md that the
//! prototype needs. The real `theme::tokens` module comes in step 2.1.

use iced::Color;

const fn hex(rgb: u32) -> Color {
    Color::from_rgb8(
        (rgb >> 16) as u8,
        (rgb >> 8 & 0xFF) as u8,
        (rgb & 0xFF) as u8,
    )
}

pub const BG_APP: Color = hex(0x121614);
pub const BG_CURSORLINE: Color = hex(0x18201D);
pub const BG_STATUSLINE: Color = hex(0x161D1A);
pub const TEXT_PRIMARY: Color = hex(0xD6DEDA);
pub const TEXT_STRONG: Color = hex(0xEEF3F1);
pub const TEXT_MUTED: Color = hex(0x9AA4A0);
pub const TEXT_SECONDARY: Color = hex(0xB5BEBA);
pub const TEXT_GUTTER: Color = hex(0x7A8882);
pub const TEXT_ON_ACCENT: Color = hex(0x121614);
pub const ACCENT: Color = hex(0x5CC4A8);
pub const ACCENT_SOFT: Color = hex(0x8ADBC5);
pub const MODE_NORMAL: Color = hex(0x5CC4A8);
pub const MODE_INSERT: Color = hex(0xE0A46E);
pub const MODE_VISUAL: Color = hex(0xB39DDB);
pub const STATUS_WARNING: Color = hex(0xE0A46E);
pub const SYNTAX_LINK: Color = hex(0xD9A47A);
pub const SYNTAX_CODE: Color = hex(0xD4C48E);
pub const SYNTAX_QUOTE: Color = hex(0x9FB8D6);
pub const EDITOR_SELECTION: Color = hex(0x263F55);
pub const EDITOR_CURSOR_VISUAL: Color = hex(0xB39DDB);
pub const EDITOR_SEARCH_MATCH: Color = hex(0x433B1D);
pub const EDITOR_SEARCH_CURRENT: Color = hex(0xD4C48E);
pub const EDITOR_SEARCH_CURRENT_TEXT: Color = hex(0x121614);

/// `type.editor`: 15 px on a 26 px line.
pub const EDITOR_TEXT_SIZE: f32 = 15.0;
pub const EDITOR_LINE_HEIGHT: f32 = 26.0;
/// `type.mono.ui`: 13 px.
pub const MONO_UI_SIZE: f32 = 13.0;
/// `size.editor.maxw`.
pub const EDITOR_MAX_WIDTH: f32 = 920.0;
/// `size.gutter.w`.
pub const GUTTER_WIDTH: f32 = 44.0;
/// `size.statusline.h`.
pub const STATUSLINE_HEIGHT: f32 = 30.0;
/// Editor area padding: 28 px top, 32 px sides (0003, section 5).
pub const EDITOR_PAD_TOP: f32 = 28.0;
pub const EDITOR_PAD_SIDE: f32 = 32.0;
/// Cursor line radius (0003, section 5).
pub const RADIUS_CURSORLINE: f32 = 3.0;
/// Insert cursor width (0003, section 6).
pub const CURSOR_INSERT_WIDTH: f32 = 2.0;
