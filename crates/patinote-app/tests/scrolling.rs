//! Lists follow the keyboard: moving the selection in the explorer (F-10)
//! or the side panel (F-09.8) scrolls it into view. Lays the app out
//! headless and runs the widget operations the keys return.

mod common;

use std::borrow::Cow;
use std::time::Instant;

use common::{Env, leader, press_held, send};
use iced::advanced::graphics::text::font_system;
use iced::advanced::renderer::Headless;
use iced::advanced::widget::Id;
use iced::advanced::widget::operation::{Operation, Outcome, Scrollable};
use iced::{Event, Pixels, Rectangle, Size, Task, Vector, mouse, window};
use iced_runtime::Action;
use iced_runtime::futures::futures::{StreamExt, executor::block_on};
use iced_runtime::user_interface::{self, UserInterface};
use patinote_app::app::{App, Message};
use patinote_app::theme::tokens::TYPE_UI;
use patinote_core::config::Config;
use patinote_core::ipc::Request;

/// The window, small enough that the lists overflow.
fn window() -> Size {
    Size::new(f32::from(1200u16), f32::from(600u16))
}

struct Ui {
    renderer: iced::Renderer,
    cache: Option<user_interface::Cache>,
}

impl Ui {
    fn new() -> Self {
        for font in patinote_app::fonts::FONTS {
            font_system()
                .write()
                .unwrap()
                .load_font(Cow::Borrowed(font));
        }
        let renderer = block_on(<iced::Renderer as Headless>::new(
            patinote_app::fonts::DEFAULT,
            Pixels(TYPE_UI.size),
            Some("tiny-skia"),
        ))
        .expect("a headless renderer");
        Self {
            renderer,
            cache: None,
        }
    }

    /// Lays out the app's view and runs `op` on it, with what it chains.
    fn operate(&mut self, app: &App, op: Option<Box<dyn Operation>>) {
        let mut ui = UserInterface::build(
            app.view(),
            window(),
            self.cache.take().unwrap_or_default(),
            &mut self.renderer,
        );
        let mut messages: Vec<Message> = Vec::new();
        let _ = ui.update(
            &[Event::Window(
                window::Event::RedrawRequested(Instant::now()),
            )],
            mouse::Cursor::Unavailable,
            &mut self.renderer,
            &mut iced::advanced::clipboard::Null,
            &mut messages,
        );
        let mut next = op;
        while let Some(mut op) = next.take() {
            ui.operate(&self.renderer, op.as_mut());
            if let Outcome::Chain(chained) = op.finish() {
                next = Some(chained);
            }
        }
        self.cache = Some(ui.into_cache());
    }

    /// Runs a key's task: widget operations on the laid-out view, messages
    /// through the app.
    fn run(&mut self, app: &mut App, task: Task<Message>) {
        let mut queue = vec![task];
        while let Some(task) = queue.pop() {
            let Some(mut stream) = iced_runtime::task::into_stream(task) else {
                continue;
            };
            while let Some(action) = block_on(stream.next()) {
                match action {
                    Action::Output(message) => queue.push(app.update(message)),
                    Action::Widget(op) => self.operate(app, Some(op)),
                    _ => {}
                }
            }
        }
    }

    fn press(&mut self, app: &mut App, key: &str) {
        let task = press_held(app, key);
        self.run(app, task);
    }

    /// Whether the container `item` is inside the visible part of the
    /// scrollable `list`.
    fn shows(&mut self, app: &App, list: &'static str, item: &'static str) -> bool {
        let mut probe = Probe {
            list: Id::new(list),
            item: Id::new(item),
            view: None,
            found: None,
        };
        self.operate(app, None);
        let mut ui = UserInterface::build(
            app.view(),
            window(),
            self.cache.take().unwrap_or_default(),
            &mut self.renderer,
        );
        ui.operate(&self.renderer, &mut probe);
        self.cache = Some(ui.into_cache());
        let ((bounds, content, offset), found) = (
            probe.view.expect("the list is drawn"),
            probe.found.expect("the item is drawn"),
        );
        let top = found.y - content.y - offset.y;
        // Iced scrolls by whole pixels, so up to a pixel of the last item can
        // stay past the end.
        top > -1.0 && top + found.height < bounds.height + 1.0
    }
}

struct Probe {
    list: Id,
    item: Id,
    view: Option<(Rectangle, Rectangle, Vector)>,
    found: Option<Rectangle>,
}

impl Operation for Probe {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
        operate(self);
    }

    fn scrollable(
        &mut self,
        id: Option<&Id>,
        bounds: Rectangle,
        content: Rectangle,
        offset: Vector,
        _state: &mut dyn Scrollable,
    ) {
        if id == Some(&self.list) {
            self.view = Some((bounds, content, offset));
        }
    }

    fn container(&mut self, id: Option<&Id>, bounds: Rectangle) {
        if id == Some(&self.item) {
            self.found = Some(bounds);
        }
    }
}

/// The mockup vault with a note of sixty headings and sixty notes that
/// link to it.
fn start(env: &Env) -> App {
    let vault = env.vault();
    for i in 0..60 {
        std::fs::write(vault.join(format!("Catatan {i:02}.md")), "[[Panjang]]\n").unwrap();
    }
    let headings: String = (0..60)
        .map(|i| format!("## Bagian {i:02}\n\nisi\n\n"))
        .collect();
    std::fs::write(vault.join("Panjang.md"), headings).unwrap();
    env.start(env.launch(Some(vault), Config::default()))
}

#[test]
fn f10_moving_down_the_explorer_keeps_the_selection_in_view() {
    let env = Env::new();
    let mut app = start(&env);
    let mut ui = Ui::new();
    leader(&mut app, "e");
    assert!(app.explorer_is_open());
    for _ in 0..70 {
        ui.press(&mut app, "j");
    }
    assert!(ui.shows(&app, "explorer-list", "explorer-selected"));
    for _ in 0..70 {
        ui.press(&mut app, "k");
    }
    assert!(ui.shows(&app, "explorer-list", "explorer-selected"));
}

#[test]
fn f09_8_moving_down_the_outline_keeps_the_focus_in_view() {
    let env = Env::new();
    let mut app = start(&env);
    let mut ui = Ui::new();
    send(
        &mut app,
        Message::Remote(Request::Open {
            path: "Panjang.md".into(),
        }),
    );
    leader(&mut app, "o");
    assert!(app.panel_focused());
    for _ in 0..59 {
        ui.press(&mut app, "j");
    }
    assert!(ui.shows(&app, "panel-list", "panel-focused"));
    for _ in 0..59 {
        ui.press(&mut app, "k");
    }
    assert!(ui.shows(&app, "panel-list", "panel-focused"));
}

#[test]
fn f09_8_moving_down_the_backlinks_keeps_the_focus_in_view() {
    let env = Env::new();
    let mut app = start(&env);
    let mut ui = Ui::new();
    send(
        &mut app,
        Message::Remote(Request::Open {
            path: "Panjang.md".into(),
        }),
    );
    leader(&mut app, "b");
    assert!(app.panel_focused());
    for _ in 0..59 {
        ui.press(&mut app, "j");
    }
    assert!(ui.shows(&app, "panel-list", "panel-focused"));
    for _ in 0..59 {
        ui.press(&mut app, "k");
    }
    assert!(ui.shows(&app, "panel-list", "panel-focused"));
}
