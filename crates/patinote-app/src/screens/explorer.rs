//! The explorer (F-10, mockups 02 and 14): the vault's folders and notes,
//! the tags with their counts, an input for a new note, folder or name in
//! the tree, and the key hints at the bottom.

use iced::widget::{Column, Space, button, column, container, row, text_input};
use iced::{Element, Length, Padding};

use crate::explorer::{Kind, Row};
use crate::theme::styles::{self, Field};
use crate::theme::tokens::*;
use crate::widgets::{label, typed};

/// The id of the explorer's name input.
pub const INPUT: &str = "explorer-input";
/// The id of the explorer's scrolling list.
pub const LIST: &str = "explorer-list";
/// The id of the selected row, scrolled into view when it moves.
pub const SELECTED: &str = "explorer-selected";

/// An input in the tree: a new note or folder in a folder, or a new name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Editing {
    /// The row the input replaces (rename) or follows (new); `None` at the
    /// top of the tree (vault root).
    pub at: Option<usize>,
    /// Indent level of the input.
    pub depth: usize,
    pub value: String,
    /// The reason under the input when the name is rejected or risky.
    pub problem: Option<(Field, String)>,
    /// Replaces the row (rename) instead of following it.
    pub replaces: bool,
}

/// What the explorer shows.
pub struct View<Message> {
    pub vault: String,
    pub rows: Vec<Row>,
    pub selected: Option<usize>,
    /// The note open in the editor, by path.
    pub active: Option<String>,
    /// (tag, uses, depth) for the tag list.
    pub tags: Vec<(String, usize, usize)>,
    pub editing: Option<Editing>,
    pub on_select: fn(usize) -> Message,
    pub on_input: fn(String) -> Message,
    pub on_submit: Message,
    pub width: f32,
    /// Classic mode's "Find note…" button (mockup 03).
    pub find: Option<Message>,
}

impl<Message: Clone> View<Message> {
    fn input<'a>(&self, e: &Editing) -> Element<'a, Message>
    where
        Message: 'a,
    {
        let state = e.problem.as_ref().map_or(Field::Normal, |(f, _)| *f);
        let mut col = column![
            text_input("", &e.value)
                .id(INPUT)
                .on_input(self.on_input)
                .on_submit(self.on_submit.clone())
                .size(TYPE_UI_SMALL.size)
                .padding(Padding::from([0.0, SPACE_8]))
                .width(Length::Fill)
                .style(styles::field(state)),
        ]
        .spacing(SPACE_4);
        if let Some((f, reason)) = &e.problem {
            let color = if *f == Field::Rejected {
                STATUS_ERROR
            } else {
                STATUS_WARNING
            };
            col = col.push(typed(reason.clone(), TYPE_UI_SMALL).color(color));
        }
        container(col)
            .padding(Padding {
                top: SPACE_2,
                right: SPACE_10,
                bottom: SPACE_2,
                left: SPACE_10 + SIZE_TREE_INDENT * e.depth as f32,
            })
            .height(Length::Shrink)
            .into()
    }

