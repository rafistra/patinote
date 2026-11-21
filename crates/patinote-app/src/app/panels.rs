//! The side panel, buffers and navigation history (F-09): Outline and
//! Backlinks, keyboard focus between editor and panel (`Ctrl+w h/l`), the
//! jump list (`Ctrl+O`, `Ctrl+I`), the buffer limit, and restoring the last
//! session's buffers and cursors.

use iced::keyboard::{Key, Modifiers, key::Named};
use iced::widget::row;
use iced::{Element, Task};
use patinote_core::names::stem;

use super::{App, Message};
use crate::editor::highlight::LineKind;
use crate::screens::panel::{self, BacklinkItem, Focused, OutlineItem};
use crate::vim;

/// At most this many buffers stay open (F-09).
pub const MAX_BUFFERS: usize = 20;
/// At most this many jumps are remembered per session (F-09).
pub const MAX_JUMPS: usize = 100;

/// A place in the jump list: a note and a cursor.
pub type Jump = (String, (usize, usize));

/// The headings of a note, from its lines and their kinds (no parsing).
pub fn outline_of(lines: &[String], kinds: &[LineKind]) -> Vec<OutlineItem> {
    lines
        .iter()
        .zip(kinds)
        .enumerate()
        .filter(|(_, (_, k))| **k == LineKind::Text)
        .filter_map(|(i, (l, _))| {
            let hashes = l.chars().take_while(|&c| c == '#').count();
            let rest = l.get(hashes..)?;
            ((1..=6).contains(&hashes) && rest.starts_with(' ')).then(|| OutlineItem {
                level: hashes as u8,
                text: rest.trim().to_owned(),
                line: i,
            })
        })
        .collect()
}

impl App {
    /// The outline of the active note.
    fn outline(&self) -> Vec<OutlineItem> {
        self.active_document()
            .map(|d| outline_of(d.buffer.lines(), d.cached_kinds()))
            .unwrap_or_default()
    }

    /// The links into the active note.
    fn backlink_items(&self) -> Vec<BacklinkItem> {
        let Some(b) = self.workspace().and_then(|w| w.active_buffer()) else {
            return Vec::new();
        };
        b.backlinks
            .iter()
            .flatten()
            .map(|l| BacklinkItem {
                title: stem(&l.path).to_owned(),
                path: l.path.clone(),
                line: l.line,
                marker: if l.kind == "frontmatter" {
                    Some("property".into())
                } else {
                    l.subpath.as_ref().map(|s| format!("→ #{s}"))
                },
                context: l.context.clone(),
            })
            .collect()
    }

    /// `<leader>b` / `<leader>o`: opens the panel with the focus on that
    /// section, or closes it when it is open (F-28.3). A closed panel drops
    /// what it showed.
    pub(super) fn toggle_panel(&mut self, backlinks: bool) -> Task<Message> {
        // Classic shows one section at a time: a key for the other one, or
        // for an unfocused panel, brings that section up instead.
        let shown_elsewhere =
            self.classic() && (self.focus.is_none() || self.panel_outline == backlinks);
        if self.panel_open && !shown_elsewhere {
            self.panel_open = false;
            self.focus = None;
        } else {
            self.panel_open = true;
            let focus = if backlinks {
                Focused::Backlink(0)
            } else {
                Focused::Outline(self.current_heading().unwrap_or(0))
            };
            self.focus_panel(focus);
        }
        let open = self.panel_open;
        if let Some(w) = self.workspace_mut() {
            w.state.panel_open = open;
        }
        self.save_vault_state()
    }

    fn classic(&self) -> bool {
        self.layout() == patinote_core::config::Layout::Classic
    }

    /// Puts the focus in the panel; in classic, its section's tab shows.
    fn focus_panel(&mut self, focus: Focused) {
        if self.classic() {
            self.panel_outline = matches!(focus, Focused::Outline(_));
        }
        self.focus = Some(focus);
    }

    /// A click on a classic panel tab: a focus in the panel moves to the
    /// section now shown.
    pub(super) fn panel_tab(&mut self, outline: bool) -> Task<Message> {
        self.panel_outline = outline;
        if self.focus.is_some() {
            let focus = if outline {
                Focused::Outline(self.current_heading().unwrap_or(0))
            } else {
                Focused::Backlink(0)
            };
            self.focus = Some(focus);
        }
        Task::none()
    }

    fn current_heading(&self) -> Option<usize> {
        let line = self.active_document()?.buffer.cursor().line;
        panel::current_heading(&self.outline(), line)
    }

    /// Backlinks of the active note as (source path, marker), for tests.
    pub fn backlink_markers(&self) -> Vec<(String, Option<String>)> {
        self.backlink_items()
            .into_iter()
            .map(|b| (b.path, b.marker))
            .collect()
    }

