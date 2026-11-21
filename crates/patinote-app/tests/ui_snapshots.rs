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

mod common;

use std::borrow::Cow;
use std::path::{Path, PathBuf};
use std::time::Instant;

use common::{Env, command};
use iced::Length;
use iced::advanced::graphics::text::font_system;
use iced::advanced::renderer::{Headless, Style};
use iced::widget::{column, container};
use iced::{Color, Element, Event, Pixels, Size, mouse, window};
use iced_runtime::user_interface::{self, UserInterface};
use patinote_app::app::App;
use patinote_app::screens::welcome;
use patinote_app::sheet::Sheet;
use patinote_app::state::{self, Recent, VaultState};
use patinote_app::theme::{self, tokens::*};
use patinote_app::widgets::bufferline;
use patinote_core::config::Config;

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
        ..Default::default()
    };
    let frame = h.render(line.view(), Size::new(400.0, SIZE_STATUSLINE_H), 1.0);
    assert!(
        frame.count_near(STATUS_ERROR, 24) > 20,
        "the ✕ glyph and text are drawn in status.error"
    );
}

const SCALES: [f32; 3] = [1.0, 1.25, 2.0];

fn size() -> Size {
    Size::new(SIZE_WINDOW.0, SIZE_WINDOW.1)
}

fn rgb(c: Color) -> [u8; 3] {
    let [r, g, b, _] = c.into_rgba8();
    [r, g, b]
}

/// Mockup 10: both cards side by side in the app's frame (bufferline with
/// the name only, empty status line and command line).
#[test]
fn mockup_10_welcome_and_missing_vault() {
    let mut h = Harness::new();
    let actions = welcome::Actions {
        choose: (),
        retry: (),
    };
    let welcome_card = welcome::Card::Welcome;
    let missing = welcome::Card::Missing {
        path: "~/Vault".into(),
    };
    for scale in SCALES {
        let view: Element<'_, ()> = column![
            bufferline::view(&[], None, |_| ()),
            container(welcome::sheet(&welcome_card, &missing, &actions))
                .width(Length::Fill)
                .height(Length::Fill)
                .style(theme::styles::app),
            container("")
                .width(Length::Fill)
                .height(SIZE_STATUSLINE_H)
                .style(theme::styles::statusline),
            container("")
                .width(Length::Fill)
                .height(SIZE_CMDLINE_H)
                .style(theme::styles::cmdline),
        ]
        .into();
        let frame = h.render(view, size(), scale);
        frame.save(&out_dir().join(file_name("10-welcome-and-missing-vault", scale)));
        let area = |px: f32| (px * scale * scale) as usize;
        assert!(
            frame.count(BG_OVERLAY) >= area(150_000.0),
            "two cards on bg.overlay"
        );
        assert!(
            frame.count(ACCENT) >= area(3_000.0),
            "primary buttons in accent"
        );
        assert_eq!(frame.pixel(2, 2), rgb(BG_CHROME), "bufferline");
    }
}

/// The real first-run screen: one card, centered.
#[test]
fn welcome_screen_shows_one_centered_card() {
    let env = Env::new();
    let (app, _) = App::new(env.launch(None, Config::default()));
    let mut h = Harness::new();
    let frame = h.render(app.view().map(|_| ()), size(), 1.0);
    frame.save(&out_dir().join("10-welcome.png"));
    // The card's middle sits at the window's middle.
    let y = 200;
    let row: Vec<u32> = (0..frame.size.width)
        .filter(|x| frame.pixel(*x, y) == rgb(BG_OVERLAY))
        .collect();
    let (first, last) = (row[0], row[row.len() - 1]);
    assert!(
        ((first + last) as i64 / 2 - 720).abs() <= 2,
        "{first}..{last}"
    );
    // The border is drawn in `border.strong`, so the fill is 2 px narrower.
    assert!(((last - first) as f32 - SIZE_WELCOME_W).abs() <= 4.0);
}

