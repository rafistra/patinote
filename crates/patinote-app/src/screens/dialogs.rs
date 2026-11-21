//! Dialogs for managing notes (F-10, mockups 09 and 14): new note, rename
//! with the links it updates, delete a note or folder, and move to a folder.

use iced::widget::{
    Column, Row, Space, button, column, container, rich_text, row, span, text_input,
};
use iced::{Alignment, Element, Length, Padding};

use crate::screens::switcher::hints;
use crate::theme::styles::{self, Field};
use crate::theme::tokens::*;
use crate::widgets::banner::Icon;
use crate::widgets::{icon, label, typed};

/// The id of the input in a dialog.
pub const INPUT: &str = "dialog-input";

fn title<'a, Message: 'a>(text: String) -> Element<'a, Message> {
    typed(text, TYPE_DIALOG_TITLE)
        .font(FONT_UI_SEMIBOLD)
        .color(TEXT_STRONG)
        .into()
}

/// A button label with its key, like `Cancel Esc`.
fn keyed<'a, Message: 'a>(text: &str, key: &str, primary: bool) -> Element<'a, Message> {
    let rich: iced::widget::text::Rich<'a, (), Message> = rich_text![
        span(text.to_owned()).font(if primary { FONT_UI_SEMIBOLD } else { FONT_UI }),
        span(" ").font(FONT_UI),
        span(key.to_owned()).font(FONT_MONO).color(if primary {
            TEXT_ON_ACCENT
        } else {
            TEXT_SUBTLE
        }),
    ]
    .size(TYPE_UI.size);
    container(rich).center_y(Length::Fill).into()
}

fn buttons<'a, Message: Clone + 'a>(
    cancel: Message,
    confirm: (&str, Message, bool),
) -> Element<'a, Message> {
    let (label, on_confirm, trash) = confirm;
    let main = button(keyed(label, "Enter", true))
        .height(SIZE_CONTROL_H_LARGE)
        .padding(Padding::from([0.0, SPACE_14]))
        .on_press(on_confirm);
    row![
        Space::new().width(Length::Fill),
        button(keyed("Cancel", "Esc", false))
            .height(SIZE_CONTROL_H_LARGE)
            .padding(Padding::from([0.0, SPACE_14]))
            .on_press(cancel)
            .style(styles::button_secondary(false)),
        if trash {
            main.style(styles::button_trash)
        } else {
            main.style(styles::button_primary(false))
        },
    ]
    .spacing(SPACE_10)
    .into()
}

fn dialog<'a, Message: 'a>(
    body: impl Into<Element<'a, Message>>,
    width: f32,
) -> Element<'a, Message> {
    container(body)
        .width(width)
        .padding(PAD_DIALOG)
        .style(styles::overlay(RADIUS_DIALOG))
        .into()
}

/// A path in mono with its own background, inside body text.
fn chip<'a, Message: 'a>(text: String) -> Element<'a, Message> {
    container(typed(text, TYPE_MONO_UI).color(TEXT_PRIMARY))
        .padding(Padding::from([0.0, SPACE_6]))
        .style(styles::path_box)
        .into()
}

fn warning_box<'a, Message: 'a>(head: String, body: &str) -> Element<'a, Message> {
    container(
        column![
            typed(head, TYPE_UI)
                .font(FONT_UI_SEMIBOLD)
                .color(BANNER_WARNING_TEXT),
            typed(body.to_owned(), TYPE_UI_SMALL).color(BANNER_WARNING_MUTED),
        ]
        .spacing(SPACE_4),
    )
    .width(Length::Fill)
    .padding(Padding::from([SPACE_12, SPACE_14]))
    .style(styles::warning_box)
    .into()
}

