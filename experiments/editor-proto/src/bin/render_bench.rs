//! `render_bench BACKEND FILE`: headless frame cost per key press.
//!
//! For each scale (1.0 and 2.0) it replays 150 `j` presses and 150 typed
//! characters, rendering a full frame after every key, and prints one JSON line
//! with p50/p95/max of `ui` (view, layout, draw) and `raster` (tiny-skia
//! rasterization, or wgpu rendering plus pixel readback).

use std::time::{Duration, Instant};

use editor_proto::app::{App, Message, Options};
use editor_proto::headless::Harness;
use editor_proto::vim::Key;
use iced::Size;

fn stats(v: &mut [Duration]) -> String {
    v.sort();
    let ms = |d: Duration| d.as_secs_f64() * 1000.0;
    let q = |p: f64| ms(v[((v.len() - 1) as f64 * p).round() as usize]);
    format!(
        "{{\"p50\":{:.2},\"p95\":{:.2},\"max\":{:.2}}}",
        q(0.5),
        q(0.95),
        q(1.0)
    )
}

fn main() {
    let mut args = std::env::args().skip(1);
    let backend = args.next().unwrap_or_else(|| "tiny-skia".to_owned());
    let path = args.next().unwrap_or_else(editor_proto::default_note);
    let mut harness = Harness::new(Some(&backend)).expect("renderer");
    let mut out = Vec::new();
    let scales: Vec<f32> = std::env::var("SCALES").ok().map_or(vec![1.0, 2.0], |s| {
        s.split(',').filter_map(|x| x.parse().ok()).collect()
    });
    for scale in scales {
        let mut app = App::new(Options {
            path: path.clone(),
            builtin: false,
            bench: false,
            started: Instant::now(),
        });
        let size = Size::new(1440.0, 860.0);
        for _ in 0..5 {
            harness.render(&app, size, scale);
        }
        let mut keys: Vec<Key> = std::iter::repeat_n(Key::Char('j'), 150).collect();
        keys.push(Key::Char('A'));
        keys.extend(
            "the quick brown fox jumps over the lazy dog "
                .repeat(4)
                .chars()
                .take(150)
                .map(Key::Char),
        );
        let (mut ui, mut raster) = (Vec::new(), Vec::new());
        for key in keys {
            let _ = app.update(Message::Key(key));
            let f = harness.render(&app, size, scale);
            ui.push(f.ui);
            raster.push(f.raster);
        }
        out.push(format!(
            "\"{scale}\":{{\"ui_ms\":{},\"raster_ms\":{}}}",
            stats(&mut ui),
            stats(&mut raster)
        ));
    }
    println!(
        "{{\"renderer\":\"{}\",\"file\":\"{}\",\"scales\":{{{}}}}}",
        harness.name(),
        path.rsplit('/').next().unwrap_or(""),
        out.join(",")
    );
}
