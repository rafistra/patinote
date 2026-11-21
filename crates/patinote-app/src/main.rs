//! The `patinote-app` binary: the Iced desktop app.
//!
//! Until the app shell (step 2.2) it shows the component sheet of mockup 17.

use patinote_app::{fonts, sheet::Sheet, theme};

/// Set to `1` to print how many font faces Iced loaded and exit; used by the
/// test that checks system fonts are not scanned.
const FONT_REPORT: &str = "PATINOTE_FONT_REPORT";

fn main() -> iced::Result {
    if let Err(reason) = fonts::restart_without_system_fonts() {
        eprintln!("patinote-app: system fonts are scanned: {reason}");
    }
    if std::env::var_os(FONT_REPORT).is_some() {
        let mut system = iced::advanced::graphics::text::font_system()
            .write()
            .expect("the font system lock is never poisoned before the UI starts");
        for font in fonts::FONTS {
            system.load_font(std::borrow::Cow::Borrowed(font));
        }
        println!("{}", system.raw().db().len());
        return Ok(());
    }

    let mut app = iced::application(Sheet::new, |_: &mut Sheet, (): ()| {}, Sheet::view)
        .title("Patinote")
        .theme(|_: &Sheet| theme::theme())
        .default_font(fonts::DEFAULT)
        .window_size(theme::tokens::SIZE_WINDOW);
    for font in fonts::FONTS {
        app = app.font(font);
    }
    app.run()
}