    /// The titles in the bufferline, for tests.
    pub fn buffer_titles(&self) -> Vec<String> {
        self.workspace()
            .map(|w| w.buffers.iter().map(|b| stem(&b.path).to_owned()).collect())
            .unwrap_or_default()
    }

    /// Backlinks of the active note as (source path, line), for tests.
    pub fn backlink_lines(&self) -> Vec<(String, usize)> {
        self.backlink_items()
            .into_iter()
            .map(|b| (b.path, b.line))
            .collect()
    }

    /// Whether the side panel is open, for tests.
    pub fn panel_is_open(&self) -> bool {
        self.panel_open
    }

    /// Whether the panel has the keyboard focus, for tests.
    pub fn panel_focused(&self) -> bool {
        self.focus.is_some()
    }

    /// `Ctrl+w` then `h`/`l`: focus between editor and panel (F-28).
    pub(super) fn window_key(&mut self, text: Option<&str>) -> Task<Message> {
        match text {
            Some("l") if self.panel_open => {
                if self.focus.is_none() {
                    let focus = if self.classic() && !self.panel_outline {
                        Focused::Backlink(0)
                    } else {
                        Focused::Outline(self.current_heading().unwrap_or(0))
                    };
                    self.focus_panel(focus);
                }
            }
            Some("h") => self.focus = None,
            _ => {}
        }
        Task::none()
    }

    /// Keys while the panel has the focus: `j`/`k` move, `Enter` opens,
    /// `Esc` goes back to the editor. Everything else is ignored.
    pub(super) fn panel_key(
        &mut self,
        key: &Key,
        text: Option<&str>,
        modifiers: Modifiers,
    ) -> Task<Message> {
        let Some(focus) = self.focus else {
            return Task::none();
        };
        if modifiers.control() && *key == Key::Character("w".into()) {
            self.window_prefix = true;
            return Task::none();
        }
        let mut outline = self.outline().len();
        let mut links = self.backlink_items().len();
        // In classic only the section shown is walked.
        if self.classic() {
            if self.panel_outline {
                links = 0;
            } else {
                outline = 0;
            }
        }
        // One list: outline items, then backlinks.
        let total = outline + links;
        let index = match focus {
            Focused::Outline(i) => i,
            Focused::Backlink(i) => outline + i,
        };
        let to = |i: usize| {
            if i < outline {
                Focused::Outline(i)
            } else {
                Focused::Backlink(i - outline)
            }
        };
        match (key, text) {
            (Key::Named(Named::Escape), _) => self.focus = None,
            (Key::Named(Named::ArrowDown), _) | (_, Some("j")) if total > 0 => {
                self.focus = Some(to((index + 1).min(total - 1)));
            }
            (Key::Named(Named::ArrowUp), _) | (_, Some("k")) if total > 0 => {
                self.focus = Some(to(index.saturating_sub(1)));
            }
            (Key::Named(Named::Enter), _) => {
                return match focus {
                    Focused::Outline(i) => {
                        self.focus = None;
                        self.outline_jump(i);
                        Task::none()
                    }
                    Focused::Backlink(i) => {
                        self.focus = None;
                        self.open_backlink(i)
                    }
                };
            }
            _ => {}
        }
        Task::none()
    }

    /// `Enter` on an outline item: the cursor goes to the heading (F-09.4).
    pub(super) fn outline_jump(&mut self, i: usize) {
        let Some(item) = self.outline().get(i).cloned() else {
            return;
        };
        self.record_jump();
        if let Some(doc) = self.active_document_mut() {
            doc.buffer.set_cursor(vim::Pos::new(item.line, 0));
        }
    }

    /// `Enter` on a backlink: the source note opens with the cursor on the
    /// link's line (F-09.3).
    pub(super) fn open_backlink(&mut self, i: usize) -> Task<Message> {
        let Some(item) = self.backlink_items().get(i).cloned() else {
            return Task::none();
        };
        self.pending_cursor
            .insert(item.path.clone(), (item.line.saturating_sub(1), 0));
        let open = self.open_note(item.path);
        self.apply_pending_cursor();
        open
    }

    /// Puts the cursor where it was asked to go, once the note is loaded.
    pub(super) fn apply_pending_cursor(&mut self) {
        let Some(path) = self.active_document().map(|d| d.path.clone()) else {
            return;
        };
        if let Some((line, col)) = self.pending_cursor.remove(&path)
            && let Some(doc) = self.active_document_mut()
        {
            doc.buffer.set_cursor(vim::Pos::new(line, col));
        }
    }

    // --- Jump list -------------------------------------------------------------

    fn here(&self) -> Option<Jump> {
        let d = self.active_document()?;
        let c = d.buffer.cursor();
        Some((d.path.clone(), (c.line, c.col)))
    }

