//! The side panel (F-09, mockup 02): Outline above, Backlinks below; and
//! the backlinks under the note when the panel is closed (mockup 01).

use iced::widget::{Column, Space, button, column, container, rich_text, row, span};
use iced::{Alignment, Element, Length, Padding};

use crate::theme::styles;
use crate::theme::tokens::*;
use crate::widgets::{label, typed};

/// One heading in the outline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutlineItem {
    pub level: u8,
    pub text: String,
    /// 0-based line in the note.
    pub line: usize,
}

/// One link into the note.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BacklinkItem {
    /// The source note's path and title.
    pub path: String,
    pub title: String,
    /// 1-based line of the link in the source.
    pub line: usize,
    /// `→ #Tabel inti`, or `property` for a frontmatter link.
    pub marker: Option<String>,
    /// The line around the link.
    pub context: String,
}

/// Which item has the keyboard focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focused {
    Outline(usize),
    Backlink(usize),
}

/// The heading the cursor is under: the last one at or above it.
pub fn current_heading(outline: &[OutlineItem], cursor_line: usize) -> Option<usize> {
    outline.iter().rposition(|h| h.line <= cursor_line)
}

/// A snippet of `context` around its first link, with `…` where it was cut,
/// as runs of (text, is_link).
pub fn snippet(context: &str, max_chars: usize) -> Vec<(String, bool)> {
    let chars: Vec<char> = context.chars().collect();
    let link = (0..chars.len().saturating_sub(1)).find(|&i| chars[i] == '[' && chars[i + 1] == '[');
    let (start, end) = if chars.len() <= max_chars {
        (0, chars.len())
    } else {
        let at = link.unwrap_or(0);
        let start = at.saturating_sub(max_chars / 4);
        (start, (start + max_chars).min(chars.len()))
    };
    let mut text: String = chars[start..end].iter().collect();
    if start > 0 {
        text.insert(0, '…');
    }
    if end < chars.len() {
        text.push('…');
    }
    // Split into plain text and `[[links]]`.
    let mut runs = Vec::new();
    let mut rest = text.as_str();
    while let Some(open) = rest.find("[[") {
        let Some(close) = rest[open..].find("]]").map(|c| open + c + 2) else {
            break;
        };
        if open > 0 {
            runs.push((rest[..open].to_owned(), false));
        }
        runs.push((rest[open..close].to_owned(), true));
        rest = &rest[close..];
    }
    if !rest.is_empty() {
        runs.push((rest.to_owned(), false));
    }
    runs
}

fn snippet_text<'a, Message: 'a>(
    context: &str,
    max_chars: usize,
    color: iced::Color,
) -> Element<'a, Message> {
    let spans: Vec<_> = snippet(context, max_chars)
        .into_iter()
        .map(|(t, link)| {
            span(t)
                .font(FONT_UI)
                .color(if link { SYNTAX_LINK } else { color })
        })
        .collect();
    let rich: iced::widget::text::Rich<'a, (), Message> =
        rich_text(spans).size(TYPE_UI_SMALL.size).line_height(
            iced::widget::text::LineHeight::Absolute(TYPE_UI_SMALL.line_height.into()),
        );
    rich.into()
}

/// Backlinks grouped by source note (F-09): runs of items with the same
/// path, as (index of the first item, items).
pub fn groups(items: &[BacklinkItem]) -> Vec<(usize, &[BacklinkItem])> {
    let mut out = Vec::new();
    let mut start = 0;
    for i in 1..=items.len() {
        if i == items.len() || items[i].path != items[start].path {
            out.push((start, &items[start..i]));
            start = i;
        }
    }
    out
}

/// A card for one source note: its title, then a snippet per link with its
/// marker (`→ #Tabel inti`, `property`). Pressing it opens the first link.
fn card<'a, Message: Clone + 'a>(
    group: &[BacklinkItem],
    focused: bool,
    inline: bool,
    on_press: Message,
) -> Element<'a, Message> {
    let first = &group[0];
    let mut title = row![
        typed(first.title.clone(), TYPE_UI)
            .font(FONT_UI_SEMIBOLD)
            .color(TEXT_STRONG)
    ]
    .spacing(SPACE_6)
    .align_y(Alignment::Center);
    // A single link shows its marker next to the title, as in mockup 02.
    if group.len() == 1
        && let Some(marker) = &first.marker
    {
        title = title.push(typed(marker.clone(), TYPE_UI_SMALL).color(TEXT_MUTED));
    }
    let snippet_color = if focused { TEXT_SECONDARY } else { TEXT_MUTED };
    let mut body = column![title].spacing(SPACE_4);
    for b in group {
        let snippet = snippet_text(&b.context, if inline { 64 } else { 96 }, snippet_color);
        body = body.push(match (&b.marker, group.len()) {
            (Some(marker), n) if n > 1 => column![
                typed(marker.clone(), TYPE_UI_SMALL).color(TEXT_MUTED),
                snippet
            ]
            .into(),
            _ => snippet,
        });
    }
    button(body)
        .width(Length::Fill)
        .padding(Padding::from([SPACE_12, SPACE_14]))
        .on_press(on_press)
        .style(styles::backlink_card(inline, focused))
        .into()
}

/// What the side panel shows.
pub struct Panel<Message> {
    pub outline: Vec<OutlineItem>,
    /// The heading the cursor is under.
    pub current: Option<usize>,
    pub backlinks: Vec<BacklinkItem>,
    pub focused: Option<Focused>,
    pub on_outline: fn(usize) -> Message,
    pub on_backlink: fn(usize) -> Message,
}

