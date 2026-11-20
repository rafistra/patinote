//! Headless rendering at any scale factor.
//!
//! `iced_test::Simulator::snapshot` in Iced 0.14 always renders at scale 2.0,
//! but CLAUDE.md section 6 needs 1.0, 1.25 and 2.0. This harness does what the
//! simulator does with public APIs: build a `UserInterface`, draw it, and call
//! `Headless::screenshot` with the requested scale.

use std::borrow::Cow;
use std::path::Path;
use std::time::{Duration, Instant};

use iced::advanced::graphics::text::font_system;
use iced::advanced::renderer::{Headless, Style};
use iced::{Event, Pixels, Size, Theme, mouse, window};
use iced_runtime::user_interface::{self, UserInterface};

use crate::app::App;
use crate::tokens::{BG_APP, TEXT_PRIMARY};

/// A headless renderer plus the UI cache kept between frames, like a window.
pub struct Harness {
    renderer: iced::Renderer,
    cache: Option<user_interface::Cache>,
}

/// One rendered frame.
pub struct Frame {
    pub rgba: Vec<u8>,
    pub size: Size<u32>,
    /// `view`, layout and `draw`.
    pub ui: Duration,
    /// Rasterizing (tiny-skia) or rendering and reading back (wgpu).
    pub raster: Duration,
}

impl Harness {
    /// Creates a renderer. `backend` is `tiny-skia` or `wgpu`; `None` reads
    /// `ICED_TEST_BACKEND`, then lets Iced choose.
    pub fn new(backend: Option<&str>) -> Option<Self> {
        for font in crate::FONTS {
            font_system().write().ok()?.load_font(Cow::Borrowed(font));
        }
        let env = std::env::var("ICED_TEST_BACKEND").ok();
        let backend = backend.or(env.as_deref());
        let renderer = iced_runtime::futures::futures::executor::block_on(
            <iced::Renderer as Headless>::new(crate::INTER, Pixels(14.0), backend),
        )?;
        Some(Self {
            renderer,
            cache: None,
        })
    }

    /// The renderer's name, for example `tiny-skia` or `wgpu`.
    pub fn name(&self) -> String {
        self.renderer.name()
    }

    /// Renders `app` at a logical `size` and `scale`.
    pub fn render(&mut self, app: &App, size: Size, scale: f32) -> Frame {
        let start = Instant::now();
        let mut ui = UserInterface::build(
            app.view(),
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
            &Theme::Dark,
            &Style {
                text_color: TEXT_PRIMARY,
            },
            mouse::Cursor::Unavailable,
        );
        self.cache = Some(ui.into_cache());
        let ui_time = start.elapsed();

        let physical = Size::new(
            (size.width * scale).round() as u32,
            (size.height * scale).round() as u32,
        );
        let start = Instant::now();
        let rgba = self.renderer.screenshot(physical, scale, BG_APP);
        Frame {
            rgba,
            size: physical,
            ui: ui_time,
            raster: start.elapsed(),
        }
    }
}

impl Frame {
    /// Writes the frame as an RGBA PNG.
    pub fn save_png(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let file = std::io::BufWriter::new(std::fs::File::create(path)?);
        let mut encoder = png::Encoder::new(file, self.size.width, self.size.height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().map_err(std::io::Error::other)?;
        writer
            .write_image_data(&self.rgba)
            .map_err(std::io::Error::other)?;
        Ok(())
    }

    /// The RGB value of pixel (x, y).
    pub fn pixel(&self, x: u32, y: u32) -> [u8; 3] {
        let i = ((y * self.size.width + x) * 4) as usize;
        [self.rgba[i], self.rgba[i + 1], self.rgba[i + 2]]
    }
}