/// Seconds before now for each recent note of mockup 11.
const RECENT: [(&str, u64); 5] = [
    ("Proyek/Desain indeks Patinote.md", 120),
    ("Harian/2026-11-19.md", 30 * 3600),
    ("Proyek/Patinote.md", 31 * 3600),
    ("Rapat/Rapat 19 Nov.md", 2 * 86_400 + 60),
    ("Referensi/Daftar bacaan.md", 8 * 86_400),
];

/// A started app on the mockup vault, with mockup 11's recent notes.
fn app_with_recent(env: &Env) -> App {
    let vault = env.vault();
    let id = patinote_core::location::vault_id(&vault).unwrap();
    let now = state::now();
    let ui = VaultState {
        recent: RECENT
            .iter()
            .map(|(path, ago)| Recent {
                path: (*path).into(),
                opened: now - ago,
            })
            .collect(),
        ..VaultState::default()
    };
    state::save(&state::vault_state_path(&env.path("state"), &id), &ui).unwrap();
    env.start(env.launch(Some(vault), Config::default()))
}

/// Mockup 11: no note open.
#[test]
fn mockup_11_no_note_open() {
    let env = Env::new();
    let app = app_with_recent(&env);
    let mut h = Harness::new();
    for scale in SCALES {
        let frame = h.render(app.view().map(|_| ()), size(), scale);
        frame.save(&out_dir().join(file_name("11-no-note-open", scale)));
        let area = |px: f32| (px * scale * scale) as usize;
        assert!(
            frame.count(BG_SELECTED) >= area(15_000.0),
            "selected recent note"
        );
        assert!(frame.count(MODE_NORMAL) >= area(1_000.0), "NORMAL badge");
        assert_eq!(frame.pixel(2, 2), rgb(BG_CHROME), "bufferline");
    }
}

/// Mockup 01: three notes open, the first active. The editor of step 2.3
/// is not here yet; the bufferline and status line are compared.
#[test]
fn mockup_01_minimal_chrome() {
    let env = Env::new();
    let mut app = app_with_recent(&env);
    for name in ["Desain indeks Patinote", "Patinote", "2026-11-19"] {
        command(&mut app, &format!("e {name}"));
    }
    command(&mut app, "b 1");
    // The cursor on the "Tabel inti" heading, as in the mockup.
    common::keys(&mut app, "12G");
    let mut h = Harness::new();
    for scale in SCALES {
        let frame = h.render(app.view().map(|_| ()), size(), scale);
        frame.save(&out_dir().join(file_name("01-minimal", scale)));
        let area = |px: f32| (px * scale * scale) as usize;
        assert!(frame.count(TAB_ACTIVE_BG) >= area(3_000.0), "active buffer");
        assert!(
            frame.count(ACCENT) >= area(200.0),
            "accent underline and mark"
        );
        assert!(
            frame.count(BG_STATUSLINE_SEGMENT) >= area(5_000.0),
            "path and position segments"
        );
        assert!(frame.count(BG_CURSORLINE) >= area(20_000.0), "cursor line");
        assert!(
            frame.count(MODE_NORMAL) >= area(1_100.0),
            "badge and block cursor"
        );
    }
    assert_eq!(app.title(), "Desain indeks Patinote — Patinote");
}

/// Mockup 05: the quick switcher over a note, with "dsn ind" typed.
#[test]
fn mockup_05_quick_switcher() {
    let env = Env::new();
    let mut app = app_with_recent(&env);
    command(&mut app, "e 2026-11-19");
    command(&mut app, "e Patinote");
    command(&mut app, "b 1");
    common::leader(&mut app, "f");
    common::send(
        &mut app,
        patinote_app::app::Message::SwitcherInput("dsn ind".into()),
    );
    let mut h = Harness::new();
    for scale in SCALES {
        let frame = h.render(app.view().map(|_| ()), size(), scale);
        frame.save(&out_dir().join(file_name("05-quick-switcher", scale)));
        let area = |px: f32| (px * scale * scale) as usize;
        assert!(frame.count(BG_OVERLAY) >= area(60_000.0), "switcher box");
        assert!(
            frame.count(BG_SELECTED) >= area(15_000.0),
            "selected result"
        );
        assert!(
            frame.count_near(ACCENT, 24) >= area(100.0),
            "matched letters"
        );
    }
}

