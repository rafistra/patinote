//! Popups that hang under the cursor (F-08): link suggestions (mockup 06)
//! and the picker for an ambiguous link (mockup 12). Both follow the list
//! picker pattern of 0003 section 6: `bg.overlay` box, selected item on
//! `bg.selected`, matched letters in `accent`, key hints at the bottom.

use iced::widget::{Column, Row, Space, button, column, container, rich_text, row, span};
use iced::{Alignment, Element, Length, Padding};

use crate::screens::switcher::hints;
use crate::theme::styles;
use crate::theme::tokens::*;
use crate::widgets::typed;

/// One suggestion row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Suggestion {
    /// A note or attachment: its title (or alias) with matched positions,
    /// the note's title after `alias →` when an alias matched, and folder.
    Note {
        text: String,
        matched: Vec<usize>,
        alias_of: Option<String>,
        folder: String,
    },
    /// A heading with its level (`#` to `######`).
    Heading { level: u8, text: String },
    /// A block ID, without `^`.
    Block(String),
}

/// Text in runs, matched letters in bold `accent`, one text widget per run
/// (see `screens/switcher.rs`).
fn marked<'a, Message: 'a>(
    text: &str,
    matched: &[usize],
    base: iced::Color,
    font: iced::Font,
) -> Vec<Element<'a, Message>> {
    let mut parts = Vec::new();
    let mut run = String::new();
    let mut run_hit = false;
    let push = |run: String, hit: bool, parts: &mut Vec<Element<'a, Message>>| {
        let (font, color) = if hit {
            (FONT_UI_BOLD, ACCENT)
        } else {
            (font, base)
        };
        parts.push(typed(run, TYPE_UI).font(font).color(color).into());
    };
    for (i, c) in text.chars().enumerate() {
        let hit = matched.binary_search(&i).is_ok();
        if hit != run_hit && !run.is_empty() {
            push(std::mem::take(&mut run), run_hit, &mut parts);
        }
        run_hit = hit;
        run.push(c);
    }
    if !run.is_empty() {
        push(run, run_hit, &mut parts);
    }
    parts
}

/// The suggestions box (mockup 06), `size.autocomplete.w` wide.
pub fn suggestions<'a, Message: Clone + 'a>(
    items: &[Suggestion],
    selected: usize,
    typed_text: &str,
    on_pick: impl Fn(usize) -> Message,
) -> Element<'a, Message> {
    let mut list = Column::new().spacing(SPACE_2);
    if items.is_empty() {
        let rich: iced::widget::text::Rich<'a, (), Message> = rich_text![
            span("Enter").font(FONT_MONO_SEMIBOLD).color(TEXT_PRIMARY),
            span(" inserts ").font(FONT_UI).color(TEXT_MUTED),
            span(format!("[[{typed_text}]]"))
                .font(FONT_MONO)
                .color(SYNTAX_LINK),
            span(" as a link to a new note")
                .font(FONT_UI)
                .color(TEXT_MUTED),
        ]
        .size(TYPE_UI_SMALL.size);
        list = list.push(container(rich).padding(Padding::from([SPACE_8, SPACE_10])));
    }
    for (i, item) in items.iter().enumerate() {
        let sel = i == selected;
        let base = if sel { TEXT_STRONG } else { TEXT_PRIMARY };
        let line: Element<'a, Message> = match item {
            Suggestion::Note {
                text,
                matched,
                alias_of,
                folder,
            } => {
                let mut parts = marked(text, matched, base, FONT_UI);
                if let Some(title) = alias_of {
                    parts.push(typed(" alias → ", TYPE_UI).color(TEXT_SUBTLE).into());
                    parts.push(typed(title.clone(), TYPE_UI).color(base).into());
                }
                row![
                    Row::with_children(parts).width(Length::Fill),
                    typed(folder.clone(), TYPE_UI_SMALL).color(if sel {
                        TEXT_SECONDARY
                    } else {
                        TEXT_MUTED
                    }),
                ]
                .spacing(SPACE_12)
                .align_y(Alignment::Center)
                .into()
            }
            Suggestion::Heading { level, text } => {
                // Indented 12 px per level below the first (0003 section 7).
                let indent = SPACE_12 * f32::from(level.saturating_sub(1));
                row![
                    Space::new().width(indent),
                    typed("#".repeat(usize::from(*level)), TYPE_MONO_UI).color(TEXT_MUTED),
                    typed(text.clone(), TYPE_UI).color(base),
                ]
                .spacing(SPACE_8)
                .align_y(Alignment::Center)
                .into()
            }
            Suggestion::Block(id) => typed(format!("^{id}"), TYPE_MONO_UI).color(base).into(),
        };
        list = list.push(
            button(line)
                .width(Length::Fill)
                .padding(Padding::from([SPACE_8, SPACE_10]))
                .on_press(on_pick(i))
                .style(styles::list_button(sel)),
        );
    }
    container(column![
        list,
        container(Space::new().width(Length::Fill).height(SIZE_BORDER)).style(styles::rule),
        container(hints(vec![
            ("Enter/Tab", "insert".to_owned()),
            ("#", "heading".to_owned()),
            ("#^", "block".to_owned()),
            ("Esc", "close".to_owned()),
        ]))
        .padding(Padding::from([SPACE_8, SPACE_10])),
    ])
    .width(SIZE_AUTOCOMPLETE_W)
    .padding(SPACE_6)
    .style(styles::overlay(RADIUS_POPUP))
    .into()
}

