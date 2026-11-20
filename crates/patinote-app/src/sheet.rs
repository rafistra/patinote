//! The component sheet of mockup 17 (banners, status line variants and
//! command line messages), drawn with the real widgets. The UI snapshot test
//! renders it, and `patinote-app` shows it until the app shell (step 2.2).
//!
//! The texts are copied from the mockup. The sheet background and the
//! spacing between samples belong to the sheet, not to the app.

use iced::widget::{Space, column, container, row};
use iced::{Element, Length, Padding};

use crate::mode::Mode;
use crate::theme::styles::{self, BannerKind, Level};
use crate::theme::tokens::*;
use crate::widgets::banner::{Action, Banner, Icon};
use crate::widgets::statusline::{Indicator, StatusLine};
use crate::widgets::{cmdline, label, typed};

/// Everything the sheet shows; built once, then viewed.
pub struct Sheet {
    banners: Vec<(&'static str, Banner<()>, Option<&'static str>)>,
    status: Vec<(&'static str, StatusLine)>,
}

/// A sample on the sheet, framed like the mockup's cards.
fn framed<'a>(content: Element<'a, ()>) -> container::Container<'a, ()> {
    container(content)
        .padding(SIZE_BORDER)
        .style(styles::frame)
        .clip(true)
}

fn action(label: &str, primary: bool) -> Action<()> {
    Action {
        label: label.into(),
        primary,
        on_press: (),
    }
}

impl Default for Sheet {
    fn default() -> Self {
        Self::new()
    }
}

impl Sheet {
    /// The samples of mockup 17.
    pub fn new() -> Self {
        let banners = vec![
            (
                "Warning · note deleted from outside (F-11)",
                Banner {
                    kind: BannerKind::Warning,
                    icon: Icon::Warning,
                    title: "Rapat 19 Nov.md was deleted outside Patinote.".into(),
                    body: "Your text is still here in the editor. Autosave is paused for this note.".into(),
                    code: None,
                    actions: vec![action("Close", false), action("Save again", true)],
                },
                None,
            ),
            (
                "Error · save failed (F-05)",
                Banner {
                    kind: BannerKind::Error,
                    icon: Icon::Error,
                    title: "Couldn't save Rapat 19 Nov.md: the disk is full.".into(),
                    body: "Your text is safe in the editor and the file on disk is unchanged. Patinote keeps retrying."
                        .into(),
                    code: None,
                    actions: vec![action("Retry now", true)],
                },
                Some("Error is the only red in the app. The status line shows ✕ not saved until a save succeeds."),
            ),
            (
                "Info · read-only file (non-UTF-8 or larger than 5 MB)",
                Banner {
                    kind: BannerKind::Info,
                    icon: Icon::Lock,
                    title: "Read-only: this note is larger than 5 MB.".into(),
                    body: "Patinote shows it but doesn't edit or index it.".into(),
                    code: None,
                    actions: vec![action("Open in default app", false)],
                },
                Some("Non-UTF-8 files use the same banner: “Read-only: this file isn't UTF-8 text.”"),
            ),
            (
                "Info · watcher limit (F-04)",
                Banner {
                    kind: BannerKind::Info,
                    icon: Icon::Info,
                    title: "File watching is limited on this system.".into(),
                    body: "Changes made in other apps show up within 30 s instead of instantly. To fix it, run:".into(),
                    code: Some("sudo sysctl fs.inotify.max_user_watches=524288".into()),
                    actions: vec![action("Copy command", false), action("Dismiss", false)],
                },
                Some("Shown once per launch when max_user_watches is too low or the watcher stops."),
            ),
        ];
        let line = |mode, path: &str, modified, notes: &[&str], indicator| StatusLine {
            mode,
            path: path.into(),
            modified,
            notes: notes.iter().map(|n| (*n).to_owned()).collect(),
            indicator: Some(indicator),
            ..StatusLine::default()
        };
        let status = vec![
            (
                "Status line · first index running (F-03)",
                line(
                    Mode::Normal,
                    "Harian/2026-11-19.md",
                    false,
                    &[],
                    Indicator::Indexing {
                        done: 1_240,
                        total: 2_143,
                    },
                ),
            ),
            (
                "Status line · watcher off, polling (F-04)",
                line(
                    Mode::Normal,
                    "Harian/2026-11-19.md",
                    false,
                    &[],
                    Indicator::Polling { seconds: 30 },
                ),
            ),
            (
                "Status line · save failing (F-05)",
                line(
                    Mode::Insert,
                    "Rapat/Rapat 19 Nov.md",
                    true,
                    &[],
                    Indicator::NotSaved,
                ),
            ),
            (
                "Status line · read-only note",
                line(
                    Mode::Read,
                    "Arsip/Ekspor besar.md",
                    false,
                    &["read-only"],
                    Indicator::Indexed,
                ),
            ),
        ];
        Self { banners, status }
    }

    /// Draws the sheet.
    pub fn view(&self) -> Element<'_, ()> {
        let section = |title: &str| label(title).color(TEXT_SUBTLE);
        let note = |text: &'static str| typed(text, TYPE_UI_SMALL).color(TEXT_MUTED);

        // Column spacing is 8 px. The mockup puts 18 px under a column title
        // (8 + 2 + 8) and 28 px before every later sample (8 + 12 + 8).
        let gap = |i: usize| Space::new().height(if i == 0 { SPACE_2 } else { SPACE_12 });

        let mut left = column![section("Banners · appear under the bufferline")].spacing(SPACE_8);
        for (i, (title, banner, caption)) in self.banners.iter().enumerate() {
            left = left.push(gap(i));
            left = left.push(section(title));
            left = left.push(framed(banner.view()));
            if let Some(c) = caption {
                left = left.push(note(c));
            }
        }

        let mut right = column![section("Status line and command line")].spacing(SPACE_8);
        for (i, (title, line)) in self.status.iter().enumerate() {
            right = right.push(gap(i));
            right = right.push(section(title));
            right = right.push(framed(line.view()));
        }
        right = right.push(Space::new().height(SPACE_12));
        right = right.push(section("Command line messages"));
        right = right.push(framed(
            column![
                cmdline::message(Level::Info, "\"Rapat 19 Nov.md\" updated from disk"),
                container(Space::new().width(Length::Fill).height(SIZE_BORDER))
                    .style(styles::rule_faint),
                cmdline::message(Level::Warning, "2 notes are named \"Rapat\""),
                container(Space::new().width(Length::Fill).height(SIZE_BORDER))
                    .style(styles::rule_faint),
                cmdline::message(Level::Error, "Can't save: the disk is full"),
            ]
            .into(),
        ));
        right = right.push(note(
            "Info in muted text, warnings start with W:, errors with E:. A message stays until the next key press, except errors, which stay for 5 s.",
        ));

        container(
            row![
                container(left).width(Length::FillPortion(5)),
                container(right).width(Length::FillPortion(4)),
            ]
            .spacing(SPACE_48),
        )
        .padding(Padding::from([SPACE_48, SPACE_48 + SPACE_16]))
        .width(Length::Fill)
        .height(Length::Fill)
        .style(styles::chrome)
        .into()
    }
}