    /// The explorer, `size.explorer.w` wide.
    pub fn view<'a>(self) -> Element<'a, Message>
    where
        Message: 'a,
    {
        let mut tree = Column::new().spacing(SPACE_2);
        if self.rows.is_empty() && self.editing.is_none() {
            tree = tree.push(
                container(
                    typed(
                        "This vault is empty. Press a to create a note.",
                        TYPE_UI_SMALL,
                    )
                    .color(TEXT_MUTED),
                )
                .padding(Padding::from([SPACE_6, SPACE_10])),
            );
        }
        if let Some(e) = self.editing.as_ref().filter(|e| e.at.is_none()) {
            tree = tree.push(self.input(e));
        }
        for (i, r) in self.rows.iter().enumerate() {
            if let Some(e) = self
                .editing
                .as_ref()
                .filter(|e| e.at == Some(i) && e.replaces)
            {
                tree = tree.push(self.input(e));
                continue;
            }
            let selected = self.selected == Some(i);
            let active = self.active.as_deref() == Some(r.path.as_str());
            let text = match r.kind {
                Kind::Folder { open: true } => format!("▾ {}", r.name),
                Kind::Folder { open: false } => format!("▸ {}", r.name),
                _ => r.name.clone(),
            };
            // Files sit under their folder's name, past the triangle.
            let extra = if matches!(r.kind, Kind::Folder { .. }) {
                0.0
            } else {
                SIZE_TREE_INDENT
            };
            let indent = SIZE_TREE_INDENT * r.depth as f32 + extra;
            let color = if selected || active {
                TEXT_STRONG
            } else if r.kind == Kind::Attachment {
                TEXT_MUTED
            } else {
                TEXT_SECONDARY
            };
            let item = container(
                button(typed(text, TYPE_UI_SMALL).color(color))
                    .width(Length::Fill)
                    .padding(Padding {
                        top: SPACE_6,
                        right: SPACE_10,
                        bottom: SPACE_6,
                        left: SPACE_10 + indent,
                    })
                    .on_press((self.on_select)(i))
                    .style(styles::list_button(
                        selected || (self.selected.is_none() && active),
                    )),
            );
            tree = tree.push(if selected { item.id(SELECTED) } else { item });
            if let Some(e) = self
                .editing
                .as_ref()
                .filter(|e| e.at == Some(i) && !e.replaces)
            {
                tree = tree.push(self.input(e));
            }
        }

        let mut tags = Column::new().spacing(SPACE_2);
        for (tag, uses, depth) in &self.tags {
            let name = if *depth == 0 {
                format!("#{tag}")
            } else {
                format!("/{}", tag.rsplit('/').next().unwrap_or(tag))
            };
            tags = tags.push(
                container(row![
                    typed(name, TYPE_UI_SMALL)
                        .color(TEXT_SECONDARY)
                        .width(Length::Fill),
                    typed(uses.to_string(), TYPE_UI_SMALL).color(TEXT_SUBTLE),
                ])
                .padding(Padding {
                    top: SPACE_6,
                    right: SPACE_10,
                    bottom: SPACE_6,
                    left: SPACE_10 + SPACE_14 * *depth as f32,
                }),
            );
        }

        let mut body = column![];
        if let Some(find) = &self.find {
            body = body.push(
                button(
                    row![
                        typed("Find note…", TYPE_UI_SMALL)
                            .color(TEXT_MUTED)
                            .width(Length::Fill),
                        typed("␣f", TYPE_LABEL).font(FONT_MONO).color(TEXT_SUBTLE),
                    ]
                    .height(Length::Fill)
                    .align_y(iced::Alignment::Center),
                )
                .width(Length::Fill)
                .height(SIZE_CONTROL_H)
                .padding(Padding::from([0.0, SPACE_10]))
                .on_press(find.clone())
                .style(styles::button_secondary(false)),
            );
            body = body.push(Space::new().height(SPACE_10));
        }
        let body = body.push(column![
            container(label(&self.vault).color(TEXT_SUBTLE))
                .padding(Padding::from([SPACE_6, SPACE_10])),
            tree,
            Space::new().height(SPACE_16),
            container(label("Tags").color(TEXT_SUBTLE)).padding(Padding::from([SPACE_6, SPACE_10])),
            tags,
        ]);
        container(column![
            iced::widget::scrollable(body).id(LIST).height(Length::Fill),
            container(
                typed(
                    "a new · A folder · r rename · m move · d delete",
                    TYPE_LABEL
                )
                .font(FONT_MONO)
                .color(TEXT_SUBTLE)
            )
            .padding(Padding::from([SPACE_8, SPACE_10])),
        ])
        .width(self.width)
        .height(Length::Fill)
        .padding(Padding::from([SPACE_16, SPACE_10]))
        .style(styles::side_panel)
        .into()
    }
}
