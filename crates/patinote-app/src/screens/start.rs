//! The screen with no note open (F-28, mockup 11): the vault name, recent
//! notes, and the main shortcuts.

use iced::widget::{Column, Space, button, column, container, rich_text, row, span};
use iced::{Alignment, Element, Length, Padding};

use crate::theme::styles;
use crate::theme::tokens::*;
use crate::widgets::statusline::thousands;
use crate::widgets::{label, typed};

/// How many recent notes the screen lists.
pub const RECENT_SHOWN: usize = 6;

/// One recent note.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecentRow {
    pub title: String,
    /// Its folder with a trailing `/`, or empty at the vault root.
    pub folder: String,
    /// `2 min ago`, `yesterday`.
    pub when: String,
}

/// What the screen shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Start {
    pub name: String,
    /// The vault path as shown (`~/Vault`).
    pub path: String,
    /// Notes in the vault, once the index knows.
    pub notes: Option<usize>,
    pub recent: Vec<RecentRow>,
    pub selected: usize,
}

/// The shortcuts, in the order of mockup 11 (two columns, filled row by row).
const SHORTCUTS: [(&str, &str); 6] = [
    ("␣ f", "Find note"),
    ("␣ n", "New note"),
    ("␣ e", "Explorer"),
    ("␣ l", "Switch layout"),
    (":e name", "Open or create by name"),
    ("␣ c", "Conflicts"),
];

/// `j/k move · Enter open`, keys in `text.primary`.
fn key_hint<'a, Message: 'a>(parts: &[(&'a str, bool)]) -> Element<'a, Message> {
    let spans: Vec<_> = parts
        .iter()
        .map(|(text, key)| {
            if *key {
                span(*text).font(FONT_MONO_SEMIBOLD).color(TEXT_PRIMARY)
            } else {
                span(*text).font(FONT_MONO).color(TEXT_MUTED)
            }
        })
        .collect();
    let rich: iced::widget::text::Rich<'a, (), Message> = rich_text(spans)
        .font(TYPE_MONO_UI.font)
        .size(TYPE_MONO_UI.size);
    rich.into()
}

impl Start {
    /// Draws the screen; `on_open` gives the message for a recent note.
    pub fn view<'a, Message: Clone + 'a>(
        &self,
        on_open: impl Fn(usize) -> Message,
    ) -> Element<'a, Message> {
        let subtitle = match self.notes {
            Some(1) => format!("{} · 1 note", self.path),
            Some(n) => format!("{} · {} notes", self.path, thousands(n)),
            None => self.path.clone(),
        };
        // The recent list bleeds 12 px into the margin so its titles line up
        // with the heading (mockup 11): everything else is inset by 12 px.
        fn inset<'a, Message: 'a>(e: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
            container(e).padding(Padding::from([0.0, SPACE_12])).into()
        }
        let mut page = column![
            inset(typed(self.name.clone(), TYPE_TITLE).color(TEXT_STRONG)),
            Space::new().height(SPACE_4),
            inset(typed(subtitle, TYPE_MONO_UI).color(TEXT_MUTED)),
            Space::new().height(SPACE_32),
            inset(label("Recent notes").color(TEXT_SUBTLE)),
            Space::new().height(SPACE_10),
        ];

        if self.recent.is_empty() {
            page = page.push(inset(key_hint(&[
                ("No recent notes yet. Press ", false),
                ("␣f", true),
                (" to find a note or ", false),
                ("␣n", true),
                (" to create one.", false),
            ])));
        } else {
            let mut list = Column::new().spacing(SPACE_2);
            for (i, r) in self.recent.iter().enumerate() {
                let selected = i == self.selected;
                let (folder_color, when_color) = if selected {
                    (TEXT_SECONDARY, TEXT_SECONDARY)
                } else {
                    (TEXT_MUTED, TEXT_SUBTLE)
                };
                let title = typed(r.title.clone(), TYPE_UI)
                    .font(if selected { FONT_UI_MEDIUM } else { FONT_UI })
                    .color(if selected { TEXT_STRONG } else { TEXT_PRIMARY })
                    .width(Length::Fill);
                let line = row![
                    title,
                    typed(r.folder.clone(), TYPE_UI_SMALL).color(folder_color),
                    typed(r.when.clone(), TYPE_UI_SMALL)
                        .color(when_color)
                        .width(SIZE_RECENT_TIME_W)
                        .align_x(Alignment::End),
                ]
                .spacing(SPACE_12)
                .align_y(Alignment::Center);
                list = list.push(
                    button(line)
                        .width(Length::Fill)
                        .padding(Padding::from([SPACE_8, SPACE_12]))
                        .on_press(on_open(i))
                        .style(styles::list_button(selected)),
                );
            }
            page = page.push(list);
            page = page.push(Space::new().height(SPACE_8));
            page = page.push(inset(key_hint(&[
                ("j", true),
                ("/", false),
                ("k", true),
                (" move · ", false),
                ("Enter", true),
                (" open", false),
            ])));
        }

        page = page.push(Space::new().height(SPACE_32));
        page = page.push(inset(label("Shortcuts").color(TEXT_SUBTLE)));
        page = page.push(Space::new().height(SPACE_14));
        let mut grid = Column::new().spacing(SPACE_12);
        for pair in SHORTCUTS.chunks(2) {
            let mut line = row![].spacing(SPACE_24);
            for (key, what) in pair {
                line = line.push(
                    row![
                        typed(*key, TYPE_MONO_UI)
                            .font(FONT_MONO_BOLD)
                            .color(ACCENT)
                            .width(SIZE_SHORTCUT_KEY_W),
                        typed(*what, TYPE_UI).color(TEXT_SECONDARY),
                    ]
                    .spacing(SPACE_12)
                    .width(Length::FillPortion(1)),
                );
            }
            grid = grid.push(line);
        }
        page = page.push(inset(grid));

        container(container(page).max_width(SIZE_START_MAXW + SPACE_24))
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(Alignment::Center)
            .padding(Padding {
                top: SPACE_48 + SPACE_24,
                ..Padding::from(SPACE_32)
            })
            .into()
    }
}

/// A relative time like the mockup's: `2 min ago`, `yesterday`, `last week`.
pub fn relative_time(seconds_ago: u64) -> String {
    const MIN: u64 = 60;
    const HOUR: u64 = 60 * MIN;
    const DAY: u64 = 24 * HOUR;
    match seconds_ago {
        s if s < MIN => "just now".into(),
        s if s < HOUR => format!("{} min ago", s / MIN),
        s if s < DAY => format!("{} h ago", s / HOUR),
        s if s < 2 * DAY => "yesterday".into(),
        s if s < 7 * DAY => format!("{} days ago", s / DAY),
        s if s < 14 * DAY => "last week".into(),
        s if s < 60 * DAY => format!("{} weeks ago", s / (7 * DAY)),
        s => format!("{} months ago", s / (30 * DAY)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_times_read_like_the_mockup() {
        assert_eq!(relative_time(5), "just now");
        assert_eq!(relative_time(125), "2 min ago");
        assert_eq!(relative_time(3 * 3600), "3 h ago");
        assert_eq!(relative_time(30 * 3600), "yesterday");
        assert_eq!(relative_time(2 * 86_400 + 10), "2 days ago");
        assert_eq!(relative_time(8 * 86_400), "last week");
        assert_eq!(relative_time(21 * 86_400), "3 weeks ago");
        assert_eq!(relative_time(90 * 86_400), "3 months ago");
    }
}
