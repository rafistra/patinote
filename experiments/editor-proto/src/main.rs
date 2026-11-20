//! `editor-proto [--builtin] [--bench] [FILE]`
//!
//! - default: the custom editor widget on FILE (default: the mockup note).
//! - `--builtin`: the stock `text_editor` with the same highlighting (item 1).
//! - `--bench`: replays scripted keys, prints a JSON line with latency and
//!   memory, then exits. Choose the renderer with `ICED_BACKEND=tiny-skia|wgpu`.

use std::time::Instant;

use editor_proto::app::{App, Options};

fn main() -> iced::Result {
    let started = Instant::now();
    let mut opts = Options {
        path: editor_proto::default_note(),
        builtin: false,
        bench: false,
        started,
    };
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "--builtin" => opts.builtin = true,
            "--bench" => opts.bench = true,
            _ => opts.path = arg,
        }
    }
    let mut application = iced::application(
        move || {
            App::new(Options {
                path: opts.path.clone(),
                ..opts
            })
        },
        App::update,
        App::view,
    )
    .title(App::title)
    .subscription(App::subscription)
    .theme(App::theme)
    .default_font(editor_proto::INTER)
    .window_size((1440.0, 860.0));
    for font in editor_proto::FONTS {
        application = application.font(*font);
    }
    application.run()
}
