//! Layouts and window controls (F-28): minimal, classic and focus with
//! `:layout` and `<leader>l`; the leader popup after 300 ms; text size with
//! `Ctrl+=`, `Ctrl+-`, `Ctrl+0`; and panel widths dragged with the mouse.

use std::time::Duration;

use iced::keyboard::Key;
use iced::widget::{container, mouse_area, row, stack};
use iced::{Element, Length, Padding, Task};
use patinote_core::config::Layout;

use super::{App, Message};
use crate::screens::leader;
use crate::theme::styles::{self, Level};
use crate::theme::tokens::*;

/// How long Space waits for the next key before the popup shows (F-28).
pub const LEADER_DELAY: Duration = Duration::from_millis(300);

/// A panel edge that can be dragged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Edge {
    Explorer,
    Panel,
}

impl App {
    /// The layout in use.
    pub fn layout(&self) -> Layout {
        self.workspace().map_or(Layout::Minimal, |w| w.layout)
    }

    /// Switches layout; buffers and cursors stay as they are (F-28.10).
    pub(super) fn set_layout(&mut self, layout: Layout) -> Task<Message> {
        let Some(w) = self.workspace_mut() else {
            return Task::none();
        };
        w.layout = layout;
        w.state.layout = Some(layout);
        match layout {
            // Classic shows explorer and side panel (F-28).
            Layout::Classic => {
                self.explorer_open = true;
                self.panel_open = true;
                self.rebuild_tree();
            }
            Layout::Focus | Layout::Minimal => {
                self.release_explorer();
                self.explorer_open = false;
                self.panel_open = false;
                self.explorer_focus = false;
                self.focus = None;
            }
        }
        self.remember_panels();
        let tags = if layout == Layout::Classic {
            self.load_tags()
        } else {
            Task::none()
        };
        Task::batch([tags, self.save_vault_state()])
    }

    /// `:layout name`.
    pub(super) fn layout_command(&mut self, name: &str) -> Task<Message> {
        match Layout::from_name(name.trim()) {
            Some(l) => self.set_layout(l),
            None if name.trim().is_empty() => {
                let current = self.layout().name();
                self.say(Level::Info, format!("layout: {current}"));
                Task::none()
            }
            None => {
                self.say(Level::Error, format!("Unknown layout: {name}"));
                Task::none()
            }
        }
    }

    /// Remembers which panels are open (F-28.11).
    pub(super) fn remember_panels(&mut self) {
        let (explorer, panel) = (self.explorer_open, self.panel_open);
        if let Some(w) = self.workspace_mut() {
            w.state.explorer_open = explorer;
            w.state.panel_open = panel;
        }
    }

    // --- Leader popup ------------------------------------------------------------

    /// Space was pressed: the popup shows if nothing follows within 300 ms.
    pub(super) fn start_leader(&mut self) -> Task<Message> {
        self.leader = true;
        self.leader_seq += 1;
        let seq = self.leader_seq;
        Task::future(async move {
            std::thread::sleep(LEADER_DELAY);
            Message::LeaderTimeout(seq)
        })
    }

    pub(super) fn leader_timeout(&mut self, seq: u64) {
        if self.leader && seq == self.leader_seq {
            self.leader_popup = true;
        }
    }