/// `<leader>n` (mockup 14, left): name, warning or reason, folder, keys.
pub fn new_note<'a, Message: Clone + 'a>(
    value: &str,
    problem: Option<&(Field, String)>,
    folder: &str,
    on_input: fn(String) -> Message,
    on_submit: Message,
) -> Element<'a, Message> {
    let state = problem.map_or(Field::Normal, |(f, _)| *f);
    let mut body = column![
        container(
            column![
                label("New note").color(TEXT_SUBTLE),
                text_input("", value)
                    .id(INPUT)
                    .on_input(on_input)
                    .on_submit(on_submit)
                    .font(TYPE_EDITOR.font)
                    .size(TYPE_EDITOR.size)
                    .padding(Padding::from([SPACE_8, SPACE_12]))
                    .style(styles::field(state)),
            ]
            .spacing(SPACE_8)
        )
        .padding(Padding::from([SPACE_14, SPACE_16])),
    ];
    if let Some((f, reason)) = problem {
        let (glyph, color) = if *f == Field::Rejected {
            (Icon::Error, STATUS_ERROR)
        } else {
            (Icon::Warning, STATUS_WARNING)
        };
        body = body.push(
            container(
                row![
                    icon(glyph, SIZE_ICON, color),
                    typed(reason.clone(), TYPE_UI_SMALL).color(BANNER_WARNING_TEXT)
                ]
                .spacing(SPACE_10)
                .align_y(Alignment::Center),
            )
            .width(Length::Fill)
            .padding(Padding::from([SPACE_10, SPACE_16]))
            .style(styles::banner(styles::BannerKind::Warning)),
        );
    }
    let place = if folder.is_empty() {
        "/ (vault root)".to_owned()
    } else {
        format!("{folder}/")
    };
    body = body.push(
        container(row![
            typed(format!("in {place}"), TYPE_MONO_UI).color(TEXT_MUTED),
            Space::new().width(SPACE_16),
            hints(vec![
                ("Enter", "create".to_owned()),
                ("Esc", "cancel".to_owned())
            ]),
        ])
        .padding(Padding::from([SPACE_10, SPACE_16])),
    );
    container(body)
        .width(SIZE_DIALOG_W_MIN)
        .style(styles::overlay(RADIUS_DIALOG))
        .into()
}

/// One link that a rename changes: the source note, old and new text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkUpdate {
    pub path: String,
    pub old: String,
    pub new: String,
}

/// Rename (mockup 09, left).
#[allow(clippy::too_many_arguments)]
pub fn rename<'a, Message: Clone + 'a>(
    path: &str,
    value: &str,
    problem: Option<&(Field, String)>,
    updates: &[LinkUpdate],
    notes: usize,
    on_input: fn(String) -> Message,
    on_submit: Message,
    on_cancel: Message,
) -> Element<'a, Message> {
    let state = problem.map_or(Field::Normal, |(f, _)| *f);
    let mut body = column![
        title("Rename note".into()),
        typed(path.to_owned(), TYPE_UI).color(TEXT_MUTED),
        Space::new().height(SPACE_12),
        label("New name").color(TEXT_SUBTLE),
        text_input("", value)
            .id(INPUT)
            .on_input(on_input)
            .on_submit(on_submit.clone())
            .font(TYPE_EDITOR.font)
            .size(TYPE_EDITOR.size)
            .padding(Padding::from([SPACE_10, SPACE_12]))
            .style(styles::field(state)),
    ]
    .spacing(SPACE_4);
    if let Some((f, reason)) = problem {
        let color = if *f == Field::Rejected {
            STATUS_ERROR
        } else {
            STATUS_WARNING
        };
        body = body.push(typed(reason.clone(), TYPE_UI_SMALL).color(color));
    }
    if !updates.is_empty() {
        let n = updates.len();
        let head = format!(
            "{n} link{} in {notes} note{} will be updated",
            if n == 1 { "" } else { "s" },
            if notes == 1 { "" } else { "s" }
        );
        let mut list = Column::new().spacing(SPACE_2);
        for u in updates {
            list = list.push(
                typed(format!("{} · {} → {}", u.path, u.old, u.new), TYPE_MONO_UI)
                    .color(TEXT_SECONDARY),
            );
        }
        body = body.push(Space::new().height(SPACE_12));
        body = body.push(
            container(
                column![
                    typed(head, TYPE_UI).font(FONT_UI_SEMIBOLD).color(TEXT_STRONG),
                    list,
                    typed("Only the link text changes. Everything else in those files stays as it is.", TYPE_UI_SMALL)
                        .color(TEXT_MUTED),
                ]
                .spacing(SPACE_8),
            )
            .width(Length::Fill)
            .padding(Padding::from([SPACE_12, SPACE_14]))
            .style(styles::inset),
        );
    }
    let action = if updates.is_empty() {
        "Rename"
    } else {
        "Rename and update links"
    };
    body = body.push(Space::new().height(SPACE_16));
    body = body.push(buttons(on_cancel, (action, on_submit, false)));
    dialog(body, SIZE_DIALOG_W_MAX)
}