    /// Remembers where the cursor is before a jump or a note switch.
    pub(super) fn record_jump(&mut self) {
        if self.navigating {
            return;
        }
        let Some(here) = self.here() else { return };
        self.jumps.truncate(self.jump_at);
        if self.jumps.last() != Some(&here) {
            self.jumps.push(here);
        }
        if self.jumps.len() > MAX_JUMPS {
            self.jumps.remove(0);
        }
        self.jump_at = self.jumps.len();
    }

    /// `Ctrl+O` (`back`) and `Ctrl+I`: move through the jump list (F-09.5).
    pub(super) fn jump(&mut self, back: bool) -> Task<Message> {
        if back {
            if self.jump_at == self.jumps.len() {
                // Remember where we are, so Ctrl+I can come back.
                if let Some(here) = self.here() {
                    if self.jumps.last() != Some(&here) {
                        self.jumps.push(here);
                    }
                    self.jump_at = self.jumps.len() - 1;
                }
            }
            if self.jump_at == 0 {
                return Task::none();
            }
            self.jump_at -= 1;
        } else {
            if self.jump_at + 1 >= self.jumps.len() {
                return Task::none();
            }
            self.jump_at += 1;
        }
        let Some((path, cursor)) = self.jumps.get(self.jump_at).cloned() else {
            return Task::none();
        };
        self.navigating = true;
        self.pending_cursor.insert(path.clone(), cursor);
        let open = self.open_note(path);
        self.apply_pending_cursor();
        self.navigating = false;
        open
    }

    // --- Buffers -----------------------------------------------------------------

    /// Closes the least recently used unchanged buffers beyond the limit;
    /// a buffer with unsaved text is never closed (F-09.7).
    pub(super) fn enforce_buffer_limit(&mut self) {
        let Some(w) = self.workspace_mut() else {
            return;
        };
        while w.buffers.len() > MAX_BUFFERS {
            let active = w.active;
            let oldest = w
                .buffers
                .iter()
                .enumerate()
                .filter(|(i, b)| Some(*i) != active && !b.doc.as_ref().is_some_and(|d| d.dirty()))
                .min_by_key(|(_, b)| b.used)
                .map(|(i, _)| i);
            let Some(i) = oldest else { break };
            w.buffers.remove(i);
            if let Some(a) = w.active
                && a > i
            {
                w.active = Some(a - 1);
            }
        }
    }

    /// `:b name`: the buffer whose name contains `name`.
    pub(super) fn buffer_by_name(&self, name: &str) -> Option<usize> {
        let w = self.workspace()?;
        let name = name.to_lowercase();
        w.buffers
            .iter()
            .position(|b| stem(&b.path).to_lowercase() == name)
            .or_else(|| {
                w.buffers
                    .iter()
                    .position(|b| b.path.to_lowercase().contains(&name))
            })
    }

    // --- Views -------------------------------------------------------------------

    /// The body with the side panel at its right, when open.
    pub(super) fn with_panel<'a>(&'a self, body: Element<'a, Message>) -> Element<'a, Message> {
        if !self.panel_open
            || self.workspace().is_none()
            || self.layout() == patinote_core::config::Layout::Focus
        {
            return body;
        }
        let outline = self.outline();
        let current = self
            .active_document()
            .and_then(|d| panel::current_heading(&outline, d.buffer.cursor().line));
        let items = self.backlink_items();
        let view = panel::Panel {
            outline,
            current,
            backlinks: items,
            focused: self.focus,
            on_outline: Message::OutlineJump,
            on_backlink: Message::OpenBacklink,
            width: self.panel_width(),
            tabs: (self.layout() == patinote_core::config::Layout::Classic).then_some((
                self.panel_outline,
                Message::PanelTab(false),
                Message::PanelTab(true),
            )),
        }
        .view();
        row![body, self.edge(super::layout::Edge::Panel), view].into()
    }

    /// The backlinks under the note, when the panel is closed (mockup 01).
    pub(super) fn inline_backlinks(&self) -> Option<Element<'_, Message>> {
        if self.panel_open {
            return None;
        }
        let items = self.backlink_items();
        if items.is_empty() {
            return None;
        }
        Some(panel::inline(&items, Message::OpenBacklink))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::highlight::line_kinds;

    #[test]
    fn the_outline_lists_headings_outside_code_and_frontmatter() {
        let lines: Vec<String> = [
            "---", "# not", "---", "# A", "```", "# code", "```", "## B", "#nottag",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let outline = outline_of(&lines, &line_kinds(&lines));
        let got: Vec<_> = outline
            .iter()
            .map(|h| (h.level, h.text.as_str(), h.line))
            .collect();
        assert_eq!(got, [(1, "A", 3), (2, "B", 7)]);
    }
}