/// Mockup 06: `[[desa` typed in insert mode, suggestions under the cursor.
#[test]
fn mockup_06_link_autocomplete() {
    let env = Env::new();
    let mut app = app_with_recent(&env);
    command(&mut app, "e 2026-11-19");
    command(&mut app, "e Patinote");
    command(&mut app, "b 1");
    common::keys(&mut app, "6GS- Lanjut bahas di [[desa");
    let mut h = Harness::new();
    for scale in SCALES {
        let frame = h.render(app.view().map(|_| ()), size(), scale);
        frame.save(&out_dir().join(file_name("06-link-autocomplete", scale)));
        let area = |px: f32| (px * scale * scale) as usize;
        assert!(frame.count(BG_OVERLAY) >= area(20_000.0), "suggestions box");
        assert!(
            frame.count(MODE_INSERT) >= area(1_000.0),
            "INSERT badge and line cursor"
        );
    }
}

/// Mockup 12: `gd` on `[[Rapat]]`, which two notes are named.
#[test]
fn mockup_12_ambiguous_link() {
    let env = Env::new();
    let mut app = app_with_recent(&env);
    command(&mut app, "e 2026-11-19");
    command(&mut app, "e Patinote");
    command(&mut app, "b 1");
    common::keys(&mut app, "7G0f[gd");
    let mut h = Harness::new();
    for scale in SCALES {
        let frame = h.render(app.view().map(|_| ()), size(), scale);
        frame.save(&out_dir().join(file_name("12-ambiguous-link", scale)));
        let area = |px: f32| (px * scale * scale) as usize;
        assert!(frame.count(BG_OVERLAY) >= area(20_000.0), "picker box");
        assert!(
            frame.count_near(STATUS_WARNING, 24) >= area(60.0),
            "wavy underline and W: message"
        );
    }
}

/// Mockup 02, part of it: the side panel (Outline and Backlinks). The
/// explorer comes with step 2.6 and the leader popup with step 2.9.
#[test]
fn mockup_02_side_panel() {
    let env = Env::new();
    let mut app = app_with_recent(&env);
    for name in ["Desain indeks Patinote", "Patinote", "2026-11-19"] {
        command(&mut app, &format!("e {name}"));
    }
    command(&mut app, "b 1");
    common::keys(&mut app, "12G");
    common::leader(&mut app, "b");
    common::press(&mut app, "Escape");
    let mut h = Harness::new();
    for scale in SCALES {
        let frame = h.render(app.view().map(|_| ()), size(), scale);
        frame.save(&out_dir().join(file_name("02-side-panel", scale)));
        let area = |px: f32| (px * scale * scale) as usize;
        assert!(frame.count(BG_PANEL) >= area(100_000.0), "side panel");
        assert!(
            frame.count(ACCENT) >= area(2_000.0),
            "the panel button is on"
        );
        assert!(frame.count(BG_CARD) >= area(10_000.0), "backlink cards");
    }
}

/// Mockup 01's backlinks under the note, on a note short enough to show
/// them (the fixture's main note is longer than the window).
#[test]
fn mockup_01_backlinks_under_the_note() {
    let env = Env::new();
    let mut app = app_with_recent(&env);
    command(&mut app, "e Patinote");
    let mut h = Harness::new();
    let frame = h.render(app.view().map(|_| ()), size(), 1.0);
    frame.save(&out_dir().join("01-minimal-backlinks.png"));
    assert!(
        frame.count(BG_CARD_INLINE) >= 20_000,
        "backlink cards under the note"
    );
}

/// Mockup 02 with the explorer: explorer, editor and side panel. The leader
/// popup comes with step 2.9.
#[test]
fn mockup_02_explorer_and_panel() {
    let env = Env::new();
    let mut app = app_with_recent(&env);
    for name in ["Desain indeks Patinote", "Patinote", "2026-11-19"] {
        command(&mut app, &format!("e {name}"));
    }
    command(&mut app, "b 1");
    common::keys(&mut app, "12G");
    common::leader(&mut app, "b");
    common::press(&mut app, "Escape");
    common::leader(&mut app, "e");
    common::press(&mut app, "Escape");
    let mut h = Harness::new();
    for scale in SCALES {
        let frame = h.render(app.view().map(|_| ()), size(), scale);
        frame.save(&out_dir().join(file_name("02-minimal-panels-open", scale)));
        let area = |px: f32| (px * scale * scale) as usize;
        assert!(
            frame.count(BG_PANEL) >= area(200_000.0),
            "explorer and side panel"
        );
        assert!(
            frame.count(BG_SELECTED) >= area(3_000.0),
            "the open note in the explorer"
        );
    }
}

