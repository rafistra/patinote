//! UI snapshots (`CLAUDE.md` section 6): renders screens headless, saves PNGs
//! to `target/ui-snapshots/` named after their mockup, and checks token
//! colors in the pixels.
//!
//! `iced_test::Simulator::snapshot` in Iced 0.14 always renders at scale 2.0
//! (docs/design/0002 section 5.2), so this harness does the same with public
//! APIs: build a `UserInterface`, draw it, and call `Headless::screenshot`
//! with the scale asked for. `ICED_TEST_BACKEND` picks the backend;
//! tiny-skia is the default.
//!
//! ```bash
//! ICED_TEST_BACKEND=tiny-skia cargo test -p patinote-app --test ui_snapshots
//! ```

use std::borrow::Cow;
use std::path::{Path, PathBuf};
use std::time::Instant;

use iced::advanced::graphics::text::font_system;
use iced::advanced::renderer::{Headless, Style};
use iced::{Color, Element, Event, Pixels, Size, mouse, window};
use iced_runtime::user_interface::{self, UserInterface};
use patinote_app::sheet::Sheet;
use patinote_app::theme::{self, tokens::*};

struct Harness {
    renderer: iced::Renderer,
    cache: Option<user_interface::Cache>,
}

struct Frame {
    rgba: Vec<u8>,
    size: Size<u32>,
}

impl Harness {
    fn new() -> Self {
        for font in patinote_app::fonts::FONTS {
            font_system()
                .write()
                .unwrap()
                .load_font(Cow::Borrowed(font));
        }
        let backend = std::env::var("ICED_TEST_BACKEND").ok();
        let renderer =
            iced_runtime::futures::futures::executor::block_on(<iced::Renderer as Headless>::new(
                patinote_app::fonts::DEFAULT,
                Pixels(TYPE_UI.size),
                // tiny-skia unless asked otherwise, so PNGs match across machines.
                Some(backend.as_deref().unwrap_or("tiny-skia")),
            ))
            .expect("a headless renderer (set ICED_TEST_BACKEND=tiny-skia)");
        Self {
            renderer,
            cache: None,
        }
    }

    fn render<'a>(&mut self, view: Element<'a, ()>, size: Size, scale: f32) -> Frame {
        let mut ui = UserInterface::build(
            view,
            size,
            self.cache.take().unwrap_or_default(),
            &mut self.renderer,
        );
        let mut messages = Vec::new();
        let _ = ui.update(
            &[Event::Window(
                window::Event::RedrawRequested(Instant::now()),
            )],
            mouse::Cursor::Unavailable,
            &mut self.renderer,
            &mut iced::advanced::clipboard::Null,
            &mut messages,
        );
        ui.draw(
            &mut self.renderer,
            &theme::theme(),
            &Style {
                text_color: TEXT_PRIMARY,
            },
            mouse::Cursor::Unavailable,
        );
        self.cache = Some(ui.into_cache());
        let physical = Size::new(
            (size.width * scale).round() as u32,
            (size.height * scale).round() as u32,
        );
        let rgba = self.renderer.screenshot(physical, scale, BG_APP);
        Frame {
            rgba,
            size: physical,
        }
    }
}

impl Frame {
    fn save(&self, path: &Path) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let file = std::io::BufWriter::new(std::fs::File::create(path).unwrap());
        let mut encoder = png::Encoder::new(file, self.size.width, self.size.height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(&self.rgba)
            .unwrap();
    }

    fn pixel(&self, x: u32, y: u32) -> [u8; 3] {
        let i = ((y * self.size.width + x) * 4) as usize;
        [self.rgba[i], self.rgba[i + 1], self.rgba[i + 2]]
    }

    /// How many pixels are within `tolerance` of this color in every channel
    /// (anti-aliased text rarely hits the exact value).
    fn count_near(&self, color: Color, tolerance: u8) -> usize {
        let [r, g, b, _] = color.into_rgba8();
        self.rgba
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|p| {
                p[0].abs_diff(r) <= tolerance
                    && p[1].abs_diff(g) <= tolerance
                    && p[2].abs_diff(b) <= tolerance
            })
            .count()
    }

    /// How many pixels have exactly this color.
    fn count(&self, color: Color) -> usize {
        let [r, g, b, _] = color.into_rgba8();
        self.rgba
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|p| p[0] == r && p[1] == g && p[2] == b)
            .count()
    }
}

fn out_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/ui-snapshots")
}

/// File name for a mockup at a scale: `17-banners-and-messages.png`, or
/// `17-banners-and-messages@1.25x.png`.
fn file_name(mockup: &str, scale: f32) -> String {
    if scale == 1.0 {
        format!("{mockup}.png")
    } else {
        format!("{mockup}@{scale}x.png")
    }
}

#[test]
fn mockup_17_component_sheet() {
    let mut h = Harness::new();
    let sheet = Sheet::new();
    let size = Size::new(1440.0, 860.0);
    for scale in [1.0, 1.25, 2.0] {
        let frame = h.render(sheet.view(), size, scale);
        frame.save(&out_dir().join(file_name("17-banners-and-messages", scale)));

        // Every surface of the sheet is drawn with its token, at every scale.
        let area = |px: f32| (px * scale * scale) as usize;
        for (name, color, min_px) in [
            ("bg.chrome (sheet)", BG_CHROME, 400_000.0),
            ("banner.warning.bg", BANNER_WARNING_BG, 20_000.0),
            ("banner.error.bg", BANNER_ERROR_BG, 20_000.0),
            ("banner.info.bg", BANNER_INFO_BG, 40_000.0),
            ("bg.statusline", BG_STATUSLINE, 10_000.0),
            ("bg.statusline.segment", BG_STATUSLINE_SEGMENT, 5_000.0),
            ("mode.normal badge", MODE_NORMAL, 1_000.0),
            ("mode.insert badge", MODE_INSERT, 1_000.0),
            ("mode.read badge", MODE_READ, 1_000.0),
        ] {
            let n = frame.count(color);
            assert!(n >= area(min_px), "{name} at scale {scale}: {n} pixels");
        }
        // Top-left corner is the sheet background.
        assert_eq!(frame.pixel(2, 2), {
            let [r, g, b, _] = BG_CHROME.into_rgba8();
            [r, g, b]
        });
    }
}

#[test]
fn bundled_fonts_render_without_system_fonts() {
    // The status line glyphs exist in JetBrains Mono, so they never fall
    // back to a system font (0003 section 4).
    let mut h = Harness::new();
    let line = patinote_app::widgets::statusline::StatusLine {
        mode: patinote_app::mode::Mode::Normal,
        path: "a.md".into(),
        modified: false,
        notes: vec![],
        indicator: Some(patinote_app::widgets::statusline::Indicator::NotSaved),
    };
    let frame = h.render(line.view(), Size::new(400.0, SIZE_STATUSLINE_H), 1.0);
    assert!(
        frame.count_near(STATUS_ERROR, 24) > 20,
        "the ✕ glyph and text are drawn in status.error"
    );
}
