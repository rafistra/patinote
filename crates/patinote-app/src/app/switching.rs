//! The quick switcher in the app (F-07): opening it, typing, moving through
//! results, opening and creating notes.

use std::collections::HashMap;

use iced::keyboard::{Key, Modifiers, key::Named};
use iced::widget::{container, stack};
use iced::{Element, Length, Padding, Task};

use super::{App, Message};
use crate::screens::switcher::{self, View};
use crate::switcher::{Candidate, Results};
use crate::theme::styles;
use crate::theme::tokens::*;

/// An open quick switcher.
#[derive(Debug, Clone, Default)]
pub struct Switcher {
    pub query: String,
    pub results: Results,
    pub selected: usize,
}

impl App {
    /// Recently opened notes, by path, 0 being the newest.
    fn recent_ranks(&self) -> HashMap<String, usize> {
        self.workspace()
            .map(|w| {
                w.state
                    .recent
                    .iter()
                    .enumerate()
                    .map(|(i, r)| (r.path.clone(), i))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Loads the notes the switcher finds, and the attachments `![[`
    /// suggests, from the index.
    pub(super) fn load_candidates(&self) -> Task<Message> {
        let Some(w) = self.workspace() else {
            return Task::none();
        };
        Task::perform(
            w.session.query(|store| {
                let aliases = store.all_aliases().unwrap_or_default();
                let mut notes = Vec::new();
                let mut attachments = Vec::new();
                for n in store.all_notes().unwrap_or_default() {
                    if n.excluded {
                        continue;
                    }
                    match n.kind.as_str() {
                        "note" => {
                            let a = aliases.get(&n.id).cloned().unwrap_or_default();
                            notes.push(Candidate::new(&n.path, a));
                        }
                        _ => attachments.push(Candidate::new(&n.path, Vec::new())),
                    }
                }
                (notes, attachments)
            }),
            |c| {
                let (notes, attachments) = c.unwrap_or_default();
                Message::Candidates(notes, attachments)
            },
        )
    }

    /// Opens the switcher (`<leader>f`, `Ctrl+Shift+O`).
    pub(super) fn open_switcher(&mut self) -> Task<Message> {
        if self.workspace().is_none() {
            return Task::none();
        }
        self.switcher = Some(Switcher::default());
        self.refresh_switcher();
        iced::widget::operation::focus(switcher::INPUT)
    }

    /// Whether the switcher is open, for tests.
    pub fn switcher_open(&self) -> bool {
        self.switcher.is_some()
    }

    /// The titles listed in the switcher, for tests.
    pub fn switcher_titles(&self) -> Vec<String> {
        let (Some(s), Some(w)) = (&self.switcher, self.workspace()) else {
            return Vec::new();
        };
        s.results
            .hits
            .iter()
            .map(|h| w.candidates[h.candidate].title.clone())
            .collect()
    }

    pub(super) fn refresh_switcher(&mut self) {
        let recent = self.recent_ranks();
        let Some(query) = self.switcher.as_ref().map(|s| s.query.clone()) else {
            return;
        };
        let Some(w) = self.workspace_mut() else {
            return;
        };
        let results = w.finder.find(&w.candidates, &query, &recent);
        if let Some(s) = &mut self.switcher {
            s.results = results;
            s.selected = 0;
        }
    }

    pub(super) fn switcher_input(&mut self, query: String) {
        if let Some(s) = &mut self.switcher {
            s.query = query;
        }
        self.refresh_switcher();
    }

    /// `Enter` opens the selected note; `Shift+Enter` creates one named as
    /// typed (F-07.5).
    pub(super) fn switcher_submit(&mut self) -> Task<Message> {
        if self.modifiers.shift() {
            let name = self
                .switcher
                .as_ref()
                .map(|s| s.query.trim().to_owned())
                .unwrap_or_default();
            if name.is_empty() {
                return Task::none();
            }
            self.switcher = None;
            return self.create_note(&name);
        }
        let selected = self.switcher.as_ref().map_or(0, |s| s.selected);
        self.switcher_pick(selected)
    }

    pub(super) fn switcher_pick(&mut self, i: usize) -> Task<Message> {
        let path = self.switcher.as_ref().and_then(|s| {
            let hit = s.results.hits.get(i)?;
            let w = self.workspace()?;
            Some(w.candidates[hit.candidate].path.clone())
        });
        self.switcher = None;
        match path {
            Some(path) => self.open_note(path),
            None => Task::none(),
        }
    }

    /// Keys while the switcher is open; `true` when handled.
    pub(super) fn switcher_key(
        &mut self,
        key: &Key,
        modifiers: Modifiers,
    ) -> Option<Task<Message>> {
        let s = self.switcher.as_mut()?;
        let n = s.results.hits.len();
        let ctrl = |c: &str| modifiers.control() && *key == Key::Character(c.into());
        if *key == Key::Named(Named::Escape) {
            self.switcher = None;
            return Some(Task::none());
        }
        if ctrl("n") || ctrl("j") || *key == Key::Named(Named::ArrowDown) {
            if n > 0 {
                s.selected = (s.selected + 1) % n;
            }
            return Some(Task::none());
        }
        if ctrl("p") || ctrl("k") || *key == Key::Named(Named::ArrowUp) {
            if n > 0 {
                s.selected = (s.selected + n - 1) % n;
            }
            return Some(Task::none());
        }
        Some(Task::none())
    }

    /// Creates a note named `name` at the vault root (the new-notes folder
    /// of F-10 comes in step 2.6) and opens it in insert mode. It is
    /// written once it has text. An existing note of that name opens instead.
    pub(super) fn create_note(&mut self, name: &str) -> Task<Message> {
        let name = name.strip_suffix(".md").unwrap_or(name);
        let path = format!("{name}.md");
        let exists = self.workspace().and_then(|w| {
            w.candidates
                .iter()
                .find(|c| c.path.eq_ignore_ascii_case(&path))
                .map(|c| c.path.clone())
        });
        if let Some(existing) = exists {
            return self.open_note(existing);
        }
        if let Some(w) = self.workspace_mut() {
            w.candidates.push(Candidate::new(&path, Vec::new()));
        }
        self.open_new_note(path, true)
    }

    /// The body with the switcher over it (mockup 05): the editor behind is
    /// dimmed; the box sits 72 px from the top of the window.
    pub(super) fn with_switcher<'a>(&'a self, body: Element<'a, Message>) -> Element<'a, Message> {
        let (Some(s), Some(w)) = (&self.switcher, self.workspace()) else {
            return body;
        };
        let view = View {
            query: &s.query,
            results: &s.results,
            candidates: &w.candidates,
            selected: s.selected,
            no_recent: w.state.recent.is_empty(),
            on_input: Message::SwitcherInput,
            on_submit: Message::SwitcherSubmit,
            on_pick: Message::SwitcherPick,
        };
        stack![
            body,
            container("")
                .width(Length::Fill)
                .height(Length::Fill)
                .style(styles::scrim),
            container(view.view())
                .width(Length::Fill)
                .height(Length::Fill)
                .align_x(iced::Alignment::Center)
                .padding(Padding {
                    top: SIZE_SWITCHER_TOP - SIZE_BUFFERLINE_H,
                    ..Padding::ZERO
                }),
        ]
        .into()
    }
}