/// Delete a note (mockup 09, right).
pub fn delete_note<'a, Message: Clone + 'a>(
    name: &str,
    path: &str,
    linking_notes: usize,
    trash: &str,
    on_confirm: Message,
    on_cancel: Message,
) -> Element<'a, Message> {
    let mut body = column![
        title(format!("Delete \"{name}\"?")),
        typed(path.to_owned(), TYPE_UI).color(TEXT_MUTED),
        Space::new().height(SPACE_12),
    ]
    .spacing(SPACE_4);
    if linking_notes > 0 {
        let head = if linking_notes == 1 {
            "1 note links here".to_owned()
        } else {
            format!("{linking_notes} notes link here")
        };
        body = body.push(warning_box(
            head,
            "Those links will point to a note that no longer exists.",
        ));
        body = body.push(Space::new().height(SPACE_8));
    }
    body = body.push(
        Row::new()
            .push(typed("The note moves to ", TYPE_UI).color(TEXT_SECONDARY))
            .push(chip(trash.to_owned()))
            .push(typed(". Nothing is deleted permanently.", TYPE_UI).color(TEXT_SECONDARY))
            .wrap(),
    );
    body = body.push(Space::new().height(SPACE_16));
    body = body.push(buttons(on_cancel, ("Move to trash", on_confirm, true)));
    dialog(body, SIZE_DIALOG_W_MIN)
}

/// Delete a folder (mockup 14, bottom right).
#[allow(clippy::too_many_arguments)]
pub fn delete_folder<'a, Message: Clone + 'a>(
    name: &str,
    notes: usize,
    attachments: usize,
    outside_links: usize,
    trash: &str,
    on_confirm: Message,
    on_cancel: Message,
) -> Element<'a, Message> {
    let plural = |n: usize, one: &str, many: &str| {
        if n == 1 {
            format!("1 {one}")
        } else {
            format!("{n} {many}")
        }
    };
    let mut body = column![
        title(format!("Delete folder \"{name}\"?")),
        typed(
            format!(
                "{}, {}",
                plural(notes, "note", "notes"),
                plural(attachments, "attachment", "attachments")
            ),
            TYPE_UI
        )
        .color(TEXT_MUTED),
        Space::new().height(SPACE_12),
    ]
    .spacing(SPACE_4);
    if outside_links > 0 {
        body = body.push(warning_box(
            format!(
                "{} outside this folder link here",
                plural(outside_links, "note", "notes")
            ),
            "Those links will point to notes that no longer exist.",
        ));
        body = body.push(Space::new().height(SPACE_8));
    }
    body = body.push(
        Row::new()
            .push(typed("Everything moves to ", TYPE_UI).color(TEXT_SECONDARY))
            .push(chip(trash.to_owned()))
            .push(typed(". Nothing is deleted permanently.", TYPE_UI).color(TEXT_SECONDARY))
            .wrap(),
    );
    body = body.push(Space::new().height(SPACE_16));
    body = body.push(buttons(on_cancel, ("Move to trash", on_confirm, true)));
    dialog(body, SIZE_DIALOG_W_MIN)
}