/// Mockup 09: the rename and delete dialogs, one picture each.
#[test]
fn mockup_09_rename_and_delete_dialogs() {
    let env = Env::new();
    let mut app = app_with_recent(&env);
    common::leader(&mut app, "e");
    app.select_in_explorer("Proyek/Rapat.md");
    common::press(&mut app, "r");
    common::send(
        &mut app,
        patinote_app::app::Message::DialogInput("Rapat IKN".into()),
    );
    let mut h = Harness::new();
    let frame = h.render(app.view().map(|_| ()), size(), 1.0);
    frame.save(&out_dir().join("09-rename-delete-dialogs-rename.png"));
    assert!(frame.count(BG_OVERLAY) >= 60_000, "rename dialog");
    common::press(&mut app, "Escape");
    app.select_in_explorer("Proyek/Rapat.md");
    common::press(&mut app, "d");
    let frame = h.render(app.view().map(|_| ()), size(), 1.0);
    frame.save(&out_dir().join("09-rename-delete-dialogs-delete.png"));
    assert!(
        frame.count(MODE_CONFLICT) >= 2_000,
        "orange Move to trash button"
    );
}

/// Mockup 14: new note with a name warning, the explorer's input with an
/// error, the move picker, and deleting a folder; one picture each.
#[test]
fn mockup_14_create_move_delete_folder() {
    use patinote_app::app::Message;
    let env = Env::new();
    let mut app = app_with_recent(&env);
    let mut h = Harness::new();
    let big = Size::new(SIZE_WINDOW.0, 1180.0);

    common::leader(&mut app, "n");
    common::send(&mut app, Message::DialogInput("Ide #rapat".into()));
    common::send(&mut app, Message::DialogSubmit);
    let frame = h.render(app.view().map(|_| ()), big, 1.0);
    frame.save(&out_dir().join("14-create-move-delete-folder-new-note.png"));
    assert!(
        frame.count_near(STATUS_WARNING, 16) >= 60,
        "warning edge and icon"
    );
    common::press_with(
        &mut app,
        iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape),
        iced::keyboard::Modifiers::default(),
        None,
        true,
    );

    common::leader(&mut app, "e");
    app.select_in_explorer("Proyek");
    common::press(&mut app, "a");
    common::send(&mut app, Message::InlineInput("Rapat: evaluasi".into()));
    let frame = h.render(app.view().map(|_| ()), big, 1.0);
    frame.save(&out_dir().join("14-create-move-delete-folder-inline.png"));
    assert!(
        frame.count_near(STATUS_ERROR, 16) >= 60,
        "rejected edge and reason"
    );
    common::press_with(
        &mut app,
        iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape),
        iced::keyboard::Modifiers::default(),
        None,
        true,
    );

    app.select_in_explorer("Rapat/Rapat 19 Nov.md");
    common::press(&mut app, "m");
    common::send(&mut app, Message::DialogInput("pro".into()));
    let frame = h.render(app.view().map(|_| ()), big, 1.0);
    frame.save(&out_dir().join("14-create-move-delete-folder-move.png"));
    assert!(frame.count(BG_SELECTED) >= 5_000, "selected folder");
    common::press_with(
        &mut app,
        iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape),
        iced::keyboard::Modifiers::default(),
        None,
        true,
    );

    app.select_in_explorer("Rapat");
    common::press(&mut app, "d");
    let frame = h.render(app.view().map(|_| ()), big, 1.0);
    frame.save(&out_dir().join("14-create-move-delete-folder-delete.png"));
    assert!(
        frame.count(MODE_CONFLICT) >= 2_000,
        "orange Move to trash button"
    );
}