impl<Message: Clone> Panel<Message> {
    /// The panel, `size.sidepanel.w` wide.
    pub fn view<'a>(self) -> Element<'a, Message>
    where
        Message: 'a,
    {
        let mut outline = Column::new().spacing(SPACE_2);
        if self.outline.is_empty() {
            outline = outline.push(typed("No headings", TYPE_UI_SMALL).color(TEXT_MUTED));
        }
        for (i, h) in self.outline.iter().enumerate() {
            let current = self.current == Some(i);
            let focused = self.focused == Some(Focused::Outline(i));
            let indent = SPACE_14 * f32::from(h.level.saturating_sub(1));
            let item = button(row![
                Space::new().width(indent),
                typed(h.text.clone(), TYPE_UI_SMALL)
                    .font(if current { FONT_UI_MEDIUM } else { FONT_UI })
                    .color(if current { ACCENT } else { TEXT_SECONDARY }),
            ])
            .width(Length::Fill)
            .padding(Padding::from([SPACE_4, 0.0]))
            .on_press((self.on_outline)(i))
            .style(styles::list_button(focused));
            outline = outline.push(item);
        }

        let mut links = Column::new().spacing(SPACE_10);
        if self.backlinks.is_empty() {
            links = links.push(typed("No backlinks", TYPE_UI_SMALL).color(TEXT_MUTED));
        }
        for (first, group) in groups(&self.backlinks) {
            let focused = matches!(self.focused, Some(Focused::Backlink(i)) if (first..first + group.len()).contains(&i));
            links = links.push(card(group, focused, false, (self.on_backlink)(first)));
        }

        let count = self.backlinks.len();
        let heading = if count == 1 {
            "1 backlink".to_owned()
        } else {
            format!("{count} backlinks")
        };
        container(
            iced::widget::scrollable(column![
                label("Outline").color(TEXT_SUBTLE),
                Space::new().height(SPACE_8),
                outline,
                Space::new().height(SPACE_24),
                label(&heading).color(TEXT_SUBTLE),
                Space::new().height(SPACE_10),
                links,
            ])
            .height(Length::Fill),
        )
        .width(SIZE_SIDEPANEL_W)
        .height(Length::Fill)
        .padding(PAD_PANEL)
        .style(styles::side_panel)
        .into()
    }
}

/// The backlinks under the note when the panel is closed (mockup 01): a
/// rule, the count, then cards two to a row.
pub fn inline<'a, Message: Clone + 'a>(
    backlinks: &[BacklinkItem],
    on_press: fn(usize) -> Message,
) -> Element<'a, Message> {
    let count = backlinks.len();
    let heading = if count == 1 {
        "1 backlink".to_owned()
    } else {
        format!("{count} backlinks")
    };
    let mut grid = Column::new().spacing(SPACE_10);
    let cards = groups(backlinks);
    for pair in cards.chunks(2) {
        let mut line = row![].spacing(SPACE_10);
        for (first, group) in pair {
            line = line.push(
                container(card(group, false, true, on_press(*first))).width(Length::FillPortion(1)),
            );
        }
        if pair.len() == 1 {
            line = line.push(Space::new().width(Length::FillPortion(1)));
        }
        grid = grid.push(line);
    }
    column![
        container(Space::new().width(Length::Fill).height(SIZE_BORDER)).style(styles::rule),
        Space::new().height(SPACE_20),
        label(&heading).color(TEXT_SUBTLE),
        Space::new().height(SPACE_12),
        grid,
    ]
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backlinks_are_grouped_by_source() {
        let item = |path: &str| BacklinkItem {
            path: path.into(),
            title: path.into(),
            line: 1,
            marker: None,
            context: String::new(),
        };
        let items = [item("a"), item("b"), item("b"), item("c")];
        let got: Vec<(usize, usize)> = groups(&items).iter().map(|(f, g)| (*f, g.len())).collect();
        assert_eq!(got, [(0, 1), (1, 2), (3, 1)]);
    }

    #[test]
    fn the_current_heading_is_the_last_one_above_the_cursor() {
        let outline = vec![
            OutlineItem {
                level: 1,
                text: "A".into(),
                line: 5,
            },
            OutlineItem {
                level: 2,
                text: "B".into(),
                line: 11,
            },
        ];
        assert_eq!(current_heading(&outline, 3), None);
        assert_eq!(current_heading(&outline, 5), Some(0));
        assert_eq!(current_heading(&outline, 12), Some(1));
    }

    #[test]
    fn snippets_cut_around_the_link_and_mark_it() {
        let runs = snippet(
            "Lanjut [[Desain indeks Patinote#Tabel inti]] setelah makan siang",
            200,
        );
        assert_eq!(
            runs,
            [
                ("Lanjut ".to_owned(), false),
                ("[[Desain indeks Patinote#Tabel inti]]".to_owned(), true),
                (" setelah makan siang".to_owned(), false),
            ]
        );
        let long = format!("{} [[X]] {}", "a ".repeat(40), "b ".repeat(40));
        let runs = snippet(&long, 30);
        assert!(runs[0].0.starts_with('…'));
        assert!(runs.last().unwrap().0.ends_with('…'));
        assert!(runs.iter().any(|(t, l)| *l && t == "[[X]]"));
    }
}
