//! Editor prototype for step 0.6. See docs/design/0002-editor-dan-renderer.md.

pub mod app;
pub mod editor;
pub mod headless;
pub mod highlight;
pub mod tokens;
pub mod vim;

/// Fonts bundled with Patinote, loaded from bytes (no system font lookup for
/// the UI itself).
pub const FONTS: &[&[u8]] = &[
    include_bytes!("../../../crates/patinote-app/assets/fonts/Inter-Regular.ttf"),
    include_bytes!("../../../crates/patinote-app/assets/fonts/Inter-Bold.ttf"),
    include_bytes!("../../../crates/patinote-app/assets/fonts/JetBrainsMono-Regular.ttf"),
    include_bytes!("../../../crates/patinote-app/assets/fonts/JetBrainsMono-Bold.ttf"),
];

/// The UI font.
pub const INTER: iced::Font = iced::Font::with_name("Inter");

/// The default note: the main note of the mockups.
pub fn default_note() -> String {
    format!(
        "{}/../../fixtures/vault-mockup/Proyek/Desain indeks Patinote.md",
        env!("CARGO_MANIFEST_DIR")
    )
}
