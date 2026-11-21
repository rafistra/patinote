//! Reading mode in the app (F-27): switching with `<leader>r` at the same
//! place, keys to scroll and move between links, links to missing notes,
//! images shrunk to the column off the UI thread, and checkboxes that save.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use iced::keyboard::{Key, Modifiers, key::Named};
use iced::widget::operation::{self, AbsoluteOffset, RelativeOffset};
use iced::{Element, Task};
use patinote_core::Resolver;
use patinote_core::model::LinkKind;

use super::{App, Message};
use crate::reading::{self, Block};
use crate::screens::reading::{self as view, Picture};
use crate::theme::tokens::*;

/// A note shown in reading mode.
#[derive(Debug, Clone)]
pub struct ReadState {
    pub doc: reading::Document,
    /// The buffer revision `doc` was rendered from.
    pub rev: u64,
    pub focus: Option<usize>,
    pub missing: HashSet<String>,
    pub pictures: HashMap<String, Picture>,
    /// How far down the view is scrolled, 0 to 1.
    pub scroll: f32,
    /// `g` was pressed, waiting for a second `g`.
    pub pending_g: bool,
}

impl App {
    /// Whether the active note is in reading mode, for tests.
    pub fn reading(&self) -> bool {
        self.read_state().is_some()
    }

    /// The link targets of the note shown in reading mode, for tests.
    pub fn reading_targets(&self) -> Vec<String> {
        self.read_state()
            .map(|r| r.doc.links.iter().map(|l| l.target.clone()).collect())
            .unwrap_or_default()
    }

    /// The link `Tab` selected in reading mode, for tests.
    pub fn reading_focus(&self) -> Option<usize> {
        self.read_state()?.focus
    }

    pub(super) fn read_state(&self) -> Option<&ReadState> {
        self.workspace()?.active_buffer()?.read.as_ref()
    }

    fn read_state_mut(&mut self) -> Option<&mut ReadState> {
        let w = self.workspace_mut()?;
        let i = w.active?;
        w.buffers.get_mut(i)?.read.as_mut()
    }

    /// `<leader>r`: reading mode on or off, at the same place (F-27.6).
    pub(super) fn toggle_reading(&mut self) -> Task<Message> {
        let Some(w) = self.workspace_mut() else {
            return Task::none();
        };
        let Some(i) = w.active else {
            return Task::none();
        };
        let b = &mut w.buffers[i];
        let Some(doc) = b.doc.as_mut() else {
            return Task::none();
        };
        let lines = doc.buffer.lines().len().max(1);
        if let Some(r) = b.read.take() {
            // Back to editing: the cursor goes to the part that was read.
            let line = ((r.scroll * (lines - 1) as f32).round() as usize).min(lines - 1);
            doc.buffer.set_cursor(crate::vim::Pos::new(line, 0));
            return Task::none();
        }
        let fraction = doc.buffer.cursor().line as f32 / (lines - 1).max(1) as f32;
        let rendered = reading::render(&doc.buffer.text());
        let rev = doc.buffer.revision();
        b.read = Some(ReadState {
            doc: rendered,
            rev,
            focus: None,
            missing: HashSet::new(),
            pictures: HashMap::new(),
            scroll: fraction,
            pending_g: false,
        });
        let path = b.path.clone();
        Task::batch([
            operation::snap_to(
                view::SCROLL,
                RelativeOffset {
                    x: 0.0,
                    y: fraction,
                },
            ),
            self.load_reading_links(path),
        ])
    }

    /// Renders the active note again after its text changed.
    pub(super) fn refresh_reading(&mut self) {
        let Some(w) = self.workspace_mut() else {
            return;
        };
        let Some(i) = w.active else { return };
        let b = &mut w.buffers[i];
        if let (Some(doc), Some(r)) = (&b.doc, &mut b.read)
            && r.rev != doc.buffer.revision()
        {
            r.doc = reading::render(&doc.buffer.text());
            r.rev = doc.buffer.revision();
        }
    }

