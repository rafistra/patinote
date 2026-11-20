//! Design tokens from `docs/design/0003-ui-design-system.md`.
//!
//! This is the only file with color values, font sizes, spacing and radii.
//! Names follow the token names: `bg.app` is `BG_APP`, `text.muted` is
//! `TEXT_MUTED`. Values without a token name in 0003 (syntax colors, mode
//! baca links) are named after the element they color.

use iced::font::{Family, Style, Weight};
use iced::{Color, Font};

const fn hex(rgb: u32) -> Color {
    Color::from_rgb8((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
}

// --- 3.1 Backgrounds and borders --------------------------------------------

pub const BG_APP: Color = hex(0x121614);
pub const BG_CHROME: Color = hex(0x0C0F0E);
pub const BG_PANEL: Color = hex(0x171C1A);
pub const BG_STATUSLINE: Color = hex(0x161D1A);
pub const BG_STATUSLINE_SEGMENT: Color = hex(0x1F2925);
pub const BG_CARD: Color = hex(0x1F2624);
pub const BG_CARD_INLINE: Color = hex(0x1A201E);
pub const BG_OVERLAY: Color = hex(0x172019);
pub const BG_CURSORLINE: Color = hex(0x18201D);
pub const BG_SELECTED: Color = hex(0x22403A);
pub const BG_CODE_INLINE: Color = hex(0x1E2522);
pub const BORDER_DEFAULT: Color = hex(0x242C29);
pub const BORDER_STRONG: Color = hex(0x2E3A35);
pub const BORDER_CONTROL: Color = hex(0x34403C);

// --- 3.2 Text ----------------------------------------------------------------

pub const TEXT_STRONG: Color = hex(0xEEF3F1);
pub const TEXT_PRIMARY: Color = hex(0xD6DEDA);
pub const TEXT_SECONDARY: Color = hex(0xB5BEBA);
pub const TEXT_MUTED: Color = hex(0x9AA4A0);
pub const TEXT_SUBTLE: Color = hex(0x8D9A94);
pub const TEXT_GUTTER: Color = hex(0x7A8882);
pub const TEXT_ON_ACCENT: Color = hex(0x121614);

// --- 3.3 Accent and status ---------------------------------------------------

pub const ACCENT: Color = hex(0x5CC4A8);
pub const ACCENT_SOFT: Color = hex(0x8ADBC5);
pub const MODE_NORMAL: Color = hex(0x5CC4A8);
pub const MODE_INSERT: Color = hex(0xE0A46E);
pub const MODE_VISUAL: Color = hex(0xB39DDB);
pub const MODE_READ: Color = hex(0x7FA6D9);
pub const MODE_CONFLICT: Color = hex(0xE0A46E);
pub const STATUS_OK: Color = hex(0x5CC4A8);
pub const STATUS_WARNING: Color = hex(0xE0A46E);
pub const STATUS_ERROR: Color = hex(0xEF8A80);
pub const STATUS_INFO: Color = hex(0x7FA6D9);

// --- 3.4 Markdown syntax in the editor ---------------------------------------

pub const SYNTAX_HEADING: Color = ACCENT;
pub const SYNTAX_LINK: Color = hex(0xD9A47A);
pub const SYNTAX_CODE: Color = hex(0xD4C48E);
pub const SYNTAX_QUOTE: Color = hex(0x9FB8D6);
pub const SYNTAX_LIST: Color = ACCENT;
pub const SYNTAX_TASK_DONE: Color = TEXT_MUTED;
pub const SYNTAX_FRONTMATTER: Color = TEXT_GUTTER;
pub const SYNTAX_TAG: Color = ACCENT_SOFT;

// --- 3.5 Selection, search and ambiguous links -------------------------------

pub const EDITOR_SELECTION: Color = hex(0x263F55);
pub const EDITOR_CURSOR_VISUAL: Color = hex(0xB39DDB);
pub const EDITOR_SEARCH_MATCH: Color = hex(0x433B1D);
pub const EDITOR_SEARCH_CURRENT: Color = hex(0xD4C48E);
pub const EDITOR_SEARCH_CURRENT_TEXT: Color = hex(0x121614);
/// Wavy underline under an ambiguous link; the link keeps `SYNTAX_LINK`.
pub const EDITOR_LINK_AMBIGUOUS: Color = STATUS_WARNING;
/// Search position in the status line (`/indeks [2/3]`).
pub const STATUSLINE_SEARCH: Color = hex(0xD4C48E);

// --- 3.6 Reading mode --------------------------------------------------------

pub const READ_LINK: Color = hex(0xD9A47A);
pub const READ_LINK_MISSING: Color = hex(0x9A8878);
pub const READ_LINK_FOCUS: Color = ACCENT;
pub const READ_TAG_TEXT: Color = ACCENT_SOFT;
pub const READ_TAG_BG: Color = BG_SELECTED;

// --- 3.7 Banners and diff ----------------------------------------------------

pub const BANNER_WARNING_BG: Color = hex(0x2A2219);
pub const BANNER_WARNING_BORDER: Color = hex(0x4A3A28);
pub const BANNER_WARNING_TEXT: Color = hex(0xF3E3D2);
pub const BANNER_WARNING_MUTED: Color = hex(0xCBB9A6);
pub const BANNER_ERROR_BG: Color = hex(0x2E1A1A);
pub const BANNER_ERROR_BORDER: Color = hex(0x5A2E2E);
pub const BANNER_ERROR_TEXT: Color = hex(0xF7DEDB);
pub const BANNER_ERROR_MUTED: Color = hex(0xD9B8B4);
pub const BANNER_INFO_BG: Color = hex(0x17222C);
pub const BANNER_INFO_BORDER: Color = hex(0x2C4258);
pub const BANNER_INFO_TEXT: Color = hex(0xDCE8F5);
pub const BANNER_INFO_MUTED: Color = hex(0xB3C3D3);
pub const DIFF_DISK_BG: Color = hex(0x3A2A1A);
pub const DIFF_DISK_MARKER: Color = hex(0xE0A46E);
pub const DIFF_MINE_BG: Color = hex(0x1C3340);
pub const DIFF_MINE_MARKER: Color = hex(0x7FA6D9);

// --- 6 Components ------------------------------------------------------------

/// Background of the active buffer tab.
pub const TAB_ACTIVE_BG: Color = hex(0x1A211E);

// --- 4 Typography ------------------------------------------------------------

/// Family names of the bundled fonts (`assets/fonts/`).
pub const FAMILY_UI: &str = "Inter";
pub const FAMILY_MONO: &str = "JetBrains Mono";

const fn font(family: &'static str, weight: Weight) -> Font {
    Font {
        family: Family::Name(family),
        weight,
        stretch: iced::font::Stretch::Normal,
        style: Style::Normal,
    }
}

pub const FONT_UI: Font = font(FAMILY_UI, Weight::Normal);
pub const FONT_UI_MEDIUM: Font = font(FAMILY_UI, Weight::Medium);
pub const FONT_UI_SEMIBOLD: Font = font(FAMILY_UI, Weight::Semibold);
pub const FONT_UI_BOLD: Font = font(FAMILY_UI, Weight::Bold);
pub const FONT_UI_ITALIC: Font = Font {
    style: Style::Italic,
    ..FONT_UI
};
pub const FONT_MONO: Font = font(FAMILY_MONO, Weight::Normal);
pub const FONT_MONO_SEMIBOLD: Font = font(FAMILY_MONO, Weight::Semibold);
pub const FONT_MONO_BOLD: Font = font(FAMILY_MONO, Weight::Bold);

/// A typography token: font, size and line height in logical pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Type {
    pub font: Font,
    pub size: f32,
    pub line_height: f32,
}

const fn ty(font: Font, size: f32, line_height: f32) -> Type {
    Type {
        font,
        size,
        line_height,
    }
}

/// `type.editor`: 15 px / 26 px; headings use `FONT_MONO_BOLD`.
pub const TYPE_EDITOR: Type = ty(FONT_MONO, 15.0, 26.0);
/// `type.focus`: 16 px / 30 px.
pub const TYPE_FOCUS: Type = ty(FONT_MONO, 16.0, 30.0);
/// `type.mono.ui`: 13 px; badges use `FONT_MONO_BOLD`.
pub const TYPE_MONO_UI: Type = ty(FONT_MONO, 13.0, 13.0 * 1.4);
/// `type.ui`: 14 px / 1.4.
pub const TYPE_UI: Type = ty(FONT_UI, 14.0, 14.0 * 1.4);
/// `type.ui.small`: 13 px / 1.5.
pub const TYPE_UI_SMALL: Type = ty(FONT_UI, 13.0, 13.0 * 1.5);
/// `type.label`: 11 px, uppercase, letter spacing 0.8 px, weight 600.
/// The line height is the mockups' `normal` for Inter (about 1.2).
pub const TYPE_LABEL: Type = ty(FONT_UI_SEMIBOLD, 11.0, 11.0 * 1.2);
/// Letter spacing of `type.label`.
pub const TYPE_LABEL_TRACKING: f32 = 0.8;
/// `type.read.body`: 16 px / 1.7.
pub const TYPE_READ_BODY: Type = ty(FONT_UI, 16.0, 16.0 * 1.7);
/// `type.read.h1`: 32 px / 1.25, weight 700.
pub const TYPE_READ_H1: Type = ty(FONT_UI_BOLD, 32.0, 32.0 * 1.25);
/// `type.read.h2`: 24 px / 1.3, weight 700.
pub const TYPE_READ_H2: Type = ty(FONT_UI_BOLD, 24.0, 24.0 * 1.3);
/// `type.dialog.title`: 17 px, weight 600.
pub const TYPE_DIALOG_TITLE: Type = ty(FONT_UI_SEMIBOLD, 17.0, 17.0 * 1.4);

/// `Ctrl+=` / `Ctrl+-` change all type sizes in 10 % steps, within 70–200 %.
pub const ZOOM_STEP: f32 = 0.1;
pub const ZOOM_MIN: f32 = 0.7;
pub const ZOOM_MAX: f32 = 2.0;

// --- 5 Sizes and spacing -----------------------------------------------------

/// The spacing scale; use only these values.
pub const SPACE_2: f32 = 2.0;
pub const SPACE_4: f32 = 4.0;
pub const SPACE_6: f32 = 6.0;
pub const SPACE_8: f32 = 8.0;
pub const SPACE_10: f32 = 10.0;
pub const SPACE_12: f32 = 12.0;
pub const SPACE_14: f32 = 14.0;
pub const SPACE_16: f32 = 16.0;
pub const SPACE_20: f32 = 20.0;
pub const SPACE_24: f32 = 24.0;
pub const SPACE_32: f32 = 32.0;
pub const SPACE_48: f32 = 48.0;
/// Every value of the spacing scale, for tests.
pub const SPACING_SCALE: [f32; 12] = [
    SPACE_2, SPACE_4, SPACE_6, SPACE_8, SPACE_10, SPACE_12, SPACE_14, SPACE_16, SPACE_20, SPACE_24,
    SPACE_32, SPACE_48,
];

/// Corner radii.
pub const RADIUS_CURSORLINE: f32 = 3.0;
pub const RADIUS_ITEM: f32 = 5.0;
pub const RADIUS_CONTROL: f32 = 6.0;
pub const RADIUS_POPUP: f32 = 8.0;
pub const RADIUS_DIALOG: f32 = 10.0;
/// Fully rounded (tag labels).
pub const RADIUS_FULL: f32 = 999.0;

/// The window size every mockup is drawn for (0003 section 2), used as the
/// first window size.
pub const SIZE_WINDOW: (f32, f32) = (1440.0, 860.0);

pub const SIZE_BUFFERLINE_H: f32 = 32.0;
pub const SIZE_TABBAR_H: f32 = 40.0;
pub const SIZE_STATUSLINE_H: f32 = 30.0;
pub const SIZE_CMDLINE_H: f32 = 26.0;
pub const SIZE_EXPLORER_W: f32 = 240.0;
pub const SIZE_EXPLORER_W_MIN: f32 = 180.0;
pub const SIZE_EXPLORER_W_MAX: f32 = 480.0;
pub const SIZE_SIDEPANEL_W: f32 = 300.0;
pub const SIZE_SIDEPANEL_W_MIN: f32 = 240.0;
pub const SIZE_SIDEPANEL_W_MAX: f32 = 480.0;
pub const SIZE_EDITOR_MAXW: f32 = 920.0;
pub const SIZE_EDITOR_MAXW_CLASSIC: f32 = 820.0;
pub const SIZE_EDITOR_MAXW_FOCUS: f32 = 760.0;
pub const SIZE_READ_MAXW: f32 = 720.0;
pub const SIZE_GUTTER_W: f32 = 44.0;
pub const SIZE_SWITCHER_W: f32 = 640.0;
pub const SIZE_SWITCHER_TOP: f32 = 72.0;
pub const SIZE_AUTOCOMPLETE_W: f32 = 460.0;
pub const SIZE_LEADER_W: f32 = 600.0;
pub const SIZE_LEADER_BOTTOM: f32 = 64.0;
pub const SIZE_DIALOG_W_MIN: f32 = 460.0;
pub const SIZE_DIALOG_W_MAX: f32 = 520.0;
pub const SIZE_PICKER_W: f32 = 460.0;
pub const SIZE_START_MAXW: f32 = 560.0;
pub const SIZE_WELCOME_W: f32 = 480.0;
pub const SIZE_CONTROL_H: f32 = 36.0;
pub const SIZE_CONTROL_H_LARGE: f32 = 40.0;
pub const SIZE_LIST_ROW: f32 = 34.0;
pub const SIZE_LIST_ROW_LARGE: f32 = 40.0;
/// Banner and status line icons.
pub const SIZE_ICON: f32 = 18.0;
/// Borders, and the active tab and focus outlines.
pub const SIZE_BORDER: f32 = 1.0;
pub const SIZE_FOCUS_OUTLINE: f32 = 2.0;

/// Common padding: editor area 28 px top and 32 px sides; panels 16 px;
/// overlays 14–16 px; dialogs 22–24 px.
pub const PAD_EDITOR_TOP: f32 = 28.0;
pub const PAD_EDITOR_SIDE: f32 = SPACE_32;
pub const PAD_PANEL: f32 = SPACE_16;
pub const PAD_OVERLAY: f32 = SPACE_16;
pub const PAD_DIALOG: f32 = SPACE_24;
