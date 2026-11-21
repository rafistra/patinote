//! The list of Syncthing conflicts (F-11, mockup 15).

use iced::widget::{Column, Space, button, column, container, row};
use iced::{Alignment, Element, Length, Padding};

use crate::screens::switcher::hints;
use crate::theme::styles;
use crate::theme::tokens::*;
use crate::widgets::banner::Icon;
use crate::widgets::{icon, typed};

/// What a conflict copy's name says: `X.sync-conflict-20261119-162104-ABCDEFG.md`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CopyInfo {
    pub device: String,
    /// `19 Nov 16:21`.
    pub when: String,
}

/// Reads the device and time from a Syncthing conflict file name.
pub fn copy_info(path: &str) -> Option<CopyInfo> {
    let file = path.rsplit('/').next()?;
    let rest = &file[file.find(".sync-conflict-")? + ".sync-conflict-".len()..];
    let mut parts = rest.splitn(3, '-');
    let date = parts.next()?;
    let time = parts.next()?;
    let device = parts.next()?.split('.').next()?.to_owned();
    if date.len() != 8 || time.len() < 4 {
        return None;
    }
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let month: usize = date.get(4..6)?.parse().ok()?;
    let day: u32 = date.get(6..8)?.parse().ok()?;
    let month = MONTHS.get(month.checked_sub(1)?)?;
    Some(CopyInfo {
        device,
        when: format!("{day} {month} {}:{}", time.get(0..2)?, time.get(2..4)?),
    })
}

/// One conflict in the list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub title: String,
    pub folder: String,
    pub copy: String,
}

/// The list box, `size.switcher.w` wide.
pub fn view<'a, Message: Clone + 'a>(
    items: &[Item],
    selected: usize,
    on_pick: fn(usize) -> Message,
) -> Element<'a, Message> {
    let head = row![
        icon(Icon::Warning, SIZE_ICON, STATUS_WARNING),
        typed("Sync conflicts", TYPE_UI)
            .font(FONT_UI_SEMIBOLD)
            .color(TEXT_STRONG),
        Space::new().width(Length::Fill),
        typed(items.len().to_string(), TYPE_UI_SMALL).color(TEXT_SUBTLE),
    ]
    .spacing(SPACE_10)
    .align_y(Alignment::Center);
    let mut list = Column::new().spacing(SPACE_2);
    if items.is_empty() {
        list = list.push(
            container(typed("No sync conflicts", TYPE_UI).color(TEXT_MUTED))
                .padding(Padding::from([SPACE_10, SPACE_12])),
        );
    }
    for (i, it) in items.iter().enumerate() {
        let sel = i == selected;
        let info = copy_info(&it.copy).map_or_else(
            || it.copy.clone(),
            |c| format!("copy from device {} · {}", c.device, c.when),
        );
        let line = row![
            column![
                row![
                    typed(it.title.clone(), TYPE_UI)
                        .font(if sel { FONT_UI_MEDIUM } else { FONT_UI })
                        .color(if sel { TEXT_STRONG } else { TEXT_PRIMARY }),
                    typed(it.folder.clone(), TYPE_UI_SMALL).color(if sel {
                        TEXT_SECONDARY
                    } else {
                        TEXT_MUTED
                    }),
                ]
                .spacing(SPACE_4)
                .align_y(Alignment::Center),
                typed(info, TYPE_UI_SMALL).color(if sel { TEXT_SECONDARY } else { TEXT_MUTED }),
            ]
            .width(Length::Fill),
            typed("2 versions", TYPE_UI_SMALL).color(STATUS_WARNING),
        ]
        .align_y(Alignment::Center);
        list = list.push(
            button(line)
                .width(Length::Fill)
                .padding(Padding::from([SPACE_8, SPACE_12]))
                .on_press(on_pick(i))
                .style(styles::list_button(sel)),
        );
    }
    let rule =
        || container(Space::new().width(Length::Fill).height(SIZE_BORDER)).style(styles::rule);
    container(column![
        container(head).padding(Padding::from([SPACE_14, SPACE_16])),
        rule(),
        container(list).padding(SPACE_6),
        rule(),
        container(
            typed(
                "Syncthing kept both versions when the same note changed on two devices. Open one to compare and pick a version.",
                TYPE_UI_SMALL
            )
            .color(TEXT_MUTED)
        )
        .padding(Padding::from([SPACE_10, SPACE_16])),
        rule(),
        container(hints(vec![
            ("Ctrl+n/p", "move".to_owned()),
            ("Enter", "compare".to_owned()),
            ("Esc", "close".to_owned()),
        ]))
        .padding(Padding::from([SPACE_10, SPACE_16])),
    ])
    .width(SIZE_SWITCHER_W)
    .style(styles::overlay(RADIUS_DIALOG))
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copy_names_give_device_and_time() {
        assert_eq!(
            copy_info("Rapat/Rapat 19 Nov.sync-conflict-20261119-162104-ABCDEFG.md"),
            Some(CopyInfo {
                device: "ABCDEFG".into(),
                when: "19 Nov 16:21".into()
            })
        );
        assert_eq!(copy_info("Rapat/biasa.md"), None);
    }
}