    /// Finds which links point nowhere and loads the images.
    fn load_reading_links(&self, path: String) -> Task<Message> {
        let (Some(w), Some(r)) = (self.workspace(), self.read_state()) else {
            return Task::none();
        };
        let targets: Vec<(String, bool)> = r
            .doc
            .links
            .iter()
            .map(|l| (l.target.clone(), l.markdown))
            .collect();
        let images: Vec<String> = r
            .doc
            .blocks
            .iter()
            .filter_map(|b| match b {
                Block::Image { target, .. } => Some(target.clone()),
                _ => None,
            })
            .collect();
        let root = w.session.paths.root.clone();
        let source = path.clone();
        Task::perform(
            w.session.query(move |store| {
                let resolve = |target: &str, markdown: bool| -> Option<String> {
                    // A Markdown link's target is already the path alone; a
                    // `#` or `|` in it belongs to the file name.
                    let clean = if markdown {
                        target.trim()
                    } else {
                        target.split(['#', '|']).next().unwrap_or(target).trim()
                    };
                    if clean.starts_with("http://")
                        || clean.starts_with("https://")
                        || clean.is_empty()
                    {
                        return Some(clean.to_owned());
                    }
                    let kind = if markdown {
                        LinkKind::Markdown
                    } else {
                        LinkKind::Wiki
                    };
                    let candidates = store.link_candidates(clean).unwrap_or_default();
                    Resolver::new(candidates)
                        .resolve(&source, kind, clean)
                        .map(|r| r.path.to_owned())
                };
                let missing: HashSet<String> = targets
                    .iter()
                    .filter(|(t, m)| resolve(t, *m).is_none())
                    .map(|(t, _)| t.clone())
                    .collect();
                let pictures: Vec<(String, Option<PathBuf>)> = images
                    .iter()
                    .map(|t| {
                        let file =
                            resolve(t, t.contains('/') || t.contains('.') && !t.contains("[["))
                                .map(|p| root.join(p))
                                .filter(|p| p.is_file());
                        (t.clone(), file)
                    })
                    .collect();
                (missing, pictures)
            }),
            move |found| {
                let (missing, pictures) = found.unwrap_or_default();
                Message::ReadingLinks(path.clone(), missing, pictures)
            },
        )
    }

    /// Links that point nowhere arrived; images start decoding.
    pub(super) fn reading_links(
        &mut self,
        path: String,
        missing: HashSet<String>,
        pictures: Vec<(String, Option<PathBuf>)>,
    ) -> Task<Message> {
        let width = (SIZE_READ_MAXW * self.scale).round() as u32;
        let Some(w) = self.workspace_mut() else {
            return Task::none();
        };
        let Some(r) = w
            .buffers
            .iter_mut()
            .find(|b| b.path == path)
            .and_then(|b| b.read.as_mut())
        else {
            return Task::none();
        };
        r.missing = missing;
        let mut tasks = Vec::new();
        for (target, file) in pictures {
            match file {
                None => {
                    r.pictures.insert(target, Picture::Missing);
                }
                Some(file) => {
                    r.pictures.insert(target.clone(), Picture::Loading);
                    let p = path.clone();
                    tasks.push(Task::future(async move {
                        Message::Picture(p, target, decode(&file, width))
                    }));
                }
            }
        }
        Task::batch(tasks)
    }

    pub(super) fn picture(
        &mut self,
        path: String,
        target: String,
        handle: Option<iced::widget::image::Handle>,
    ) {
        if let Some(r) = self
            .workspace_mut()
            .and_then(|w| w.buffers.iter_mut().find(|b| b.path == path))
            .and_then(|b| b.read.as_mut())
        {
            r.pictures
                .insert(target, handle.map_or(Picture::Missing, Picture::Ready));
        }
    }

    /// Keys in reading mode (F-27).
    pub(super) fn reading_key(
        &mut self,
        key: &Key,
        text: Option<&str>,
        modifiers: Modifiers,
    ) -> Task<Message> {
        let line = TYPE_READ_BODY.line_height;
        let half = f32::midpoint(0.0, self.window_height).max(line);
        let links = self.read_state().map_or(0, |r| r.doc.links.len());
        let Some(r) = self.read_state_mut() else {
            return Task::none();
        };
        let pending_g = std::mem::take(&mut r.pending_g);
        let ctrl = |c: &str| modifiers.control() && *key == Key::Character(c.into());
        let by = |y: f32| operation::scroll_by(view::SCROLL, AbsoluteOffset { x: 0.0, y });
        match (key, text) {
            _ if ctrl("d") => by(half),
            _ if ctrl("u") => by(-half),
            (Key::Named(Named::ArrowDown), _) | (_, Some("j")) => by(line),
            (Key::Named(Named::ArrowUp), _) | (_, Some("k")) => by(-line),
            (_, Some("G")) => operation::snap_to(view::SCROLL, RelativeOffset { x: 0.0, y: 1.0 }),
            (_, Some("g")) if pending_g => {
                operation::snap_to(view::SCROLL, RelativeOffset { x: 0.0, y: 0.0 })
            }
            (_, Some("g")) => {
                r.pending_g = true;
                Task::none()
            }
            (Key::Named(Named::Tab), _) if links > 0 => {
                r.focus = Some(match (r.focus, modifiers.shift()) {
                    (None, false) => 0,
                    (None, true) => links - 1,
                    (Some(i), false) => (i + 1) % links,
                    (Some(i), true) => (i + links - 1) % links,
                });
                Task::none()
            }
            (Key::Named(Named::Enter), _) => match r.focus {
                Some(i) => self.reading_link(i),
                None => Task::none(),
            },
            (_, Some(":")) => self.open_prompt(super::Prompt::Command),
            _ => Task::none(),
        }
    }

