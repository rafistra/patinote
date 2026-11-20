//! Item 8: headless screenshots, and pixel checks for items 2–7.
//!
//! Run with `ICED_TEST_BACKEND=tiny-skia cargo test --test screenshot`.
//! PNGs go to `target/ui-snapshots/editor-proto/`.

use std::path::PathBuf;
use std::time::Instant;

use editor_proto::app::{App, Message, Options};
use editor_proto::headless::{Frame, Harness};
use editor_proto::tokens::*;
use editor_proto::vim::Key;
use iced::{Color, Size};

const SIZE: Size = Size::new(1440.0, 860.0);

fn note(rel: &str) -> String {
    format!(
        "{}/../../fixtures/vault-mockup/{rel}",
        env!("CARGO_MANIFEST_DIR")
    )
}

fn app(rel: &str, keys: &str) -> App {
    let mut app = App::new(Options {
        path: note(rel),
        builtin: false,
        bench: false,
        started: Instant::now(),
    });
    for c in keys.chars() {
        let key = match c {
            '⎋' => Key::Esc,
            '⏎' => Key::Enter,
            c => Key::Char(c),
        };
        let _ = app.update(Message::Key(key));
    }
    app
}

fn rgb(c: Color) -> [u8; 3] {
    let [r, g, b, _] = c.into_rgba8();
    [r, g, b]
}

fn count(frame: &Frame, color: Color) -> usize {
    let want = rgb(color);
    let editor_h =
        frame.size.height - (STATUSLINE_HEIGHT * frame.size.height as f32 / SIZE.height) as u32;
    (0..editor_h)
        .flat_map(|y| (0..frame.size.width).map(move |x| (x, y)))
        .filter(|&(x, y)| frame.pixel(x, y) == want)
        .count()
}

/// Longest vertical run of `color` in the editor area, in physical pixels.
fn tallest_run(frame: &Frame, color: Color) -> u32 {
    let want = rgb(color);
    let editor_h =
        frame.size.height - (STATUSLINE_HEIGHT * frame.size.height as f32 / SIZE.height) as u32;
    let mut best = 0;
    for x in 0..frame.size.width {
        let mut run = 0;
        for y in 0..editor_h {
            run = if frame.pixel(x, y) == want {
                run + 1
            } else {
                0
            };
            best = best.max(run);
        }
    }
    best
}

fn shoot(name: &str, app: &App, scale: f32) -> Frame {
    let mut h = Harness::new(None).expect("headless renderer");
    let frame = h.render(app, SIZE, scale);
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/ui-snapshots/editor-proto");
    frame
        .save_png(&dir.join(format!("{name}@{scale}x-{}.png", h.name())))
        .expect("write png");
    assert_eq!(
        frame.size,
        Size::new(
            (1440.0 * scale).round() as u32,
            (860.0 * scale).round() as u32
        )
    );
    frame
}

const SCALES: [f32; 3] = [1.0, 1.25, 2.0];

#[test]
fn item3_and_5_normal_mode_cursor_line_and_block_cursor() {
    // On the space after "Hari", so no glyph splits the cursor block.
    let app = app("Proyek/Desain indeks Patinote.md", "jjjjjjjllll");
    for scale in SCALES {
        let f = shoot("01-normal", &app, scale);
        // Cursor line: a wide band of bg.cursorline.
        assert!(
            count(&f, BG_CURSORLINE) as f32 > 600.0 * EDITOR_LINE_HEIGHT * 0.5 * scale * scale,
            "cursor line"
        );
        // Block cursor: a solid column of mode.normal as tall as a line.
        assert!(
            tallest_run(&f, MODE_NORMAL) as f32 >= (EDITOR_LINE_HEIGHT * scale).floor() - 2.0,
            "block cursor at {scale}"
        );
    }
}

#[test]
fn item5_insert_cursor_is_a_line() {
    let app = app("Proyek/Desain indeks Patinote.md", "jjjjjjjA");
    for scale in SCALES {
        let f = shoot("06-insert", &app, scale);
        assert!(
            tallest_run(&f, MODE_INSERT) as f32 >= (EDITOR_LINE_HEIGHT * scale).floor() - 2.0,
            "insert cursor at {scale}"
        );
        let area = count(&f, MODE_INSERT) as f32;
        assert!(
            area <= (CURSOR_INSERT_WIDTH + 1.0) * EDITOR_LINE_HEIGHT * scale * scale,
            "insert cursor is thin: {area}"
        );
    }
}

#[test]
fn item2_selection_and_search_backgrounds() {
    let app = app("Proyek/Desain indeks Patinote.md", "/indeks⏎vjj");
    for scale in SCALES {
        let f = shoot("13-visual-search", &app, scale);
        assert!(count(&f, EDITOR_SELECTION) > 1000, "selection background");
        assert!(
            count(&f, EDITOR_SEARCH_MATCH) > 100,
            "search match background"
        );
        assert!(
            tallest_run(&f, EDITOR_CURSOR_VISUAL) as f32
                >= (EDITOR_LINE_HEIGHT * scale).floor() - 2.0,
            "visual cursor"
        );
    }
    // The active match keeps its own background when not covered by the selection.
    let f = shoot(
        "13-search-current",
        &self::app("Proyek/Desain indeks Patinote.md", "/indeks⏎n"),
        1.0,
    );
    assert!(
        count(&f, EDITOR_SEARCH_CURRENT) > 100,
        "current match background"
    );
}

#[test]
fn item4_ambiguous_link_has_wavy_underline() {
    let app = app("Harian/2026-11-19.md", "");
    for scale in SCALES {
        let f = shoot("12-ambiguous", &app, scale);
        assert!(count(&f, STATUS_WARNING) > 20, "wavy underline at {scale}");
    }
}

#[test]
fn item7_column_is_at_most_920_px_and_centered() {
    let app = app("Proyek/Desain indeks Patinote.md", "jjjjjjj");
    let f = shoot("07-column", &app, 1.0);
    // The cursor line spans the column: find its horizontal extent.
    let want = rgb(BG_CURSORLINE);
    let xs: Vec<u32> = (0..f.size.width)
        .filter(|&x| (0..f.size.height).any(|y| f.pixel(x, y) == want))
        .collect();
    let (left, right) = (*xs.first().unwrap(), *xs.last().unwrap());
    let width = right - left + 1;
    assert!(
        (EDITOR_MAX_WIDTH - 2.0..=EDITOR_MAX_WIDTH).contains(&(width as f32)),
        "column width {width}"
    );
    let center = (left + right) as f32 / 2.0;
    assert!((center - 720.0).abs() <= 2.0, "column center {center}");
}

#[test]
fn item1_builtin_text_editor_colors_runs_with_bundled_font() {
    let app = App::new(Options {
        path: note("Proyek/Desain indeks Patinote.md"),
        builtin: true,
        bench: false,
        started: Instant::now(),
    });
    let f = shoot("01-builtin", &app, 2.0);
    // Glyph cores of links, code and headings carry their exact syntax colors.
    for (name, color) in [
        ("link", SYNTAX_LINK),
        ("code", SYNTAX_CODE),
        ("heading", ACCENT),
    ] {
        assert!(count(&f, color) > 50, "{name} color missing");
    }
}