/// One note in the ambiguous link picker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Choice {
    pub title: String,
    pub folder: String,
    /// `edited 2 days ago`, `edited Mar 2025`.
    pub edited: String,
}

/// The picker for a link that matches several notes (mockup 12),
/// `size.picker.w` wide.
pub fn picker<'a, Message: Clone + 'a>(
    name: &str,
    choices: &[Choice],
    selected: usize,
    precise: &str,
    on_pick: impl Fn(usize) -> Message,
) -> Element<'a, Message> {
    let mut list = Column::new().spacing(SPACE_2);
    for (i, c) in choices.iter().enumerate() {
        let sel = i == selected;
        let line = row![
            typed(c.title.clone(), TYPE_UI)
                .font(if sel { FONT_UI_MEDIUM } else { FONT_UI })
                .color(if sel { TEXT_STRONG } else { TEXT_PRIMARY }),
            typed(c.folder.clone(), TYPE_UI_SMALL).color(if sel {
                TEXT_SECONDARY
            } else {
                TEXT_MUTED
            }),
            Space::new().width(Length::Fill),
            typed(c.edited.clone(), TYPE_UI_SMALL).color(if sel {
                TEXT_SECONDARY
            } else {
                TEXT_MUTED
            }),
        ]
        .spacing(SPACE_6)
        .align_y(Alignment::Center);
        list = list.push(
            button(line)
                .width(Length::Fill)
                .padding(Padding::from([SPACE_8, SPACE_10]))
                .on_press(on_pick(i))
                .style(styles::list_button(sel)),
        );
    }
    let advice: iced::widget::text::Rich<'a, (), Message> = rich_text![
        span("Write ").font(FONT_UI).color(TEXT_MUTED),
        span(format!("[[{precise}]]"))
            .font(FONT_MONO)
            .color(SYNTAX_LINK),
        span(" to make the link point to one note.")
            .font(FONT_UI)
            .color(TEXT_MUTED),
    ]
    .size(TYPE_UI_SMALL.size);
    let rule =
        || container(Space::new().width(Length::Fill).height(SIZE_BORDER)).style(styles::rule);
    container(column![
        container(
            typed(
                format!("{} notes are named \"{name}\"", choices.len()),
                TYPE_UI_SMALL
            )
            .font(FONT_UI_SEMIBOLD)
            .color(TEXT_STRONG)
        )
        .padding(Padding {
            top: SPACE_12,
            right: SPACE_14,
            bottom: SPACE_8,
            left: SPACE_14,
        }),
        container(list).padding(Padding {
            top: 0.0,
            right: SPACE_6,
            bottom: SPACE_6,
            left: SPACE_6,
        }),
        rule(),
        container(advice).padding(Padding::from([SPACE_8, SPACE_14])),
        rule(),
        container(hints(vec![
            ("Ctrl+n/p", "move".to_owned()),
            ("Enter", "open".to_owned()),
            ("Esc", "cancel".to_owned()),
        ]))
        .padding(Padding::from([SPACE_8, SPACE_14])),
    ])
    .width(SIZE_PICKER_W)
    .style(styles::overlay(RADIUS_POPUP))
    .into()
}

/// `edited 2 days ago`, or `edited Mar 2025` for older edits (mockup 12).
pub fn edited(mtime_ns: i64, now_secs: u64) -> String {
    let secs = u64::try_from(mtime_ns.div_euclid(1_000_000_000)).unwrap_or(0);
    let ago = now_secs.saturating_sub(secs);
    if ago < 30 * 86_400 {
        return format!("edited {}", super::start::relative_time(ago));
    }
    let iso = patinote_core::time::iso8601_utc(mtime_ns);
    let month: usize = iso[5..7].parse().unwrap_or(1);
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    format!("edited {} {}", MONTHS[month.clamp(1, 12) - 1], &iso[..4])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edit_times_read_like_mockup_12() {
        let now = 1_795_102_530;
        assert_eq!(
            edited((now - 2 * 86_400 - 60) as i64 * 1_000_000_000, now),
            "edited 2 days ago"
        );
        // 2025-03-15
        assert_eq!(
            edited(1_742_000_000 * 1_000_000_000, now),
            "edited Mar 2025"
        );
    }
}