    /// Follows link `i` of the note in reading mode.
    pub(super) fn reading_link(&mut self, i: usize) -> Task<Message> {
        let Some((link, source)) = self
            .read_state()
            .and_then(|r| r.doc.links.get(i).cloned())
            .zip(self.active_document().map(|d| d.path.clone()))
        else {
            return Task::none();
        };
        if let Some(r) = self.read_state_mut() {
            r.focus = Some(i);
        }
        let target = link.target.trim();
        if target.starts_with("http://") || target.starts_with("https://") {
            if let Err(e) = crate::fonts::external_command("xdg-open")
                .arg(target)
                .spawn()
            {
                self.say(
                    super::Level::Error,
                    format!("Can't open the default app: {e}"),
                );
            }
            return Task::none();
        }
        let (name, sub) = if link.markdown {
            (target.to_owned(), link.subpath.clone())
        } else {
            match target.split('|').next().unwrap_or(target).split_once('#') {
                Some((n, s)) => (n.to_owned(), Some(s.to_owned())),
                None => (target.split('|').next().unwrap_or(target).to_owned(), None),
            }
        };
        let kind = if link.markdown {
            LinkKind::Markdown
        } else {
            LinkKind::Wiki
        };
        self.resolve_link(source, name, kind, sub, 0)
    }

    /// A checkbox in reading mode: the task's box changes in the file at
    /// once; nothing else does (F-27.3).
    pub(super) fn toggle_task(&mut self, line: usize) -> Task<Message> {
        let Some(doc) = self.active_document_mut() else {
            return Task::none();
        };
        if doc.read_only.is_some() {
            return Task::none();
        }
        let Some(new) = doc
            .buffer
            .lines()
            .get(line)
            .and_then(|l| reading::toggle_task(l))
        else {
            return Task::none();
        };
        if doc.vim.replace_line(&mut doc.buffer, line, &new) {
            doc.edited(std::time::Instant::now());
            doc.kinds();
        }
        self.refresh_reading();
        self.autosave(true)
    }

    /// The reading view of the active note, if it is in reading mode.
    pub(super) fn reading_view(&self) -> Option<Element<'_, Message>> {
        let r = self.read_state()?;
        Some(
            view::View {
                doc: &r.doc,
                missing: &r.missing,
                focused: r.focus,
                pictures: &r.pictures,
                properties_open: self.properties_open,
                on_link: Message::ReadLink,
                on_toggle: Message::ToggleTask,
                on_properties: Message::ToggleProperties,
                on_scroll: Message::ReadScrolled,
            }
            .view(),
        )
    }

    pub(super) fn read_scrolled(&mut self, y: f32) {
        if let Some(r) = self.read_state_mut() {
            r.scroll = y.clamp(0.0, 1.0);
        }
    }
}

/// Decodes an image and shrinks it to `width` pixels when wider, so only
/// the bitmap that shows stays in memory (F-27.4).
fn decode(file: &std::path::Path, width: u32) -> Option<iced::widget::image::Handle> {
    let img = image::open(file).ok()?;
    let img = if img.width() > width {
        let height =
            (u64::from(img.height()) * u64::from(width) / u64::from(img.width().max(1))) as u32;
        img.resize_exact(width, height.max(1), image::imageops::FilterType::Triangle)
    } else {
        img
    };
    let rgba = img.into_rgba8();
    let (w, h) = rgba.dimensions();
    Some(iced::widget::image::Handle::from_rgba(
        w,
        h,
        rgba.into_raw(),
    ))
}

/// The bitmap size a decoded image keeps, for tests: (width, height).
pub fn decoded_size(file: &std::path::Path, width: u32) -> Option<(u32, u32)> {
    let img = image::open(file).ok()?;
    if img.width() > width {
        Some((
            width,
            (u64::from(img.height()) * u64::from(width) / u64::from(img.width().max(1))) as u32,
        ))
    } else {
        Some((img.width(), img.height()))
    }
}
