//! Reading mode's view (F-27, mockup 07): the note rendered with Inter in a
//! column of `size.read.maxw`, links clickable and reachable with `Tab`,
//! tags as labels, checkboxes that save, images shrunk to the column.

use std::collections::{HashMap, HashSet};

use iced::widget::{
    Column, Row, Space, button, checkbox, column, container, image, rich_text, row, scrollable,
    span,
};
use iced::{Alignment, Border, Element, Length, Padding};

use crate::reading::{Block, Document, Inline, Item, Value};
use crate::theme::styles;
use crate::theme::tokens::*;
use crate::widgets::{label, typed};

/// The id of the reading view's scrollable, to scroll it from the keyboard.
pub const SCROLL: &str = "reading";

/// A decoded image, or why there is none.
#[derive(Debug, Clone)]
pub enum Picture {
    Loading,
    Ready(image::Handle),
    Missing,
}

/// What the view needs besides the blocks.
pub struct View<'a, Message> {
    pub doc: &'a Document,
    /// Link targets that point to no note (shown dimmed, F-27).
    pub missing: &'a HashSet<String>,
    /// The link `Tab` has selected.
    pub focused: Option<usize>,
    pub pictures: &'a HashMap<String, Picture>,
    pub properties_open: bool,
    pub on_link: fn(usize) -> Message,
    pub on_toggle: fn(usize) -> Message,
    pub on_properties: Message,
    pub on_scroll: fn(f32) -> Message,
}

fn link_span<'a>(
    text: String,
    index: usize,
    target: &str,
    missing: &HashSet<String>,
    focused: Option<usize>,
) -> iced::widget::text::Span<'a, usize, iced::Font> {
    let gone = missing.contains(target);
    let mut s = span(text)
        .font(FONT_UI)
        .color(if gone { READ_LINK_MISSING } else { READ_LINK })
        .underline(gone)
        .link(index);
    if focused == Some(index) {
        s = s.border(Border {
            color: READ_LINK_FOCUS,
            width: SIZE_FOCUS_OUTLINE,
            radius: SIZE_FOCUS_OUTLINE.into(),
        });
    }
    s
}

fn spans<'a>(
    inlines: &[Inline],
    missing: &HashSet<String>,
    focused: Option<usize>,
    base: iced::Color,
) -> Vec<iced::widget::text::Span<'a, usize, iced::Font>> {
    inlines
        .iter()
        .map(|i| match i {
            Inline::Text {
                text,
                bold,
                italic,
                strike,
            } => span(text.clone())
                .font(if *bold {
                    FONT_UI_BOLD
                } else if *italic {
                    FONT_UI_ITALIC
                } else {
                    FONT_UI
                })
                .color(base)
                .strikethrough(*strike),
            Inline::Code(t) => span(t.clone())
                .font(FONT_MONO)
                .color(TEXT_PRIMARY)
                .background(BG_CODE_INLINE)
                .padding(Padding::from([0.0, SPACE_4])),
            Inline::Link {
                text,
                target,
                index,
                ..
            } => link_span(text.clone(), *index, target, missing, focused),
            Inline::Tag(t) => span(format!("#{t}"))
                .font(FONT_UI)
                .color(READ_TAG_TEXT)
                .background(READ_TAG_BG)
                .padding(Padding::from([0.0, SPACE_6])),
            Inline::Math(t) => span(t.clone()).font(FONT_MONO).color(TEXT_SECONDARY),
        })
        .collect()
}