    /// The mode the status line shows, for tests (F-28.5).
    pub fn status_mode(&self) -> &'static str {
        self.status_line().mode.label()
    }

    /// Whether the leader popup shows, for tests.
    pub fn leader_popup_shown(&self) -> bool {
        self.leader_popup
    }

    // --- Text size ----------------------------------------------------------------

    /// The app's scale: the text size set with `Ctrl+=`/`Ctrl+-` (F-28).
    pub fn zoom(&self) -> f32 {
        self.zoom
    }

    /// `Ctrl+=`, `Ctrl+-`, `Ctrl+0`; `None` for other keys.
    pub(super) fn zoom_key(&mut self, key: &Key) -> Option<Task<Message>> {
        let Key::Character(c) = key else { return None };
        let z = match c.as_str() {
            "=" | "+" => (self.zoom + ZOOM_STEP).min(ZOOM_MAX),
            "-" => (self.zoom - ZOOM_STEP).max(ZOOM_MIN),
            "0" => 1.0,
            _ => return None,
        };
        // Steps of 10 %.
        self.zoom = (z / ZOOM_STEP).round() * ZOOM_STEP;
        self.app_state.zoom = Some(self.zoom);
        Some(self.save_app_state())
    }

    // --- Dragging panel edges ----------------------------------------------------------

    pub(super) fn drag_start(&mut self, edge: Edge) {
        self.dragging = Some(edge);
    }

    /// The mouse moved while an edge is dragged: the panel follows, within
    /// the widths `0003` allows.
    pub(super) fn drag_move(&mut self, x: f32) {
        let Some(edge) = self.dragging else { return };
        let window = self.window_width;
        let Some(w) = self.workspace_mut() else {
            return;
        };
        match edge {
            Edge::Explorer => {
                w.state.explorer_w = Some(x.clamp(SIZE_EXPLORER_W_MIN, SIZE_EXPLORER_W_MAX));
            }
            Edge::Panel => {
                w.state.panel_w =
                    Some((window - x).clamp(SIZE_SIDEPANEL_W_MIN, SIZE_SIDEPANEL_W_MAX));
            }
        }
    }

    /// The drag ended: the width is kept for the next session (F-28.7).
    pub(super) fn drag_end(&mut self) -> Task<Message> {
        self.dragging = None;
        self.save_vault_state()
    }

    /// The explorer's width, for tests.
    pub fn explorer_w(&self) -> f32 {
        self.explorer_width()
    }

    /// The explorer's and side panel's widths.
    pub(super) fn explorer_width(&self) -> f32 {
        self.workspace()
            .and_then(|w| w.state.explorer_w)
            .unwrap_or(SIZE_EXPLORER_W)
    }

    pub(super) fn panel_width(&self) -> f32 {
        self.workspace()
            .and_then(|w| w.state.panel_w)
            .unwrap_or(SIZE_SIDEPANEL_W)
    }

    /// The edge between a panel and the editor, as a 1 px line that can be
    /// dragged, `accent` while the mouse is on it (0003 section 7).
    pub(super) fn edge(&self, edge: Edge) -> Element<'_, Message> {
        let hover = self.hover_edge == Some(edge) || self.dragging == Some(edge);
        let width = if hover {
            SIZE_FOCUS_OUTLINE
        } else {
            SIZE_BORDER
        };
        mouse_area(
            container("")
                .width(width)
                .height(Length::Fill)
                .style(styles::divider(hover)),
        )
        .on_press(Message::DragStart(edge))
        .on_enter(Message::HoverEdge(Some(edge)))
        .on_exit(Message::HoverEdge(None))
        .interaction(iced::mouse::Interaction::ResizingHorizontally)
        .into()
    }

    /// The leader popup over the body, 64 px above the status line.
    pub(super) fn with_leader<'a>(&'a self, body: Element<'a, Message>) -> Element<'a, Message> {
        if !self.leader_popup {
            return body;
        }
        stack![
            body,
            container(leader::view())
                .width(Length::Fill)
                .height(Length::Fill)
                .align_x(iced::Alignment::Center)
                .align_y(iced::Alignment::End)
                .padding(Padding {
                    bottom: SIZE_LEADER_BOTTOM,
                    ..Padding::ZERO
                }),
        ]
        .into()
    }

    /// The small footer of focus mode (mockup 04): words and mode.
    pub(super) fn focus_footer(&self) -> Element<'_, Message> {
        let line = self.status_line();
        let mode_color = styles::mode_color(line.mode);
        let words = line
            .notes
            .iter()
            .find(|n| n.ends_with("words") || n.ends_with("word"))
            .cloned()
            .unwrap_or_default();
        container(
            row![
                crate::widgets::typed(words, TYPE_UI_SMALL).color(TEXT_SUBTLE),
                crate::widgets::typed(line.mode.label(), TYPE_UI_SMALL)
                    .font(FONT_UI_BOLD)
                    .color(mode_color),
            ]
            .spacing(SPACE_16),
        )
        .width(Length::Fill)
        .height(SIZE_BUFFERLINE_H)
        .align_x(iced::Alignment::End)
        .center_y(SIZE_BUFFERLINE_H)
        .padding(Padding::from([0.0, SPACE_20]))
        .style(styles::app)
        .into()
    }
}