/// Move to a folder (mockup 14, top right).
#[allow(clippy::too_many_arguments)]
pub fn move_picker<'a, Message: Clone + 'a>(
    name: &str,
    current: &str,
    query: &str,
    folders: &[(String, Vec<usize>)],
    selected: usize,
    path_links: usize,
    on_input: fn(String) -> Message,
    on_submit: Message,
    on_pick: fn(usize) -> Message,
) -> Element<'a, Message> {
    let here = if current.is_empty() {
        "/".to_owned()
    } else {
        format!("{current}/")
    };
    let mut list = Column::new().spacing(SPACE_2);
    if folders.is_empty() {
        list = list.push(
            container(typed(format!("No folders match \"{query}\""), TYPE_UI).color(TEXT_MUTED))
                .padding(Padding::from([SPACE_8, SPACE_10])),
        );
    }
    for (i, (folder, matched)) in folders.iter().enumerate() {
        let sel = i == selected;
        let shown = if folder.is_empty() {
            "/".to_owned()
        } else {
            format!("{folder}/")
        };
        let mut parts: Vec<Element<'a, Message>> = Vec::new();
        for (k, c) in shown.chars().enumerate() {
            let hit = matched.binary_search(&k).is_ok();
            parts.push(
                typed(c.to_string(), TYPE_MONO_UI)
                    .color(if hit {
                        ACCENT
                    } else if sel {
                        TEXT_STRONG
                    } else {
                        TEXT_PRIMARY
                    })
                    .into(),
            );
        }
        list = list.push(
            button(
                row![
                    icon(
                        Icon::Folder,
                        SIZE_ICON,
                        if sel { TEXT_SECONDARY } else { TEXT_MUTED }
                    ),
                    Row::with_children(parts),
                ]
                .spacing(SPACE_10)
                .align_y(Alignment::Center),
            )
            .width(Length::Fill)
            .padding(Padding::from([SPACE_8, SPACE_10]))
            .on_press(on_pick(i))
            .style(styles::list_button(sel)),
        );
    }
    let note = if path_links == 0 {
        "Links by name stay as they are.".to_owned()
    } else if path_links == 1 {
        "1 link uses the path and will be updated. Links by name stay as they are.".to_owned()
    } else {
        format!(
            "{path_links} links use the path and will be updated. Links by name stay as they are."
        )
    };
    let rule =
        || container(Space::new().width(Length::Fill).height(SIZE_BORDER)).style(styles::rule);
    container(column![
        container(
            column![
                typed(format!("Move \"{name}\" to…"), TYPE_UI)
                    .font(FONT_UI_SEMIBOLD)
                    .color(TEXT_STRONG),
                typed(format!("Currently in {here}"), TYPE_UI_SMALL).color(TEXT_MUTED),
            ]
            .spacing(SPACE_2)
        )
        .padding(Padding::from([SPACE_14, SPACE_16])),
        rule(),
        container(
            row![
                icon(Icon::Search, SIZE_ICON, TEXT_MUTED),
                text_input("Folder", query)
                    .id(INPUT)
                    .on_input(on_input)
                    .on_submit(on_submit)
                    .font(TYPE_EDITOR.font)
                    .size(TYPE_EDITOR.size)
                    .padding(0)
                    .style(styles::switcher_input),
            ]
            .spacing(SPACE_10)
            .align_y(Alignment::Center)
        )
        .padding(Padding::from([SPACE_10, SPACE_16])),
        rule(),
        container(list).padding(SPACE_6),
        rule(),
        container(typed(note, TYPE_UI_SMALL).color(TEXT_MUTED))
            .padding(Padding::from([SPACE_10, SPACE_16])),
        rule(),
        container(hints(vec![
            ("Enter", "move".to_owned()),
            ("Esc", "cancel".to_owned())
        ]))
        .padding(Padding::from([SPACE_10, SPACE_16])),
    ])
    .width(SIZE_PICKER_W)
    .style(styles::overlay(RADIUS_DIALOG))
    .into()
}