impl<'a, Message: Clone + 'a> View<'a, Message> {
    fn text(&self, inlines: &[Inline], ty: Type, base: iced::Color) -> Element<'a, Message> {
        let on_link = self.on_link;
        rich_text(spans(inlines, self.missing, self.focused, base))
            .size(ty.size)
            .line_height(iced::widget::text::LineHeight::Absolute(
                ty.line_height.into(),
            ))
            .on_link_click(on_link)
            .into()
    }

    fn item(&self, it: &Item) -> Element<'a, Message> {
        let indent = Space::new().width(SPACE_24 * it.depth as f32);
        if let Some((done, line)) = it.task {
            let on_toggle = self.on_toggle;
            let text = if done {
                // Done tasks read muted and struck through (mockup 07).
                let struck: Vec<Inline> = it
                    .inlines
                    .iter()
                    .cloned()
                    .map(|i| match i {
                        Inline::Text {
                            text, bold, italic, ..
                        } => Inline::Text {
                            text,
                            bold,
                            italic,
                            strike: true,
                        },
                        other => other,
                    })
                    .collect();
                self.text(&struck, TYPE_READ_BODY, TEXT_SUBTLE)
            } else {
                self.text(&it.inlines, TYPE_READ_BODY, TEXT_PRIMARY)
            };
            return row![
                indent,
                checkbox(done)
                    .on_toggle(move |_| on_toggle(line))
                    .style(styles::task_box),
                text
            ]
            .spacing(SPACE_10)
            .align_y(Alignment::Center)
            .into();
        }
        let marker = it
            .ordered
            .map_or_else(|| "•".to_owned(), |n| format!("{n}."));
        row![
            indent,
            typed(marker, TYPE_READ_BODY).color(TEXT_SECONDARY),
            self.text(&it.inlines, TYPE_READ_BODY, TEXT_PRIMARY)
        ]
        .spacing(SPACE_10)
        .into()
    }

    fn block(&self, b: &Block) -> Element<'a, Message> {
        match b {
            Block::Properties(props) => {
                let arrow = if self.properties_open { "▾ " } else { "▸ " };
                let mut col = column![
                    button(label(&format!("{arrow}Properties")).color(TEXT_MUTED))
                        .padding(0)
                        .on_press(self.on_properties.clone())
                        .style(styles::statusline_button(false))
                ]
                .spacing(SPACE_10);
                if self.properties_open {
                    let mut rows = Column::new().spacing(SPACE_6);
                    for (key, values) in props {
                        let mut vals = Row::new().spacing(SPACE_6);
                        for v in values {
                            vals = vals.push(match v {
                                Value::Text(t) => Element::from(
                                    typed(t.clone(), TYPE_UI_SMALL).color(TEXT_PRIMARY),
                                ),
                                Value::Tag(t) => {
                                    container(typed(t.clone(), TYPE_UI_SMALL).color(READ_TAG_TEXT))
                                        .padding(Padding::from([0.0, SPACE_8]))
                                        .style(styles::tag_label)
                                        .into()
                                }
                                Value::Link { target, index } => {
                                    let on_link = self.on_link;
                                    let rich: iced::widget::text::Rich<'a, usize, Message> =
                                        rich_text(vec![link_span(
                                            target.clone(),
                                            *index,
                                            target,
                                            self.missing,
                                            self.focused,
                                        )])
                                        .size(TYPE_UI_SMALL.size)
                                        .on_link_click(on_link);
                                    rich.into()
                                }
                            });
                        }
                        rows = rows.push(row![
                            typed(key.clone(), TYPE_UI_SMALL)
                                .color(TEXT_MUTED)
                                .width(SIZE_PROPERTY_KEY_W),
                            vals
                        ]);
                    }
                    col = col.push(rows);
                }
                container(col)
                    .width(Length::Fill)
                    .padding(Padding::from([SPACE_10, SPACE_14]))
                    .style(styles::properties_box)
                    .into()
            }
            Block::Heading { level, inlines, .. } => {
                let ty = match level {
                    1 => TYPE_READ_H1,
                    2 => TYPE_READ_H2,
                    _ => TYPE_READ_BODY,
                };
                let bold: Vec<Inline> = inlines
                    .iter()
                    .cloned()
                    .map(|i| match i {
                        Inline::Text {
                            text,
                            italic,
                            strike,
                            ..
                        } => Inline::Text {
                            text,
                            bold: true,
                            italic,
                            strike,
                        },
                        other => other,
                    })
                    .collect();
                self.text(&bold, ty, TEXT_STRONG)
            }
            Block::Paragraph { inlines, .. } => self.text(inlines, TYPE_READ_BODY, TEXT_PRIMARY),
            Block::List { items, .. } => {
                let mut col = Column::new().spacing(SPACE_6);
                for it in items {
                    col = col.push(self.item(it));
                }
                col.into()
            }
            Block::Quote { title, body, .. } => {
                let mut col = Column::new().spacing(SPACE_4);
                if let Some(t) = title {
                    col = col.push(
                        typed(t.clone(), TYPE_READ_BODY)
                            .font(FONT_UI_BOLD)
                            .color(TEXT_STRONG),
                    );
                }
                for p in body {
                    col = col.push(self.text(p, TYPE_READ_BODY, TEXT_PRIMARY));
                }
                container(col)
                    .width(Length::Fill)
                    .padding(Padding::from([SPACE_6, SPACE_16]))
                    .style(styles::quote)
                    .into()
            }
            Block::Rule { .. } => container(Space::new().width(Length::Fill).height(SIZE_BORDER))
                .style(styles::rule)
                .into(),
            Block::Table { header, rows, .. } => {
                let cell = |inlines: &[Inline], head: bool| -> Element<'a, Message> {
                    let base = if head { TEXT_STRONG } else { TEXT_PRIMARY };
                    let text: Vec<Inline> = inlines
                        .iter()
                        .cloned()
                        .map(|i| match i {
                            Inline::Text {
                                text,
                                italic,
                                strike,
                                ..
                            } if head => Inline::Text {
                                text,
                                bold: true,
                                italic,
                                strike,
                            },
                            other => other,
                        })
                        .collect();
                    container(self.text(&text, TYPE_UI, base))
                        .width(Length::FillPortion(1))
                        .padding(Padding::from([SPACE_8, SPACE_10]))
                        .into()
                };
                let mut col = Column::new();
                col = col.push(Row::with_children(header.iter().map(|c| cell(c, true))));
                col = col.push(
                    container(Space::new().width(Length::Fill).height(SIZE_BORDER))
                        .style(styles::rule_control),
                );
                for r in rows {
                    col = col.push(Row::with_children(r.iter().map(|c| cell(c, false))));
                    col = col.push(
                        container(Space::new().width(Length::Fill).height(SIZE_BORDER))
                            .style(styles::rule),
                    );
                }
                col.into()
            }
            Block::Code { text, .. } => {
                container(typed(text.clone(), TYPE_MONO_UI).color(TEXT_PRIMARY))
                    .width(Length::Fill)
                    .padding(Padding::from([SPACE_12, SPACE_14]))
                    .style(styles::inset)
                    .into()
            }
            Block::Image { target, .. } => match self.pictures.get(target) {
                Some(Picture::Ready(handle)) => image(handle.clone()).width(Length::Fill).into(),
                other => {
                    let note = if matches!(other, Some(Picture::Missing)) {
                        format!("{target} · not found")
                    } else {
                        target.clone()
                    };
                    container(typed(note, TYPE_UI_SMALL).color(TEXT_MUTED))
                        .width(Length::Fill)
                        .height(SIZE_CONTROL_H_LARGE + SIZE_CONTROL_H_LARGE)
                        .center(Length::Fill)
                        .style(styles::frame)
                        .into()
                }
            },
            Block::Embed { target, index, .. } => {
                let title = target.split(['#', '|']).next().unwrap_or(target).to_owned();
                button(
                    row![
                        crate::widgets::icon(
                            crate::widgets::banner::Icon::Note,
                            SIZE_ICON,
                            TEXT_MUTED
                        ),
                        column![
                            typed(title, TYPE_UI)
                                .font(FONT_UI_SEMIBOLD)
                                .color(TEXT_STRONG),
                            typed(
                                "Embedded note · full transclusion in Phase 6",
                                TYPE_UI_SMALL
                            )
                            .color(TEXT_MUTED),
                        ]
                        .spacing(SPACE_2),
                    ]
                    .spacing(SPACE_12)
                    .align_y(Alignment::Center),
                )
                .width(Length::Fill)
                .padding(Padding::from([SPACE_12, SPACE_16]))
                .on_press((self.on_link)(*index))
                .style(styles::backlink_card(true, self.focused == Some(*index)))
                .into()
            }
        }
    }

    /// The whole note, scrollable.
    pub fn view(self) -> Element<'a, Message> {
        let mut col = Column::new().spacing(SPACE_16).max_width(SIZE_READ_MAXW);
        for b in &self.doc.blocks {
            col = col.push(self.block(b));
        }
        let on_scroll = self.on_scroll;
        scrollable(
            container(col)
                .width(Length::Fill)
                .align_x(Alignment::Center)
                .padding(Padding {
                    top: SPACE_32 + SPACE_4,
                    right: PAD_EDITOR_SIDE,
                    bottom: SPACE_48,
                    left: PAD_EDITOR_SIDE,
                }),
        )
        .id(SCROLL)
        .on_scroll(move |v| on_scroll(v.relative_offset().y))
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    }
}
