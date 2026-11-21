//! The quick switcher box (F-07, mockup 05): input, results with matched
//! letters highlighted, folders, and the key hints.

use iced::widget::{Column, Space, button, column, container, rich_text, row, span, text_input};
use iced::{Alignment, Element, Length, Padding};

use crate::switcher::{Candidate, Hit, Matched, Results};
use crate::theme::styles;
use crate::theme::tokens::*;
use crate::widgets::banner::Icon;
use crate::widgets::statusline::thousands;
use crate::widgets::{icon, typed};

/// The id of the switcher input, to focus it.
pub const INPUT: &str = "switcher";

/// What the switcher shows.
pub struct View<'a, Message> {
    pub query: &'a str,
    pub results: &'a Results,
    pub candidates: &'a [Candidate],
    pub selected: usize,
    /// No note has been opened in this vault yet.
    pub no_recent: bool,
    pub on_input: fn(String) -> Message,
    pub on_submit: Message,
    pub on_pick: fn(usize) -> Message,
}

/// A title (or alias) with matched characters in bold `accent`, one text
/// widget per run. (One `rich_text` would be simpler, but in Iced 0.14 the
/// bold of a matched letter spreads to the rest of its word.)
fn highlighted<'a, Message: 'a>(
    text: &str,
    matched: &[usize],
    base: iced::Color,
    font: iced::Font,
) -> Vec<Element<'a, Message>> {
    let mut parts = Vec::new();
    let mut run = String::new();
    let mut run_hit = false;
    for (i, c) in text.chars().enumerate() {
        let hit = matched.binary_search(&i).is_ok();
        if hit != run_hit && !run.is_empty() {
            parts.push(piece(std::mem::take(&mut run), run_hit, base, font));
        }
        run_hit = hit;
        run.push(c);
    }
    if !run.is_empty() {
        parts.push(piece(run, run_hit, base, font));
    }
    parts
}

fn piece<'a, Message: 'a>(
    text: String,
    hit: bool,
    base: iced::Color,
    font: iced::Font,
) -> Element<'a, Message> {
    let (font, color) = if hit {
        (FONT_UI_BOLD, ACCENT)
    } else {
        (font, base)
    };
    typed(text, TYPE_UI).font(font).color(color).into()
}

/// One result row.
fn row_view<'a, Message: Clone + 'a>(
    c: &Candidate,
    hit: &Hit,
    selected: bool,
    on_press: Message,
) -> Element<'a, Message> {
    let base = if selected { TEXT_STRONG } else { TEXT_PRIMARY };
    let font = if selected { FONT_UI_MEDIUM } else { FONT_UI };
    let parts = match &hit.matched {
        Matched::Title(idx) => highlighted(&c.title, idx, base, font),
        Matched::Alias(n, idx) => {
            let mut p = highlighted(&c.aliases[*n], idx, base, font);
            p.push(typed(" alias → ", TYPE_UI).color(TEXT_SUBTLE).into());
            p.push(
                typed(c.title.clone(), TYPE_UI)
                    .font(font)
                    .color(base)
                    .into(),
            );
            p
        }
    };
    let title = iced::widget::Row::with_children(parts).width(Length::Fill);
    let line = row![
        title,
        typed(c.folder.clone(), TYPE_UI_SMALL).color(if selected {
            TEXT_SECONDARY
        } else {
            TEXT_MUTED
        }),
    ]
    .spacing(SPACE_16)
    .align_y(Alignment::Center);
    button(line)
        .width(Length::Fill)
        .padding(Padding::from([SPACE_10, SPACE_12]))
        .on_press(on_press)
        .style(styles::list_button(selected))
        .into()
}

/// `Ctrl+n/p move`: keys in `text.primary`, words in `text.muted`.
pub fn hints<'a, Message: 'a>(items: Vec<(&'static str, String)>) -> Element<'a, Message> {
    let mut line = row![].spacing(SPACE_16);
    for (key, what) in items {
        let rich: iced::widget::text::Rich<'a, (), Message> = rich_text![
            span(key).font(FONT_MONO_SEMIBOLD).color(TEXT_PRIMARY),
            span(" ").font(FONT_MONO),
            span(what).font(FONT_MONO).color(TEXT_MUTED),
        ]
        .font(TYPE_MONO_UI.font)
        .size(TYPE_MONO_UI.size);
        line = line.push(rich);
    }
    line.into()
}

/// A muted line in place of results (0003 section 7).
fn note<'a, Message: 'a>(text: String) -> Element<'a, Message> {
    container(typed(text, TYPE_UI).color(TEXT_MUTED))
        .padding(Padding::from([SPACE_10, SPACE_12]))
        .into()
}

impl<'a, Message: Clone + 'a> View<'a, Message> {
    /// The box, `size.switcher.w` wide.
    pub fn view(self) -> Element<'a, Message> {
        let count = if self.query.trim().is_empty() {
            String::new()
        } else {
            format!(
                "{} of {}",
                thousands(self.results.total_matches),
                thousands(self.candidates.len())
            )
        };
        let input = row![
            icon(Icon::Search, SIZE_LOGO, TEXT_MUTED),
            text_input("Find note", self.query)
                .id(INPUT)
                .on_input(self.on_input)
                .on_submit(self.on_submit.clone())
                .font(TYPE_FOCUS.font)
                .size(TYPE_FOCUS.size)
                .padding(0)
                .style(styles::switcher_input),
            typed(count, TYPE_MONO_UI).color(TEXT_SUBTLE),
        ]
        .spacing(SPACE_10)
        .align_y(Alignment::Center);

        let mut list = Column::new().spacing(SPACE_2);
        if self.results.hits.is_empty() {
            list = list.push(if self.query.trim().is_empty() {
                if self.no_recent {
                    note("No recent notes yet".into())
                } else {
                    note(String::new())
                }
            } else {
                note(format!("No notes match \"{}\"", self.query.trim()))
            });
        }
        for (i, hit) in self.results.hits.iter().enumerate() {
            let c = &self.candidates[hit.candidate];
            list = list.push(row_view(c, hit, i == self.selected, (self.on_pick)(i)));
        }

        let name = self.query.trim();
        let mut hint_items = vec![
            ("Ctrl+n/p", "move".to_owned()),
            ("Enter", "open".to_owned()),
        ];
        if !name.is_empty() {
            hint_items.push(("Shift+Enter", format!("create \"{name}\"")));
        }
        hint_items.push(("Esc", "close".to_owned()));
        let footer = hints(hint_items);

        container(column![
            container(input).padding(Padding::from([SPACE_14, SPACE_16])),
            container(Space::new().width(Length::Fill).height(SIZE_BORDER)).style(styles::rule),
            container(list).padding(SPACE_6),
            container(Space::new().width(Length::Fill).height(SIZE_BORDER)).style(styles::rule),
            container(footer).padding(Padding::from([SPACE_10, SPACE_16])),
        ])
        .width(SIZE_SWITCHER_W)
        .style(styles::overlay(RADIUS_DIALOG))
        .into()
    }
}
